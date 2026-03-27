// ==============================================================================
// errors.rs - Error types pour la pipeline ETI
// ==============================================================================

use std::fmt;

#[derive(Debug)]
pub enum EtiError {
    /// Erreur de synchronisation OFDM
    SyncError(String),

    /// Erreur d'entrée/sortie (device ou fichier)
    IoError(String),

    /// Erreur de configuration invalide
    ConfigError(String),

    /// Erreur de décodage ETI
    DecodeError(String),

    /// Erreur de buffer insuffisant
    BufferError(String),

    /// Erreur de device
    DeviceError(String),

    /// Erreur interne de processing
    ProcessingError(String),

    /// Erreur d'allocation mémoire
    AllocationError(String),
}

impl fmt::Display for EtiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EtiError::SyncError(msg) => write!(f, "Sync Error: {}", msg),
            EtiError::IoError(msg) => write!(f, "IO Error: {}", msg),
            EtiError::ConfigError(msg) => write!(f, "Config Error: {}", msg),
            EtiError::DecodeError(msg) => write!(f, "Decode Error: {}", msg),
            EtiError::BufferError(msg) => write!(f, "Buffer Error: {}", msg),
            EtiError::DeviceError(msg) => write!(f, "Device Error: {}", msg),
            EtiError::ProcessingError(msg) => write!(f, "Processing Error: {}", msg),
            EtiError::AllocationError(msg) => write!(f, "Allocation Error: {}", msg),
        }
    }
}

impl std::error::Error for EtiError {}

pub type Result<T> = std::result::Result<T, EtiError>;
