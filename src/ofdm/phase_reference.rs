use num_complex::Complex32;

const TU: usize = 2_048;
const TS: usize = 2_552;
const CARRIER_SPACING_HZ: f32 = 1_000.0;

// ETSI EN 300 401 §14: evaluate the cyclic prefix phase shift for fine frequency correction.
#[derive(Default)]
pub struct PhaseReference {
    last_phase_rad: f32,
    last_freq_error_hz: f32,
}

impl PhaseReference {
    pub fn analyze(&mut self, samples: &[Complex32]) -> f32 {
        if samples.len() < TS {
            self.last_phase_rad = 0.0;
            self.last_freq_error_hz = 0.0;
            return self.last_phase_rad;
        }

        let mut corr = Complex32::new(0.0, 0.0);
        for idx in TU..TS {
            corr += samples[idx] * samples[idx - TU].conj();
        }

        self.last_phase_rad = corr.arg();
        self.last_freq_error_hz =
            self.last_phase_rad / (2.0 * std::f32::consts::PI) * CARRIER_SPACING_HZ;
        self.last_phase_rad
    }

    pub fn last_freq_error_hz(&self) -> f32 {
        self.last_freq_error_hz
    }
}

#[cfg(test)]
mod tests {
    use num_complex::Complex32;

    use super::{PhaseReference, TS};

    #[test]
    fn estimates_prefix_phase_shift() {
        let phase_step = 0.1f32;
        let samples = (0..TS)
            .map(|idx| {
                let phase = idx as f32 * phase_step;
                Complex32::new(phase.cos(), phase.sin())
            })
            .collect::<Vec<_>>();

        let mut pr = PhaseReference::default();
        let phase = pr.analyze(&samples);
        assert!(phase.is_finite());
        assert!(pr.last_freq_error_hz().is_finite());
    }
}
