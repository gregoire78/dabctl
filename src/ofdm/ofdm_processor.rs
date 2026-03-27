// OFDM processor - converted from ofdm-processor.cpp (eti-cmdline)

use num_complex::Complex32;
use rustfft::{FftPlanner, Fft};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::dab_constants::{jan_abs, INPUT_RATE, DIFF_LENGTH};
use crate::support::dab_params::DabParams;
use crate::ofdm::phase_reference::PhaseReference;
use crate::ofdm::freq_interleaver::FreqInterleaver;
use crate::eti_handling::eti_generator::EtiGenerator;
use crate::device::rtlsdr_handler::RtlsdrHandler;

pub struct OfdmProcessor {
    _params: DabParams,
    t_null: usize,
    t_s: usize,
    t_u: usize,
    t_g: usize,
    t_f: usize,
    nr_blocks: usize,
    carriers: usize,
    carrier_diff: i32,
    phase_synchronizer: PhaseReference,
    freq_interleaver: FreqInterleaver,
    fft: Arc<dyn Fft<f32>>,
    fft_buffer: Vec<Complex32>,
    reference_phase: Vec<Complex32>,
    ofdm_buffer: Vec<Complex32>,
    nco_phase: f32,
    fine_corrector: f32,
    coarse_corrector: i32,
    f2_correction: bool,
    s_level: f32,
    buffer_content: i32,
    running: Arc<AtomicBool>,
    // Callbacks
    sync_signal: Option<Box<dyn Fn(bool) + Send>>,
    show_snr: Option<Box<dyn Fn(i16) + Send>>,
}

/// Errors that cause the processor to exit 
pub enum ProcessorError {
    Stopped,
}

impl OfdmProcessor {
    pub fn new(
        dab_mode: u8,
        _threshold_1: i16,
        _threshold_2: i16,
        running: Arc<AtomicBool>,
    ) -> Self {
        let params = DabParams::new(dab_mode);
        let t_u = params.t_u as usize;
        let t_s = params.t_s as usize;
        let t_g = params.t_g as usize;
        let t_null = params.t_null as usize;
        let t_f = params.t_f as usize;
        let nr_blocks = params.l as usize;
        let carriers = params.k as usize;
        let carrier_diff = params.carrier_diff;

        let freq_interleaver = FreqInterleaver::new(&params);
        let phase_synchronizer = PhaseReference::new(
            t_u, carriers, params.dab_mode, DIFF_LENGTH as usize,
        );

        let mut planner = FftPlanner::new();
        let fft = planner.plan_fft_forward(t_u);

        // NCO: no table needed, phase tracked incrementally

        OfdmProcessor {
            _params: params.clone(),
            t_null, t_s, t_u, t_g, t_f, nr_blocks, carriers, carrier_diff,
            phase_synchronizer,
            freq_interleaver,
            fft,
            fft_buffer: vec![Complex32::new(0.0, 0.0); t_u],
            reference_phase: vec![Complex32::new(0.0, 0.0); t_u],
            ofdm_buffer: vec![Complex32::new(0.0, 0.0); 2 * t_s],
            nco_phase: 0.0,
            fine_corrector: 0.0,
            coarse_corrector: 0,
            f2_correction: true,
            s_level: 0.0,
            buffer_content: 0,
            running,
            sync_signal: None,
            show_snr: None,
        }
    }

