use num_complex::Complex32;

const NULL_SYMBOL_SAMPLES: usize = 2_656;
const POWER_WINDOW: usize = 256;

// ETSI EN 300 401 Mode I: frame acquisition first looks for the low-power null symbol.
#[derive(Default)]
pub struct TimeSyncer {
    last_peak_index: Option<usize>,
}

impl TimeSyncer {
    pub fn push(&mut self, samples: &[Complex32]) -> Option<usize> {
        if samples.len() < NULL_SYMBOL_SAMPLES + POWER_WINDOW {
            return None;
        }

        let mut best_index = 0usize;
        let mut best_metric = f32::MAX;

        for start in 0..=samples.len() - POWER_WINDOW {
            let power = samples[start..start + POWER_WINDOW]
                .iter()
                .map(|sample| sample.norm_sqr())
                .sum::<f32>()
                / POWER_WINDOW as f32;

            if power < best_metric {
                best_metric = power;
                best_index = start;
            }
        }

        let symbol_zero_start =
            (best_index + NULL_SYMBOL_SAMPLES).min(samples.len().saturating_sub(1));
        self.last_peak_index = Some(symbol_zero_start);
        self.last_peak_index
    }
}

#[cfg(test)]
mod tests {
    use num_complex::Complex32;

    use super::TimeSyncer;

    #[test]
    fn locates_symbol_after_null_region() {
        let mut samples = vec![Complex32::new(1.0, 0.0); 10_000];
        for sample in &mut samples[1000..(1000 + 2656)] {
            *sample = Complex32::new(0.0, 0.0);
        }

        let mut syncer = TimeSyncer::default();
        let start = syncer.push(&samples).expect("time sync should be found");
        assert!((3600..3700).contains(&start));
    }
}
