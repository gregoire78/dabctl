use anyhow::Result;
use num_complex::Complex32;

use crate::device::RtlSdrDevice;

// Literal receive-side helper mirroring DABstar's sample-reader stage.
pub struct SampleReader {
    device: RtlSdrDevice,
    scratch: Vec<u8>,
}

impl SampleReader {
    pub fn new(device: RtlSdrDevice) -> Self {
        Self {
            device,
            scratch: Vec::new(),
        }
    }

    pub fn read_iq_block(&mut self, byte_len: usize) -> Result<Vec<Complex32>> {
        self.scratch.resize(byte_len, 0);
        let n_read = self.device.read_sync(&mut self.scratch)?;
        let mut iq = Vec::with_capacity(n_read / 2);

        for chunk in self.scratch[..n_read].chunks_exact(2) {
            let i = (f32::from(chunk[0]) - 127.5) / 127.5;
            let q = (f32::from(chunk[1]) - 127.5) / 127.5;
            iq.push(Complex32::new(i, q));
        }

        Ok(iq)
    }
}
