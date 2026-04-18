use std::io::{self, Write};

use anyhow::Result;

// Raw PCM sink: s16le, 48 kHz, stereo, stdout only.
pub struct PcmOutput {
    stdout: io::Stdout,
}

impl PcmOutput {
    pub fn stdout() -> Self {
        Self {
            stdout: io::stdout(),
        }
    }

    pub fn write_interleaved(&mut self, samples: &[i16]) -> Result<()> {
        let mut bytes = Vec::with_capacity(samples.len() * 2);
        for sample in samples {
            bytes.extend_from_slice(&sample.to_le_bytes());
        }
        self.stdout.write_all(&bytes)?;
        self.stdout.flush()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn i16_to_le_bytes_layout_matches_s16le() {
        let sample = -2i16;
        assert_eq!(sample.to_le_bytes(), [0xFE, 0xFF]);
    }
}
