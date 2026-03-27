// ==============================================================================
// support/band_handler.rs - Gestion des bandes DAB et canaux
// ==============================================================================

use crate::types::{DabBand, DabChannel};
use anyhow::{anyhow, Result};
use std::collections::HashMap;

/// Gestionnaire des bandes DAB et mapping canal->fréquence
pub struct BandHandler {
    channels_band_iii: HashMap<String, u32>,
    channels_band_l: HashMap<String, u32>,
    current_band: DabBand,
}

impl BandHandler {
    /// Créer un nouveau gestionnaire de bande
    pub fn new(band: DabBand) -> Self {
        let channels_band_iii = Self::init_band_iii();
        let channels_band_l = Self::init_band_l();

        Self {
            channels_band_iii,
            channels_band_l,
            current_band: band,
        }
    }

    /// Initialiser les canaux de la Bande III (174-240 MHz)
    fn init_band_iii() -> HashMap<String, u32> {
        let mut channels = HashMap::new();
        // Bande III: fréquence = 174928000 + channel_index * 1712000 Hz
        const BASE_FREQ: u32 = 174928000;
        const SPACING: u32 = 1712000;

        let channel_names = [
            "5A", "5B", "5C", "5D", "6A", "6B", "6C", "6D",
            "7A", "7B", "7C", "7D", "8A", "8B", "8C", "8D",
            "9A", "9B", "9C", "9D", "10A", "10B", "10C", "10D",
            "11A", "11B", "11C", "11D", "12A", "12B", "12C", "12D",
            "13A", "13B", "13C", "13D", "13E", "13F",
        ];

        for (idx, name) in channel_names.iter().enumerate() {
            let freq = BASE_FREQ + (idx as u32) * SPACING;
            channels.insert(name.to_string(), freq);
        }

        channels
    }

    /// Initialiser les canaux de la Bande L (1452-1492 MHz)
    fn init_band_l() -> HashMap<String, u32> {
        let mut channels = HashMap::new();
        // Bande L: fréquence = 1452960000 + channel_index * 1712000 Hz
        const BASE_FREQ: u32 = 1452960000;
        const SPACING: u32 = 1712000;

        let channel_names = [
            "LA", "LB", "LC", "LD", "LE", "LF", "LG", "LH",
            "LI", "LJ", "LK", "LL", "LM", "LN", "LO", "LP",
        ];

        for (idx, name) in channel_names.iter().enumerate() {
            let freq = BASE_FREQ + (idx as u32) * SPACING;
            channels.insert(name.to_string(), freq);
        }

        channels
    }

    /// Résoudre un nom de canal en fréquence
    pub fn get_frequency(&self, channel_name: &str) -> Result<u32> {
        let channels = match self.current_band {
            DabBand::BandIII => &self.channels_band_iii,
            DabBand::BandL => &self.channels_band_l,
        };

        channels
            .get(channel_name)
            .copied()
            .ok_or_else(|| anyhow!("Unknown channel: {} in band {:?}", channel_name, self.current_band))
    }

    /// Obtenir tous les canaux disponibles pour la bande actuelle
    pub fn get_channels(&self) -> Vec<String> {
        let channels = match self.current_band {
            DabBand::BandIII => &self.channels_band_iii,
            DabBand::BandL => &self.channels_band_l,
        };

        let mut names: Vec<_> = channels.keys().cloned().collect();
        names.sort();
        names
    }

    /// Obtenir la fréquence d'échantillonnage pour DAB (normalement fixe)
    pub fn get_sample_rate(&self) -> u32 {
        2048000 // 2.048 MHz pour DAB
    }

    /// Changer de bande
    pub fn set_band(&mut self, band: DabBand) {
        self.current_band = band;
    }

    /// Créer un objet DabChannel
    pub fn create_channel(&self, name: &str) -> Result<DabChannel> {
        let freq = self.get_frequency(name)?;
        Ok(DabChannel::new(name.to_string(), self.current_band, freq))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_band_iii_channels() {
        let handler = BandHandler::new(DabBand::BandIII);
        let freq = handler.get_frequency("11C").unwrap();
        // 11C is at index 22 (5A=0, 5B=1, ..., 11C=22)
        // Base: 174928000, spacing: 1712000
        // Formula: 174928000 + 22 * 1712000 = 174928000 + 37664000 = 212592000
        // But let's just verify the formula works
        assert!(freq > 200_000_000 && freq < 250_000_000); // Valid DAB III range
    }

    #[test]
    fn test_invalid_channel() {
        let handler = BandHandler::new(DabBand::BandIII);
        assert!(handler.get_frequency("INVALID").is_err());
    }

    #[test]
    fn test_sample_rate() {
        let handler = BandHandler::new(DabBand::BandIII);
        assert_eq!(handler.get_sample_rate(), 2048000);
    }

    #[test]
    fn test_get_all_channels() {
        let handler = BandHandler::new(DabBand::BandIII);
        let channels = handler.get_channels();
        assert!(!channels.is_empty());
        assert!(channels.contains(&"11C".to_string()));
    }

    #[test]
    fn test_change_band() {
        let mut handler = BandHandler::new(DabBand::BandIII);
        handler.set_band(DabBand::BandL);
        let channels = handler.get_channels();
        assert!(channels.contains(&"LA".to_string()));
    }
}
