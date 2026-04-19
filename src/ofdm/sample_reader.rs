use anyhow::Result;
use num_complex::Complex32;
use tracing::debug;

use crate::device::RtlSdrDevice;

// Literal receive-side helper mirroring DABstar's sample-reader stage.
// BB frequency rotation is NOT applied here; it is applied per-symbol in
// DabProcessor so that buf always holds unrotated DC-corrected IQ samples.
pub struct SampleReader {
    device: RtlSdrDevice,
    scratch: Vec<u8>,
    mean_i: f32,
    mean_q: f32,
    mean_ii: f32,
    mean_qq: f32,
    mean_iq: f32,
    signal_level: f32,
    do_dc_or_iq_corr: bool,
    do_iq_corr: bool,
    blocks_read: u64,
}

impl SampleReader {
    pub fn new(device: RtlSdrDevice) -> Self {
        Self {
            device,
            scratch: Vec::new(),
            mean_i: 0.0,
            mean_q: 0.0,
            mean_ii: 1.0,
            mean_qq: 1.0,
            mean_iq: 0.0,
            signal_level: 0.0,
            do_dc_or_iq_corr: true,
            do_iq_corr: false,
            blocks_read: 0,
        }
    }

    pub fn signal_level(&self) -> f32 {
        self.signal_level
    }

    pub fn set_dc_and_iq_correction(&mut self, do_dc_corr: bool, do_iq_corr: bool) {
        if !do_dc_corr && !do_iq_corr {
            self.mean_i = 0.0;
            self.mean_q = 0.0;
            self.mean_ii = 1.0;
            self.mean_qq = 1.0;
            self.mean_iq = 0.0;
        }
        self.do_dc_or_iq_corr = do_dc_corr || do_iq_corr;
        self.do_iq_corr = do_iq_corr;
    }

    pub fn center_freq_hz(&self) -> u32 {
        self.device.center_freq_hz()
    }

    pub fn set_center_freq_hz(&mut self, center_freq_hz: u32) -> Result<()> {
        self.device.set_center_freq_hz(center_freq_hz)
    }

    pub fn reset_buffer(&mut self) -> Result<()> {
        self.device.reset_buffer()
    }

    pub fn read_iq_block(&mut self, byte_len: usize) -> Result<Vec<Complex32>> {
        self.scratch.resize(byte_len, 0);
        let n_read = self.device.read_sync(&mut self.scratch)?;
        let mut iq = Vec::with_capacity(n_read / 2);
        let mut sum_power = 0.0f32;
        let mut sum_abs = 0.0f32;
        let mut peak_abs = 0.0f32;
        let mut clipped_samples = 0usize;

        for chunk in self.scratch[..n_read].chunks_exact(2) {
            if chunk[0] <= 1 || chunk[0] >= 254 || chunk[1] <= 1 || chunk[1] >= 254 {
                clipped_samples += 1;
            }

            let mut i = (f32::from(chunk[0]) - 127.38) / 128.0;
            let mut q = (f32::from(chunk[1]) - 127.38) / 128.0;

            if self.do_dc_or_iq_corr {
                const ALPHA: f32 = 1.0 / 2_048_000.0;
                mean_filter(&mut self.mean_i, i, ALPHA);
                mean_filter(&mut self.mean_q, q, ALPHA);

                if self.do_iq_corr {
                    let x_i = i - self.mean_i;
                    let x_q = q - self.mean_q;
                    mean_filter(&mut self.mean_ii, x_i * x_i, ALPHA);
                    mean_filter(&mut self.mean_iq, x_i * x_q, ALPHA);
                    let phi = if self.mean_ii.abs() > f32::EPSILON {
                        self.mean_iq / self.mean_ii
                    } else {
                        0.0
                    };
                    let x_q_corr = x_q - phi * x_i;
                    mean_filter(&mut self.mean_qq, x_q_corr * x_q_corr, ALPHA);
                    let gain_q = if self.mean_qq > f32::EPSILON {
                        (self.mean_ii / self.mean_qq).sqrt()
                    } else {
                        1.0
                    };
                    i = x_i;
                    q = x_q_corr * gain_q;
                } else {
                    i -= self.mean_i;
                    q -= self.mean_q;
                }
            }

            let v_abs = Complex32::new(i, q).norm();
            sum_power += i * i + q * q;
            sum_abs += v_abs;
            peak_abs = peak_abs.max(i.abs()).max(q.abs());
            iq.push(Complex32::new(i, q));
        }

        self.blocks_read = self.blocks_read.saturating_add(1);
        if !iq.is_empty() {
            let rms = (sum_power / iq.len() as f32).sqrt();
            let mean_abs = sum_abs / iq.len() as f32;
            let alpha = (1.0f32 - (1.0f32 - 0.00001f32).powi(iq.len() as i32)).clamp(0.0, 1.0);
            mean_filter(&mut self.signal_level, mean_abs, alpha);
            let clip_ratio = clipped_samples as f32 / iq.len() as f32;
            if self.blocks_read <= 4 || self.blocks_read.is_multiple_of(16) || clip_ratio > 0.001 {
                debug!(
                    block = self.blocks_read,
                    samples = iq.len(),
                    rms,
                    peak_abs,
                    clip_ratio,
                    signal_level = self.signal_level,
                    dc_i = self.mean_i,
                    dc_q = self.mean_q,
                    iq_gain_balance = if self.mean_qq > f32::EPSILON {
                        (self.mean_ii / self.mean_qq).sqrt()
                    } else {
                        1.0
                    },
                    "AGC input monitor"
                );
            }
        }

        Ok(iq)
    }
}

fn mean_filter(target: &mut f32, value: f32, alpha: f32) {
    *target += alpha * (value - *target);
}
