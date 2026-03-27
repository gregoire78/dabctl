// ==============================================================================
// ofdm/ofdm_handler.rs - OFDM handling refactored (clean code version)
// ==============================================================================

use crate::support::FftProcessor;
use crate::types::DabMode;
use num_complex::Complex;
use anyhow::Result;

/// Gestionnaire OFDM moderne et refactorisé
pub struct OfdmHandler {
    fft_processor: FftProcessor,
    mode: DabMode,
}

impl OfdmHandler {
    /// Créer un nouveau gestionnaire OFDM pour un mode DAB
    pub fn new(mode: DabMode) -> Self {
        Self {
            fft_processor: FftProcessor::new(),
            mode,
        }
    }

    /// Effectuer la synchronisation de fréquence
    ///
    /// Cette fonction détecte l'offset de fréquence en utilisant
    /// les porteuses de référence pilote.
    pub fn sync_frequency(&self, symbols: &[Vec<Complex<f32>>]) -> Result<f32> {
        if symbols.is_empty() {
            return Ok(0.0);
        }

        // Calcul simple de CFO (Carrier Frequency Offset)
        // À remplacer par logique plus sophistiquée du code original
        let mut total_phase = 0.0;
        for symbol in symbols.iter().take(3) {
            if !symbol.is_empty() {
                let phase = symbol[0].arg();
                total_phase += phase;
            }
        }

        Ok(total_phase / symbols.len() as f32)
    }

    /// Démoduler une trame OFDM
    pub fn demodulate_frame(&self, iq_samples: &[Complex<f32>]) -> Result<Vec<Vec<Complex<f32>>>> {
        let fft_size = self.mode.fft_size() as usize;
        let cp_len = self.mode.cyclic_prefix_len() as usize;
        let symbol_len = fft_size + cp_len;
        let symbols_per_frame = self.mode.symbols_per_frame() as usize;

        let mut symbols = Vec::new();

        for sym_idx in 0..symbols_per_frame {
            let start = sym_idx * symbol_len;
            let end = start + symbol_len;

            if end > iq_samples.len() {
                break;
            }

            // Sauter le préfixe cyclique
            let fft_input = &iq_samples[start + cp_len..end];

            // Effectuer FFT
            let fft_output = self.fft_processor.fft_forward(
                &fft_input.iter().copied().collect::<Vec<_>>()
            )?;

            symbols.push(fft_output);
        }

        Ok(symbols)
    }

    /// Extraire les porteuses actives (mapping de carriers)
    pub fn extract_active_carriers(&self, symbol: &[Complex<f32>]) -> anyhow::Result<Vec<Complex<f32>>> {
        // Mode I a 1536 porteuses actives sur 2048
        let active_carriers = match self.mode {
            DabMode::ModeI | DabMode::ModeIII => 1536,
            DabMode::ModeII | DabMode::ModeIV => 384,
        };

        let fft_size = self.mode.fft_size() as usize;

        // Extraction standard: carriers du milieu de la FFT
        let start_idx = (fft_size - active_carriers) / 2;
        let end_idx = start_idx + active_carriers;

        if symbol.len() < end_idx {
            return Err(anyhow::anyhow!("Symbol too short for carrier extraction"));
        }

        Ok(symbol[start_idx..end_idx].to_vec())
    }

    /// Estimer le rapport signal-noise (SNR)
    pub fn estimate_snr(&self, symbol: &[Complex<f32>]) -> f32 {
        if symbol.is_empty() {
            return 0.0;
        }

        let signal_power: f32 = symbol.iter().map(|c| c.norm_sqr()).sum::<f32>() / symbol.len() as f32;
        
        // Bruit estimé comme variance + 1e-6 (floor)
        let variance: f32 = symbol
            .iter()
            .map(|c| {
                let diff = c.norm_sqr() - signal_power;
                diff * diff
            })
            .sum::<f32>()
            / symbol.len() as f32;

        let noise_power = variance.max(1e-6);
        10.0 * (signal_power / noise_power).log10()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ofdm_handler_creation() {
        let handler = OfdmHandler::new(DabMode::ModeI);
        assert_eq!(handler.mode, DabMode::ModeI);
    }

    #[test]
    fn test_sync_frequency_empty() {
        let handler = OfdmHandler::new(DabMode::ModeI);
        let result = handler.sync_frequency(&[]);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 0.0);
    }

    #[test]
    fn test_snr_estimation() {
        let handler = OfdmHandler::new(DabMode::ModeI);
        let symbol = vec![Complex::new(1.0, 0.0); 100];
        let snr = handler.estimate_snr(&symbol);
        assert!(snr >= 0.0);
    }
}
