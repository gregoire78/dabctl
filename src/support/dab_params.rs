// ==============================================================================
// support/dab_params.rs - Paramètres DAB standards
// ==============================================================================

use crate::types::DabMode;

/// Paramètres DAB standardisés
pub struct DabParams;

impl DabParams {
    /// Taille FFT pour le mode DAB
    pub fn fft_size(mode: DabMode) -> usize {
        mode.fft_size() as usize
    }

    /// Longueur du préfixe cyclique
    pub fn cyclic_prefix_len(mode: DabMode) -> usize {
        mode.cyclic_prefix_len() as usize
    }

    /// Nombre de symboles OFDM par trame
    pub fn symbols_per_frame(mode: DabMode) -> usize {
        mode.symbols_per_frame() as usize
    }

    /// Nombre de porteuses actives
    pub fn active_carriers(mode: DabMode) -> usize {
        match mode {
            DabMode::ModeI | DabMode::ModeIII => 1536,
            DabMode::ModeII | DabMode::ModeIV => 384,
        }
    }

    /// Nombre de porteuses de référence pilote
    pub fn pilot_carriers(mode: DabMode) -> usize {
        match mode {
            DabMode::ModeI | DabMode::ModeIII => 192,
            DabMode::ModeII | DabMode::ModeIV => 48,
        }
    }

    /// Nombre de symboles FIC par trame
    pub fn fic_symbols(_mode: DabMode) -> usize {
        3
    }

    /// Nombre de symboles MSC par trame
    pub fn msc_symbols(mode: DabMode) -> usize {
        Self::symbols_per_frame(mode) - Self::fic_symbols(mode)
    }

    /// Nombre de bits FIC par bloc
    pub fn fic_bits_per_block() -> usize {
        384
    }

    /// Nombre de bytes FIC par bloc
    pub fn fic_bytes_per_block() -> usize {
        Self::fic_bits_per_block() / 8
    }

    /// Nombre de bits FIB par bloc
    pub fn fib_bits() -> usize {
        256
    }

    /// Nombre de bytes FIB par bloc
    pub fn fib_bytes() -> usize {
        Self::fib_bits() / 8
    }

    /// Nombre de FIB par trame
    pub fn fibs_per_frame() -> usize {
        3
    }

    /// Longueur totale de la trame ETI en bytes
    pub fn eti_frame_bytes() -> usize {
        6144
    }

    /// Nombre de bytes ETI FIC par trame
    pub fn eti_fic_bytes() -> usize {
        384
    }

    /// Nombre de bytes MSC par symbole
    pub fn msc_bytes_per_symbol(mode: DabMode) -> usize {
        Self::active_carriers(mode) * 2 / 8 // 2 bits/carrier QPSK
    }

    /// Nombre de bytes de données par trame ETI
    pub fn eti_data_bytes() -> usize {
        Self::eti_frame_bytes() - Self::eti_fic_bytes() - 16 // 16 bytes = header + footers
    }

    /// Durée d'une trame en millisecondes
    pub fn frame_duration_ms() -> u32 {
        24 // 24 ms par trame DAB
    }

    /// Taille du buffer d'entrée IQ recommandée
    pub fn recommended_iq_buffer_size() -> usize {
        16 * 16384 // 256 KB
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fft_size() {
        assert_eq!(DabParams::fft_size(DabMode::ModeI), 2048);
        assert_eq!(DabParams::fft_size(DabMode::ModeII), 512);
    }

    #[test]
    fn test_cyclic_prefix() {
        assert_eq!(DabParams::cyclic_prefix_len(DabMode::ModeI), 504);
    }

    #[test]
    fn test_eti_frame_size() {
        assert_eq!(DabParams::eti_frame_bytes(), 6144);
    }

    #[test]
    fn test_frame_duration() {
        assert_eq!(DabParams::frame_duration_ms(), 24);
    }

    #[test]
    fn test_fib_params() {
        assert_eq!(DabParams::fib_bytes(), 32);
        assert_eq!(DabParams::fibs_per_frame(), 3);
    }
}