    pub fn set_sync_signal<F: Fn(bool) + Send + 'static>(&mut self, f: F) {
        self.sync_signal = Some(Box::new(f));
    }

    pub fn set_show_snr<F: Fn(i16) + Send + 'static>(&mut self, f: F) {
        self.show_snr = Some(Box::new(f));
    }

    pub fn sync_reached(&mut self) {
        self.f2_correction = false;
    }

    fn emit_sync_signal(&self, val: bool) {
        if let Some(ref f) = self.sync_signal {
            f(val);
        }
    }

    fn emit_snr(&self, val: i16) {
        if let Some(ref f) = self.show_snr {
            f(val);
        }
    }

    /// Get a single IQ sample from the device, applying frequency correction
    fn get_sample(&mut self, device: &RtlsdrHandler, phase: i32) -> Result<Complex32, ProcessorError> {
        if !self.running.load(Ordering::Relaxed) {
            return Err(ProcessorError::Stopped);
        }

        while self.buffer_content == 0 {
            if !self.running.load(Ordering::Relaxed) {
                return Err(ProcessorError::Stopped);
            }
            self.buffer_content = device.samples() as i32;
            if self.buffer_content == 0 {
                std::thread::sleep(std::time::Duration::from_micros(1000));
            }
        }

        let mut temp = [Complex32::new(0.0, 0.0)];
        device.get_samples(&mut temp);
        self.buffer_content -= 1;

        // Apply frequency correction via NCO
        let delta = -2.0 * std::f32::consts::PI * phase as f32 / INPUT_RATE as f32;
        self.nco_phase += delta;
        // Keep phase in [-PI, PI] to avoid precision loss
        if self.nco_phase > std::f32::consts::PI { self.nco_phase -= 2.0 * std::f32::consts::PI; }
        if self.nco_phase < -std::f32::consts::PI { self.nco_phase += 2.0 * std::f32::consts::PI; }
        let corrected = temp[0] * Complex32::new(self.nco_phase.cos(), self.nco_phase.sin());
        self.s_level = 0.00001 * jan_abs(corrected) + (1.0 - 0.00001) * self.s_level;
        Ok(corrected)
    }

    /// Get N IQ samples with frequency correction
    fn get_samples(&mut self, device: &RtlsdrHandler, v: &mut [Complex32], phase: i32) -> Result<(), ProcessorError> {
        let n = v.len() as i32;
        if !self.running.load(Ordering::Relaxed) {
            return Err(ProcessorError::Stopped);
        }

        while self.buffer_content < n {
            if !self.running.load(Ordering::Relaxed) {
                return Err(ProcessorError::Stopped);
            }
            self.buffer_content = device.samples() as i32;
            if self.buffer_content < n {
                std::thread::sleep(std::time::Duration::from_micros(1000));
            }
        }

        device.get_samples(v);
        self.buffer_content -= n;

        let delta = -2.0 * std::f32::consts::PI * phase as f32 / INPUT_RATE as f32;
        for sample in v.iter_mut() {
            self.nco_phase += delta;
            if self.nco_phase > std::f32::consts::PI { self.nco_phase -= 2.0 * std::f32::consts::PI; }
            if self.nco_phase < -std::f32::consts::PI { self.nco_phase += 2.0 * std::f32::consts::PI; }
            *sample = *sample * Complex32::new(self.nco_phase.cos(), self.nco_phase.sin());
            self.s_level = 0.00001 * jan_abs(*sample) + (1.0 - 0.00001) * self.s_level;
        }
        Ok(())
    }

    /// Demodulate an OFDM data symbol into soft bits (differential QPSK)
    fn process_block(&mut self, inv: &[Complex32], ibits: &mut [i16]) {
        self.fft_buffer[..self.t_u].copy_from_slice(&inv[self.t_g..self.t_g + self.t_u]);
        self.fft.process(&mut self.fft_buffer);

        for i in 0..self.carriers {
            let mut index = self.freq_interleaver.map_in(i) as i32;
            if index < 0 {
                index += self.t_u as i32;
            }
            let index = index as usize;

            let r1 = self.fft_buffer[index] * self.reference_phase[index].conj();
            self.reference_phase[index] = self.fft_buffer[index];
            let ab1 = jan_abs(r1);
            if ab1 > 0.0 {
                ibits[i] = (-r1.re / ab1 * 127.0) as i16;
                ibits[self.carriers + i] = (-r1.im / ab1 * 127.0) as i16;
            }
        }
    }

    /// Main processing loop - runs in its own thread
    /// This is the faithful translation of ofdmProcessor::run() from C++
    #[allow(unused_assignments)]
    pub fn run(&mut self, device: &RtlsdrHandler, eti_generator: &mut EtiGenerator) {
        let sync_buffer_size: usize = 32768;
        let sync_buffer_mask = sync_buffer_size - 1;
        let mut env_buffer = vec![0.0f32; sync_buffer_size];
        let mut sync_buffer_index: usize;
        let mut current_strength: f32;
        let mut attempts: i16 = 0;
        let mut ibits = vec![0i16; 2 * self.carriers];
        let mut snr: f32 = 0.0;
        let mut snr_count = 0;
        let mut null_buf = vec![Complex32::new(0.0, 0.0); self.t_null];
        let mut check_buf = vec![Complex32::new(0.0, 0.0); self.t_u];
        let mut block_buf = vec![Complex32::new(0.0, 0.0); self.t_s];
        let _phase = self.coarse_corrector + self.fine_corrector as i32;

        // Initialize signal level (batch read in chunks)
        self.s_level = 0.0;
        {
            let init_count = self.t_f / 2;
            let mut read = 0;
            while read < init_count {
                let chunk = (init_count - read).min(block_buf.len());
                if self.get_samples(device, &mut block_buf[..chunk], 0).is_err() { return; }
                read += chunk;
            }
        }

        loop { // notSynced loop
            sync_buffer_index = 0;
            current_strength = 0.0;
            self.s_level = 0.0;

            {
                let skip_count = self.t_f;
                let mut read = 0;
                while read < skip_count {
                    let chunk = (skip_count - read).min(block_buf.len());
                    if self.get_samples(device, &mut block_buf[..chunk], 0).is_err() { return; }
                    read += chunk;
                }
            }

            sync_buffer_index = 0;
            current_strength = 0.0;
            for _ in 0..50 {
                let sample = match self.get_sample(device, 0) {
                    Ok(s) => s,
                    Err(_) => return,
                };
                env_buffer[sync_buffer_index] = jan_abs(sample);
                current_strength += env_buffer[sync_buffer_index];
                sync_buffer_index += 1;
            }

            // SyncOnNull: look for the null level (a dip)
            let mut counter = 0i32;
            let phase = self.coarse_corrector + self.fine_corrector as i32;
            loop {
                if current_strength / 50.0 <= 0.50 * self.s_level {
                    break;
                }
                let sample = match self.get_sample(device, phase) {
                    Ok(s) => s,
                    Err(_) => return,
                };
                env_buffer[sync_buffer_index] = jan_abs(sample);
                let old_idx = (sync_buffer_index + sync_buffer_size - 50) & sync_buffer_mask;
                current_strength += env_buffer[sync_buffer_index] - env_buffer[old_idx];
                sync_buffer_index = (sync_buffer_index + 1) & sync_buffer_mask;
                counter += 1;
                if counter > self.t_f as i32 {
                    attempts += 1;
                    if attempts >= 5 {
                        self.emit_sync_signal(false);
                        attempts = 0;
                        break;
                    }
                }
            }
            if counter > self.t_f as i32 && attempts == 0 {
                continue; // notSynced
            }
            if counter > self.t_f as i32 {
                continue;
            }

            // SyncOnEndNull: look for end of null period
            counter = 0;
            loop {
                if current_strength / 50.0 >= 0.75 * self.s_level {
                    break;
                }
                let sample = match self.get_sample(device, phase) {
                    Ok(s) => s,
                    Err(_) => return,
                };
                env_buffer[sync_buffer_index] = jan_abs(sample);
                let old_idx = (sync_buffer_index + sync_buffer_size - 50) & sync_buffer_mask;
                current_strength += env_buffer[sync_buffer_index] - env_buffer[old_idx];
                sync_buffer_index = (sync_buffer_index + 1) & sync_buffer_mask;
                counter += 1;
                if counter > self.t_null as i32 + 50 {
                    break;
                }
            }
            if counter > self.t_null as i32 + 50 {
                continue; // notSynced
            }

            // Read T_u samples for phase synchronization (batch via temp buffer)
            if self.get_samples(device, &mut check_buf[..self.t_u], phase).is_err() { return; }
            self.ofdm_buffer[..self.t_u].copy_from_slice(&check_buf[..self.t_u]);

            let start_index = self.phase_synchronizer.find_index(
                &self.ofdm_buffer[..self.t_u], 2
            );
            if start_index < 0 {
                continue; // notSynced
            }

            // Synchronized - enter the main frame processing loop
            let mut start_index = start_index as usize;
            
            // First sync
            loop {
                // SyncOnPhase: copy remaining data from sync
                let remaining = self.t_u - start_index;
                let tmp: Vec<Complex32> = self.ofdm_buffer[start_index..self.t_u].to_vec();
                self.ofdm_buffer[..remaining].copy_from_slice(&tmp);
                let ofdm_buffer_index = remaining;

                eti_generator.new_frame();

                // Block 0: read remaining samples and process
                let phase = self.coarse_corrector + self.fine_corrector as i32;
                let needed = self.t_u - ofdm_buffer_index;
                // Use check_buf as temp scratch for remaining samples (avoids alloc)
                if self.get_samples(device, &mut check_buf[..needed], phase).is_err() { return; }
                self.ofdm_buffer[ofdm_buffer_index..self.t_u].copy_from_slice(&check_buf[..needed]);
                
                // Process block 0 inline (avoid borrow conflict with to_vec)
                self.fft_buffer[..self.t_u].copy_from_slice(&self.ofdm_buffer[..self.t_u]);
                self.fft.process(&mut self.fft_buffer);
                self.reference_phase[..self.t_u].copy_from_slice(&self.fft_buffer[..self.t_u]);

                if self.f2_correction {
                    let correction = self.phase_synchronizer.estimate_offset(
                        &self.ofdm_buffer[..self.t_u]
                    );
                    if correction != 100 {
                        self.coarse_corrector += correction as i32 * self.carrier_diff;
                        if self.coarse_corrector.abs() > 35000 {
                            self.coarse_corrector = 0;
                        }
                    }
                }

                // Data blocks (symbols 2..L)
                let mut freq_corr = Complex32::new(0.0, 0.0);
                for symbol_count in 2..=(self.nr_blocks as u16) {
                    block_buf.fill(Complex32::new(0.0, 0.0));
                    let phase = self.coarse_corrector + self.fine_corrector as i32;
                    if self.get_samples(device, &mut block_buf, phase).is_err() { return; }
                    
                    // Accumulate frequency correction from cyclic prefix
                    for i in self.t_u..self.t_s {
                        freq_corr += block_buf[i] * block_buf[i - self.t_u].conj();
                    }
                    
                    self.process_block(&block_buf, &mut ibits);
                    eti_generator.process_block(&ibits, symbol_count as i16);
                }

                // Integrate frequency error
                self.fine_corrector += 0.1 * freq_corr.arg() / std::f32::consts::PI
                    * (self.carrier_diff as f32 / 2.0);

                // Skip null symbol and compute SNR
                null_buf.fill(Complex32::new(0.0, 0.0));
                let phase = self.coarse_corrector + self.fine_corrector as i32;
                if self.get_samples(device, &mut null_buf, phase).is_err() { return; }

                let sum: f32 = null_buf.iter().map(|s| s.norm()).sum::<f32>() / self.t_null as f32;
                snr = 0.9 * snr + 0.1 * 20.0 * ((self.s_level + 0.005) / sum).log10();
                snr_count += 1;
                if snr_count > 10 {
                    snr_count = 0;
                    self.emit_snr(snr as i16);
                }

                // Adjust fine/coarse frequency correction
                if self.fine_corrector > self.carrier_diff as f32 / 2.0 {
                    self.coarse_corrector += self.carrier_diff;
                    self.fine_corrector -= self.carrier_diff as f32;
                } else if self.fine_corrector < -(self.carrier_diff as f32 / 2.0) {
                    self.coarse_corrector -= self.carrier_diff;
                    self.fine_corrector += self.carrier_diff as f32;
                }

                // Check_endOfNull: verify sync on next frame
                check_buf.fill(Complex32::new(0.0, 0.0));
                let phase = self.coarse_corrector + self.fine_corrector as i32;
                if self.get_samples(device, &mut check_buf, phase).is_err() { return; }

                start_index = {
                    let idx = self.phase_synchronizer.find_index(&check_buf, 2);
                    if idx < 0 {
                        break; // Lost sync, go back to notSynced
                    }
                    idx as usize
                };
                // Copy for next frame
                self.ofdm_buffer[..self.t_u].copy_from_slice(&check_buf);
                self.emit_sync_signal(true);
            }
        }
    }
}
