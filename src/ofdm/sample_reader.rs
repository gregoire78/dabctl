use anyhow::Result;
use num_complex::Complex32;

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
    do_dc_or_iq_corr: bool,
    do_iq_corr: bool,
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
            do_dc_or_iq_corr: true,
            do_iq_corr: false,
        }
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

    pub fn read_iq_block(&mut self, byte_len: usize) -> Result<Vec<Complex32>> {
        self.scratch.resize(byte_len, 0);
        let n_read = self.device.read_sync(&mut self.scratch)?;
        let mut iq = Vec::with_capacity(n_read / 2);

        for chunk in self.scratch[..n_read].chunks_exact(2) {
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

            iq.push(Complex32::new(i, q));
        }

        Ok(iq)
    }
}

fn mean_filter(target: &mut f32, value: f32, alpha: f32) {
    *target += alpha * (value - *target);
}
