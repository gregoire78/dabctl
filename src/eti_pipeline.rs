// ==============================================================================
// pipeline.rs - Pipeline orchestration (refactored)
// ==============================================================================

use crate::callbacks::CallbackHub;
use crate::eti_handling::EtiGenerator;
use crate::ofdm::OfdmHandler;
use crate::support::{BandHandler};
use crate::support::DabParams;
use crate::types::{DabConfig};
use num_complex::Complex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Pipeline complète de traitement DAB → ETI
pub struct EtiPipeline {
    config: DabConfig,
    ofdm_handler: OfdmHandler,
    eti_generator: EtiGenerator,
    _band_handler: BandHandler,
    callbacks: CallbackHub,
    sync_state: Arc<AtomicBool>,
}

impl EtiPipeline {
    /// Créer une nouvelle pipeline
    pub fn new(config: DabConfig, callbacks: CallbackHub) -> anyhow::Result<Self> {
        Ok(Self {
            config: config.clone(),
            ofdm_handler: OfdmHandler::new(config.mode),
            eti_generator: EtiGenerator::new(config.mode),
            _band_handler: BandHandler::new(config.band),
            callbacks,
            sync_state: Arc::new(AtomicBool::new(false)),
        })
    }

    /// Traiter un bloc de samples IQ
    pub fn process_iq_block(&mut self, iq_samples: &[Complex<f32>]) -> anyhow::Result<Vec<Vec<u8>>> {
        let mut eti_frames = Vec::new();

        // Phase 1: Démoduler l'OFDM
        let symbols = self.ofdm_handler.demodulate_frame(iq_samples)?;

        if symbols.is_empty() {
            return Ok(eti_frames);
        }

        // Phase 2: Estimer la synchronisation
        let _cfo = self.ofdm_handler.sync_frequency(&symbols)?;
        let snr = self.ofdm_handler.estimate_snr(&symbols[0]);

        // Callback SNR
        if let Some(cb) = &self.callbacks.snr_signal {
            cb.on_snr_signal(snr as i16);
        }

        // Phase 3: Extraire les porteuses actives
        let mut fic_data = Vec::new();
        let mut msc_data = Vec::new();

        for (idx, symbol) in symbols.iter().enumerate() {
            match self.extract_symbol_data(symbol, idx)? {
                (fic, msc) => {
                    fic_data.extend_from_slice(&fic);
                    msc_data.extend_from_slice(&msc);
                }
            }
        }

        // Phase 4: Générer une trame ETI
        let eti_frame = self.eti_generator.generate_frame(&fic_data, &msc_data)?;
        let frame_bytes = eti_frame.to_bytes();

        // Callback ETI
        if let Some(writer) = &self.callbacks.eti_writer {
            writer.write_eti_frame(&frame_bytes)?;
        }

        eti_frames.push(frame_bytes);

        Ok(eti_frames)
    }

    /// Extraire les données FIC et MSC d'un symbole
    fn extract_symbol_data(&self, symbol: &[Complex<f32>], sym_index: usize) -> anyhow::Result<(Vec<u8>, Vec<u8>)> {
        let carriers = self.ofdm_handler.extract_active_carriers(symbol)?;

        // Symboles 0-2 = FIC, 3+ = MSC
        if sym_index < 3 {
            // FIC: mapper les carriers et créer des soft bits
            let fic_bits = self.map_carriers_to_bits(&carriers, DabParams::fic_bytes_per_block());
            Ok((fic_bits, Vec::new()))
        } else {
            // MSC: mapper les carriers
            let msc_bits = self.map_carriers_to_bits(&carriers, DabParams::msc_bytes_per_symbol(self.config.mode));
            Ok((Vec::new(), msc_bits))
        }
    }

    /// Mapper les porteuses complexes en bits
    fn map_carriers_to_bits(&self, carriers: &[Complex<f32>], expected_bytes: usize) -> Vec<u8> {
        let mut bits = Vec::new();

        for carrier in carriers {
            // QPSK: hard decision
            let i_bit = if carrier.re > 0.0 { 1 } else { 0 };
            let q_bit = if carrier.im > 0.0 { 1 } else { 0 };
            bits.push(i_bit);
            bits.push(q_bit);
        }

        // Convertir bits en bytes
        let mut bytes = Vec::new();
        for chunk in bits.chunks(8) {
            let mut byte = 0u8;
            for (i, &bit) in chunk.iter().enumerate() {
                byte |= bit << (7 - i);
            }
            bytes.push(byte);
        }

        bytes.truncate(expected_bytes.max(1));
        bytes
    }

    /// Signal de synchronisation atteint
    pub fn signal_sync(&self) {
        self.sync_state.store(true, Ordering::Relaxed);
        if let Some(cb) = &self.callbacks.sync_signal {
            cb.on_sync_signal(true);
        }
    }

    /// Vérifier l'état de synchronisation
    pub fn is_synced(&self) -> bool {
        self.sync_state.load(Ordering::Relaxed)
    }

    /// Réinitialiser la pipeline
    pub fn reset(&mut self) {
        self.sync_state.store(false, Ordering::Relaxed);
        self.eti_generator.reset();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pipeline_creation() {
        let config = DabConfig::default();
        let pipeline = EtiPipeline::new(config, CallbackHub::new());
        assert!(pipeline.is_ok());
    }

    #[test]
    fn test_pipeline_sync_state() {
        let config = DabConfig::default();
        let mut pipeline = EtiPipeline::new(config, CallbackHub::new()).unwrap();
        assert!(!pipeline.is_synced());
        pipeline.signal_sync();
        assert!(pipeline.is_synced());
    }
}
