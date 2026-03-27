// ==============================================================================
// callbacks.rs - Callbacks traits for ETI processing pipeline
// ==============================================================================
// Traits remplaçant les callbacks C++ pour une architecture Rust idiomatique
// et découplée.

use std::sync::Arc;

/// Trait pour écrire les trames ETI générées
pub trait EtiWriter: Send + Sync {
    /// Écrire une trame ETI
    fn write_eti_frame(&self, data: &[u8]) -> anyhow::Result<()>;
}

/// Trait pour notifier le nom de l'ensemble (DAB ensemble)
pub trait EnsembleNameCallback: Send + Sync {
    /// Appel quand le nom de l'ensemble est reçu
    fn on_ensemble_name(&self, name: String, ensemble_id: u32);
}

/// Trait pour notifier le nom du service/programme
pub trait ProgramNameCallback: Send + Sync {
    /// Appel quand le nom du programme est reçu
    fn on_program_name(&self, name: String, service_id: i32);
}

/// Trait pour notifier l'état de synchronisation
pub trait SyncSignalCallback: Send + Sync {
    /// Appel quand l'état de sync change
    fn on_sync_signal(&self, is_synced: bool);
}

/// Trait pour notifier le SNR (Signal-to-Noise Ratio)
pub trait SnrSignalCallback: Send + Sync {
    /// Appel quand le SNR est calculé
    fn on_snr_signal(&self, snr: i16);
}

/// Trait pour notifier la qualité du FIB
pub trait FibQualityCallback: Send + Sync {
    /// Appel quand la qualité du FIB est évaluée
    fn on_fib_quality(&self, quality: i16);
}

/// Trait pour notifier l'arrêt de l'input
pub trait InputStoppedCallback: Send + Sync {
    /// Appel quand l'input s'arrête
    fn on_input_stopped(&self);
}

/// Agrégateur de callbacks pour orchestration centralisée
#[derive(Clone)]
pub struct CallbackHub {
    pub eti_writer: Option<Arc<dyn EtiWriter>>,
    pub ensemble_name: Option<Arc<dyn EnsembleNameCallback>>,
    pub program_name: Option<Arc<dyn ProgramNameCallback>>,
    pub sync_signal: Option<Arc<dyn SyncSignalCallback>>,
    pub snr_signal: Option<Arc<dyn SnrSignalCallback>>,
    pub fib_quality: Option<Arc<dyn FibQualityCallback>>,
    pub input_stopped: Option<Arc<dyn InputStoppedCallback>>,
}

impl CallbackHub {
    /// Créer un hub vide
    pub fn new() -> Self {
        Self {
            eti_writer: None,
            ensemble_name: None,
            program_name: None,
            sync_signal: None,
            snr_signal: None,
            fib_quality: None,
            input_stopped: None,
        }
    }

    /// Builder pattern pour construire le hub
    pub fn with_eti_writer(mut self, writer: Arc<dyn EtiWriter>) -> Self {
        self.eti_writer = Some(writer);
        self
    }

    pub fn with_ensemble_name(mut self, callback: Arc<dyn EnsembleNameCallback>) -> Self {
        self.ensemble_name = Some(callback);
        self
    }

    pub fn with_program_name(mut self, callback: Arc<dyn ProgramNameCallback>) -> Self {
        self.program_name = Some(callback);
        self
    }

    pub fn with_sync_signal(mut self, callback: Arc<dyn SyncSignalCallback>) -> Self {
        self.sync_signal = Some(callback);
        self
    }

    pub fn with_snr_signal(mut self, callback: Arc<dyn SnrSignalCallback>) -> Self {
        self.snr_signal = Some(callback);
        self
    }

    pub fn with_fib_quality(mut self, callback: Arc<dyn FibQualityCallback>) -> Self {
        self.fib_quality = Some(callback);
        self
    }

    pub fn with_input_stopped(mut self, callback: Arc<dyn InputStoppedCallback>) -> Self {
        self.input_stopped = Some(callback);
        self
    }
}

impl Default for CallbackHub {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockEtiWriter {
        frames_written: std::sync::Mutex<usize>,
    }

    impl EtiWriter for MockEtiWriter {
        fn write_eti_frame(&self, _data: &[u8]) -> anyhow::Result<()> {
            *self.frames_written.lock().unwrap() += 1;
            Ok(())
        }
    }

    #[test]
    fn test_callback_hub_builder() {
        let writer = Arc::new(MockEtiWriter {
            frames_written: std::sync::Mutex::new(0),
        });

        let hub = CallbackHub::new()
            .with_eti_writer(writer.clone());

        assert!(hub.eti_writer.is_some());
        assert!(hub.ensemble_name.is_none());
    }
}
