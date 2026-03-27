// ==============================================================================
// tests/integration_tests.rs - Integration tests for ETI pipeline
// ==============================================================================

use eti_rtlsdr_rust::callbacks::{CallbackHub, EtiWriter};
use eti_rtlsdr_rust::eti_pipeline::EtiPipeline;
use eti_rtlsdr_rust::support::BandHandler;
use eti_rtlsdr_rust::types::{DabBand, DabConfig, DabMode};
use num_complex::Complex;
use std::sync::Arc;
use std::sync::Mutex;

/// Mock EtiWriter pour les tests
struct TestEtiWriter {
    frames: Arc<Mutex<Vec<Vec<u8>>>>,
}

impl EtiWriter for TestEtiWriter {
    fn write_eti_frame(&self, data: &[u8]) -> anyhow::Result<()> {
        self.frames.lock().unwrap().push(data.to_vec());
        Ok(())
    }
}

#[test]
fn test_band_handler_band_iii() {
    let handler = BandHandler::new(DabBand::BandIII);
    let freq = handler.get_frequency("11C").unwrap();
    // Verify 11C frequency is in valid DAB III range
    assert!(freq > 200_000_000 && freq < 250_000_000);
}

#[test]
fn test_band_handler_all_channels() {
    let handler = BandHandler::new(DabBand::BandIII);
    let channels = handler.get_channels();
    assert!(channels.contains(&"5A".to_string()));
    assert!(channels.contains(&"11C".to_string()));
    assert!(channels.contains(&"13F".to_string()));
    assert!(channels.len() > 30);
}

#[test]
fn test_dab_config_creation() {
    let config = DabConfig::default();
    assert_eq!(config.channel, "11C");
    assert_eq!(config.mode, DabMode::ModeI);
    assert_eq!(config.band, DabBand::BandIII);
}

#[test]
fn test_eti_pipeline_creation() {
    let config = DabConfig::default();
    let pipeline = EtiPipeline::new(config, CallbackHub::new());
    assert!(pipeline.is_ok());
}

#[test]
fn test_eti_pipeline_with_writer() {
    let config = DabConfig::default();
    let frames = Arc::new(Mutex::new(Vec::new()));
    let writer = Arc::new(TestEtiWriter {
        frames: frames.clone(),
    });

    let callbacks = CallbackHub::new().with_eti_writer(writer);
    let mut pipeline = EtiPipeline::new(config, callbacks).unwrap();

    // Créer quelques samples de test (au moins 1 trame complète)
    // Mode I: 76 symboles par trame, chaque symbole = FFT(2048) + CP(504)
    let symbol_len = 2048 + 504;
    let iq_samples: Vec<Complex<f32>> = (0..76 * symbol_len)
        .map(|i| Complex::new((i as f32).sin() * 0.1, (i as f32).cos() * 0.1))
        .collect();

    // Traiter
    let result = pipeline.process_iq_block(&iq_samples);
    assert!(result.is_ok());

    // Vérifier que des frames ont été écrites
    let frames_written = frames.lock().unwrap();
    assert!(frames_written.len() > 0 || result.unwrap().len() > 0, "Expected frames generated");
}

#[test]
fn test_eti_pipeline_sync_state() {
    let config = DabConfig::default();
    let pipeline = EtiPipeline::new(config, CallbackHub::new()).unwrap();

    assert!(!pipeline.is_synced());
    // Note: signal_sync() takes &self due to internal mutability in Arc<Atomic>
}

#[test]
fn test_eti_generator_frame_numbers() {
    use eti_rtlsdr_rust::eti_handling::EtiGenerator;
    
    let mut gen = EtiGenerator::new(DabMode::ModeI);

    let frame0 = gen.generate_empty_frame();
    let frame1 = gen.generate_empty_frame();
    let frame2 = gen.generate_empty_frame();

    assert_eq!(frame0.frame_number, 0);
    assert_eq!(frame1.frame_number, 1);
    assert_eq!(frame2.frame_number, 2);
}

#[test]
fn test_ofdm_handler_snr_estimation() {
    use eti_rtlsdr_rust::ofdm::OfdmHandler;
    
    let handler = OfdmHandler::new(DabMode::ModeI);

    // Symbol with constant amplitude
    let symbol = vec![Complex::new(1.0, 1.0); 100];
    let snr = handler.estimate_snr(&symbol);

    assert!(snr >= 0.0);
    assert!(snr.is_finite());
}

#[test]
fn test_cli_args_parsing() {
    use eti_rtlsdr_rust::cli::CliArgs;
    
    let args = CliArgs {
        silent: true,
        channel: "11C".to_string(),
        gain: 75,
        ppm: 5,
        autogain: false,
        output: "output.eti".to_string(),
        device_index: 0,
        band: "III".to_string(),
        raw_file: None,
        wait_sync_ms: 5000,
        collect_time_secs: 10,
        num_processors: 4,
    };

    let config = args.to_config().unwrap();
    assert_eq!(config.channel, "11C");
    assert_eq!(config.gain_percent, 75);
    assert_eq!(config.ppm_correction, 5);
    assert_eq!(config.band, DabBand::BandIII);
}
