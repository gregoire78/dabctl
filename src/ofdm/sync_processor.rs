use crate::ofdm::ofdm_processor::{OfdmSyncDetector, PipelineReport, SyncCandidate};
use rustfft::num_complex::Complex32;

const IQ_BYTES_PER_SAMPLE: usize = 2;

impl OfdmSyncDetector {
    pub fn new(fft_len: usize, cp_len: usize, threshold: f32) -> Self {
        Self {
            fft_len,
            cp_len,
            symbol_len: fft_len + cp_len,
            threshold,
            tail: Vec::new(),
            total_samples_seen: 0,
            predicted_next_symbol_sample: None,
            cfo_phase_smoothed: 0.0,
            lock_misses: 0,
        }
    }

    pub fn inspect(&mut self, chunk: &[u8]) -> PipelineReport {
        if chunk.len() < IQ_BYTES_PER_SAMPLE {
            return empty_report(None, 0);
        }

        let mut merged = Vec::with_capacity(self.tail.len() + chunk.len());
        merged.extend_from_slice(&self.tail);
        merged.extend_from_slice(chunk);

        let sample_count = merged.len() / IQ_BYTES_PER_SAMPLE;
        let mut best = None;
        let mut best_metric = 0.0f32;

        if sample_count >= self.symbol_len {
            let max_offset = sample_count - self.symbol_len;

            // 1) Fast tracking around predicted next symbol if available.
            if let Some(predicted_abs) = self.predicted_next_symbol_sample {
                let merged_start_abs = self.total_samples_seen.saturating_sub(self.tail.len() / IQ_BYTES_PER_SAMPLE);
                let pred_local = predicted_abs.saturating_sub(merged_start_abs);
                let search_half = 32usize;
                let start = pred_local.saturating_sub(search_half).min(max_offset);
                let end = pred_local.saturating_add(search_half).min(max_offset);
                let mut offset = start;
                while offset <= end {
                    let (metric, cfo_phase_per_sample) = self.compute_cp_stats(&merged, offset);
                    if metric > (self.threshold * 0.85) && metric > best_metric {
                        best_metric = metric;
                        best = Some(SyncCandidate {
                            sample_offset: self.total_samples_seen + offset,
                            metric,
                            cfo_phase_per_sample,
                        });
                    }
                    offset += 1;
                }
            }

            // 2) Full scan fallback with decimation (lower CPU).
            if best.is_none() {
                let mut offset = 0usize;
                while offset <= max_offset {
                    let (metric, cfo_phase_per_sample) = self.compute_cp_stats(&merged, offset);
                    if metric > self.threshold && metric > best_metric {
                        best_metric = metric;
                        best = Some(SyncCandidate {
                            sample_offset: self.total_samples_seen + offset,
                            metric,
                            cfo_phase_per_sample,
                        });
                    }
                    offset += 4;
                }
            }

            // 3) Local refinement around coarse best.
            if let Some(mut coarse) = best {
                let local = coarse.sample_offset.saturating_sub(self.total_samples_seen);
                let start = local.saturating_sub(3).min(max_offset);
                let end = local.saturating_add(3).min(max_offset);
                for off in start..=end {
                    let (metric, cfo_phase_per_sample) = self.compute_cp_stats(&merged, off);
                    if metric > coarse.metric {
                        coarse = SyncCandidate {
                            sample_offset: self.total_samples_seen + off,
                            metric,
                            cfo_phase_per_sample,
                        };
                    }
                }
                best = Some(coarse);
            }
        }

        if let Some(mut sync) = best {
            // IIR smoothing on CFO estimate to reduce jitter injected downstream.
            let alpha = 0.2f32;
            self.cfo_phase_smoothed = (1.0 - alpha) * self.cfo_phase_smoothed + alpha * sync.cfo_phase_per_sample;
            sync.cfo_phase_per_sample = self.cfo_phase_smoothed;
            self.predicted_next_symbol_sample = Some(sync.sample_offset + self.symbol_len);
            self.lock_misses = 0;
            best = Some(sync);
        } else {
            self.lock_misses = self.lock_misses.saturating_add(1);
            if self.lock_misses >= 8 {
                self.predicted_next_symbol_sample = None;
                self.cfo_phase_smoothed = 0.0;
            } else if let Some(pred) = self.predicted_next_symbol_sample {
                self.predicted_next_symbol_sample = Some(pred + self.symbol_len);
            }
        }

        let tail_keep_samples = self.symbol_len.min(sample_count);
        let tail_keep_bytes = tail_keep_samples * IQ_BYTES_PER_SAMPLE;
        self.tail.clear();
        self.tail
            .extend_from_slice(&merged[merged.len().saturating_sub(tail_keep_bytes)..]);

        self.total_samples_seen += chunk.len() / IQ_BYTES_PER_SAMPLE;

        empty_report(best, sample_count)
    }

