// ==============================================================================
// eti_handling/eti_handler.rs - ETI generation refactored
// ==============================================================================

use crate::support::DabParams;
use crate::types::DabMode;
use anyhow::Result;

/// Structure pour representa une trame ETI
#[derive(Debug, Clone)]
pub struct EtiFrame {
    /// Byte de synchronisation (pair/impair)
    pub sync: [u8; 4],
    /// Données FIC (96 bytes pour Mode I)
    pub fic_data: Vec<u8>,
    /// Données MSC
    pub msc_data: Vec<u8>,
    /// Numéro de trame
    pub frame_number: u32,
}

impl EtiFrame {
    /// Créer une nouvelle trame ETI
    pub fn new(_mode: DabMode, frame_number: u32) -> Self {
        let fic_len = DabParams::eti_fic_bytes();
        let msc_len = DabParams::eti_frame_bytes() - fic_len - 16;

        Self {
            sync: [0; 4],
            fic_data: vec![0; fic_len],
            msc_data: vec![0; msc_len],
            frame_number,
        }
    }

    /// Encoder la trame ETI en bytes
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buffer = Vec::new();

        // Sync + header
        buffer.extend_from_slice(&self.sync);
        buffer.extend_from_slice(&self.frame_number.to_le_bytes());

        // FIC
        buffer.extend_from_slice(&self.fic_data);

        // MSC
        buffer.extend_from_slice(&self.msc_data);

        // Padding to 6144 bytes
        while buffer.len() < DabParams::eti_frame_bytes() {
            buffer.push(0);
        }

        buffer.truncate(DabParams::eti_frame_bytes());
        buffer
    }

    /// Obtenir la longueur de la trame en bytes
    pub fn size(&self) -> usize {
        DabParams::eti_frame_bytes()
    }
}

/// Générateur ETI principale
pub struct EtiGenerator {
    mode: DabMode,
    frame_counter: u32,
}

impl EtiGenerator {
    /// Créer un nouveau générateur ETI
    pub fn new(mode: DabMode) -> Self {
        Self {
            mode,
            frame_counter: 0,
        }
    }

    /// Générer une trame ETI vide
    pub fn generate_empty_frame(&mut self) -> EtiFrame {
        let frame_num = self.frame_counter;
        self.frame_counter = self.frame_counter.wrapping_add(1);

        let mut frame = EtiFrame::new(self.mode, frame_num);

        // Définir les bytes de sync (pair/impair)
        if frame_num.is_multiple_of(2) {
            frame.sync = [0x00, 0x49, 0xC5, 0xF8]; // SYNC even
        } else {
            frame.sync = [0x00, 0xB6, 0x3A, 0x07]; // SYNC odd
        }

        frame
    }

    /// Générer une trame ETI avec données FIC et MSC
    pub fn generate_frame(&mut self, fic: &[u8], msc: &[u8]) -> Result<EtiFrame> {
        let frame_num = self.frame_counter;
        self.frame_counter = self.frame_counter.wrapping_add(1);

        let mut frame = EtiFrame::new(self.mode, frame_num);

        // Copier les données
        let fic_len = frame.fic_data.len().min(fic.len());
        frame.fic_data[..fic_len].copy_from_slice(&fic[..fic_len]);

        let msc_len = frame.msc_data.len().min(msc.len());
        frame.msc_data[..msc_len].copy_from_slice(&msc[..msc_len]);

        // Sync byte
        if frame_num.is_multiple_of(2) {
            frame.sync = [0x00, 0x49, 0xC5, 0xF8]; // Even
        } else {
            frame.sync = [0x00, 0xB6, 0x3A, 0x07]; // Odd
        }

        Ok(frame)
    }

    /// Obtenir le numéro de trame actuel
    pub fn get_frame_number(&self) -> u32 {
        self.frame_counter
    }

    /// Réinitialiser le générateur
    pub fn reset(&mut self) {
        self.frame_counter = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_eti_frame_creation() {
        let frame = EtiFrame::new(DabMode::ModeI, 0);
        assert_eq!(frame.frame_number, 0);
        assert_eq!(frame.size(), DabParams::eti_frame_bytes());
    }

    #[test]
    fn test_eti_frame_to_bytes() {
        let frame = EtiFrame::new(DabMode::ModeI, 0);
        let bytes = frame.to_bytes();
        assert_eq!(bytes.len(), DabParams::eti_frame_bytes());
    }

    #[test]
    fn test_eti_generator() {
        let mut gen = EtiGenerator::new(DabMode::ModeI);
        let frame1 = gen.generate_empty_frame();
        let frame2 = gen.generate_empty_frame();

        assert_eq!(frame1.frame_number, 0);
        assert_eq!(frame2.frame_number, 1);
        assert_ne!(frame1.sync, frame2.sync); // Odd/even should differ
    }

    #[test]
    fn test_eti_generator_with_data() {
        let mut gen = EtiGenerator::new(DabMode::ModeI);
        let fic = vec![1, 2, 3];
        let msc = vec![4, 5, 6];

        let frame = gen.generate_frame(&fic, &msc).unwrap();
        assert_eq!(frame.fic_data[0], 1);
        assert_eq!(frame.msc_data[0], 4);
    }
}
