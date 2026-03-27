// ==============================================================================
// types.rs - Types et énumérations DAB fondamentales
// ==============================================================================

use std::fmt;

/// Modes DAB (Mode I est le plus courant)
#[repr(u8)]
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum DabMode {
    /// Mode I - DAB standard
    ModeI = 1,
    /// Mode II - DAB avec FFT réduite
    ModeII = 2,
    /// Mode III - Bande L
    ModeIII = 3,
    /// Mode IV - Bande L avec FFT réduite
    ModeIV = 4,
}

impl DabMode {
    /// Obtenir la taille de la FFT pour ce mode
    pub fn fft_size(&self) -> u16 {
        match self {
            Self::ModeI => 2048,
            Self::ModeII => 512,
            Self::ModeIII => 2048,
            Self::ModeIV => 512,
        }
    }

    /// Obtenir la longueur du préfixe cyclique
    pub fn cyclic_prefix_len(&self) -> u16 {
        match self {
            Self::ModeI => 504,
            Self::ModeII => 126,
            Self::ModeIII => 504,
            Self::ModeIV => 126,
        }
    }

    /// Obtenir le nombre de symboles OFDM par trame
    pub fn symbols_per_frame(&self) -> u16 {
        match self {
            Self::ModeI | Self::ModeIII => 76,
            Self::ModeII | Self::ModeIV => 76,
        }
    }
}

impl fmt::Display for DabMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ModeI => write!(f, "Mode I"),
            Self::ModeII => write!(f, "Mode II"),
            Self::ModeIII => write!(f, "Mode III"),
            Self::ModeIV => write!(f, "Mode IV"),
        }
    }
}

/// Bandes de fréquence DAB
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum DabBand {
    /// Bande III (174-240 MHz) - Europe, Afrique
    BandIII,
    /// Bande L (1452-1492 MHz) - Satellite, portable
    BandL,
}

impl fmt::Display for DabBand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BandIII => write!(f, "Band III"),
            Self::BandL => write!(f, "Band L"),
        }
    }
}

/// Canal DAB avec fréquence
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct DabChannel {
    pub name: String,
    pub band: DabBand,
    pub frequency_hz: u32,
}

impl DabChannel {
    /// Créer un canal DAB
    pub fn new(name: String, band: DabBand, frequency_hz: u32) -> Self {
        Self {
            name,
            band,
            frequency_hz,
        }
    }

    /// Parser un canal "11C" ou "5A"
    pub fn from_string(s: &str) -> anyhow::Result<(String, u32)> {
        // Retune simplement le nom et zéro pour fréquence (sera résolu par band handler)
        Ok((s.to_string(), 0))
    }
}

impl fmt::Display for DabChannel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} ({})", self.name, self.frequency_hz)
    }
}

/// État de synchronisation OFDM
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum SyncState {
    /// Pas synchronisé, en recherche
    Searching,
    /// Synchronisé en fréquence, recherche de phase
    FreqSync,
    /// Complètement synchronisé
    Synced,
    /// Perdu la synchronisation
    Lost,
}

impl fmt::Display for SyncState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Searching => write!(f, "Searching"),
            Self::FreqSync => write!(f, "FreqSync"),
            Self::Synced => write!(f, "Synced"),
            Self::Lost => write!(f, "Lost"),
        }
    }
}

/// Paramètres de protection UEP/EEP
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum ProtectionLevel {
    /// Protection inégale (UEP)
    Uep(u8),
    /// Protection égale (EEP)
    Eep(u8),
}

/// Type de service
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum ServiceType {
    /// Audio numérique
    Audio,
    /// Données
    Data,
    /// Autres
    Other,
}

/// Configuration de la pipeline ETI
#[derive(Debug, Clone)]
pub struct DabConfig {
    /// Mode DAB
    pub mode: DabMode,
    /// Bande de fréquence
    pub band: DabBand,
    /// Canal à recevoir
    pub channel: String,
    /// Gain (0-100 %)
    pub gain_percent: u32,
    /// Correction PPM
    pub ppm_correction: i32,
    /// Autogain activé
    pub autogain: bool,
    /// Index du device
    pub device_index: u32,
    /// Mode silencieux
    pub silent: bool,
}

impl Default for DabConfig {
    fn default() -> Self {
        Self {
            mode: DabMode::ModeI,
            band: DabBand::BandIII,
            channel: "11C".to_string(),
            gain_percent: 50,
            ppm_correction: 0,
            autogain: false,
            device_index: 0,
            silent: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dab_mode_properties() {
        assert_eq!(DabMode::ModeI.fft_size(), 2048);
        assert_eq!(DabMode::ModeI.cyclic_prefix_len(), 504);
        assert_eq!(DabMode::ModeII.fft_size(), 512);
    }

    #[test]
    fn test_dab_channel_creation() {
        let ch = DabChannel::new("11C".to_string(), DabBand::BandIII, 223936000);
        assert_eq!(ch.name, "11C");
        assert_eq!(ch.frequency_hz, 223936000);
    }

    #[test]
    fn test_sync_state_display() {
        assert_eq!(format!("{}", SyncState::Synced), "Synced");
    }

    #[test]
    fn test_dab_config_default() {
        let config = DabConfig::default();
        assert_eq!(config.channel, "11C");
        assert_eq!(config.gain_percent, 50);
        assert!(!config.silent);
    }
}