    pub fn compute_cp_metric(&self, iq: &[u8], sample_offset: usize) -> f32 {
        self.compute_cp_stats(iq, sample_offset).0
    }

    pub fn compute_cp_stats(&self, iq: &[u8], sample_offset: usize) -> (f32, f32) {
        let a_start = sample_offset;
        let b_start = sample_offset + self.fft_len;
        let pairs = self.cp_len;

        let mut corr_re = 0.0f64;
        let mut corr_im = 0.0f64;
        let mut energy_a = 0.0f64;
        let mut energy_b = 0.0f64;

        for k in 0..pairs {
            let a_i = sample_i(iq, a_start + k, 0) as f64;
            let a_q = sample_i(iq, a_start + k, 1) as f64;
            let b_i = sample_i(iq, b_start + k, 0) as f64;
            let b_q = sample_i(iq, b_start + k, 1) as f64;

            corr_re += a_i * b_i + a_q * b_q;
            corr_im += a_i * b_q - a_q * b_i;
            energy_a += a_i * a_i + a_q * a_q;
            energy_b += b_i * b_i + b_q * b_q;
        }

        let denom = (energy_a * energy_b).sqrt() + 1e-9;
        let metric = ((corr_re * corr_re + corr_im * corr_im).sqrt() / denom) as f32;
        let phase = corr_im.atan2(corr_re) as f32;
        (metric, phase / self.fft_len as f32)
    }
}

fn empty_report(sync_candidate: Option<SyncCandidate>, inspected_samples: usize) -> PipelineReport {
    PipelineReport {
        sync_candidate,
        sync_cfo_phase_per_sample: sync_candidate.map(|sync| sync.cfo_phase_per_sample),
        prs_eq_mse: None,
        prs_eq_phase_rms_rad: None,
        prs_channel_gain_avg: None,
        prs_channel_gain_spread_db: None,
        inspected_samples,
        aligned_symbols: 0,
        last_symbol_start: None,
        completed_frames: 0,
        last_frame_start: None,
        frequency_frames: 0,
        last_frequency_frame_start: None,
        mapped_frames: 0,
        last_mapped_frame_start: None,
        normalized_frames: 0,
        fic_candidates: 0,
        fic_bitstreams: 0,
        last_fic_bit_count: None,
        fic_deinterleaved: 0,
        fic_segments: 0,
        fic_blocks: 0,
        fic_crc_ok: 0,
        fib_candidates: 0,
        fig_candidates: 0,
        fig_type0: 0,
        fig_type1: 0,
        fig_type0_unique_extensions: 0,
        last_fig0_extension: None,
        multiplex_updates: 0,
        eti_frames_built: 0,
        eti_frames_emitted: 0,
        eti_fic_cache_valid: false,
    }
}

pub fn apply_cfo_correction(samples: &mut [Complex32], phase_per_sample: f32) {
    if phase_per_sample == 0.0 {
        return;
    }

    for (index, sample) in samples.iter_mut().enumerate() {
        let phase = -phase_per_sample * index as f32;
        let rotation = Complex32::new(phase.cos(), phase.sin());
        *sample *= rotation;
    }
}

#[inline]
fn sample_i(iq: &[u8], sample_idx: usize, iq_idx: usize) -> i16 {
    let byte_idx = sample_idx * IQ_BYTES_PER_SAMPLE + iq_idx;
    iq[byte_idx] as i16 - 128
}

pub fn iq_bytes_to_complex(iq: &[u8]) -> Vec<Complex32> {
    iq.chunks_exact(IQ_BYTES_PER_SAMPLE)
        .map(|sample| Complex32::new(sample[0] as f32 - 128.0, sample[1] as f32 - 128.0))
        .collect()
}
