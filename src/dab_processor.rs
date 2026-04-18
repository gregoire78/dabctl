use anyhow::Result;
use tracing::{info, warn};

use crate::backend::{
    audio::{DecoderFactory, DEFAULT_DAB_PLUS_BITRATE},
    msc_handler::MscHandler,
};
use crate::cli::{AacDecoderKind, Cli};
use crate::decoder::{fib_decoder::FibDecoder, fic_decoder::FicDecoder};
use crate::device::{DeviceOptions, RtlSdrDevice};
use crate::metadata::MetadataWriter;
use crate::ofdm::{
    ofdm_decoder::{OfdmDecoder, TS, TU},
    phase_reference::PhaseReference,
    sample_reader::SampleReader,
    time_syncer::TimeSyncer,
};
use crate::pcm::PcmOutput;

#[derive(Debug, Clone)]
pub struct ReceiverConfig {
    pub channel: String,
    pub sid: u32,
    pub label: Option<String>,
    pub center_freq_hz: u32,
    pub gain: Option<u8>,
    pub hardware_agc: bool,
    pub driver_agc: bool,
    pub software_agc: bool,
    pub silent: bool,
    pub slide_base64: bool,
    pub slide_dir: Option<std::path::PathBuf>,
    pub device_index: u32,
    pub aac_decoder: AacDecoderKind,
}

impl ReceiverConfig {
    pub fn from_cli(cli: &Cli, center_freq_hz: u32) -> Self {
        Self {
            channel: cli.channel.clone(),
            sid: cli.sid,
            label: cli.label.clone(),
            center_freq_hz,
            gain: cli.gain,
            hardware_agc: cli.hardware_agc,
            driver_agc: cli.driver_agc,
            software_agc: cli.software_agc,
            silent: cli.silent,
            slide_base64: cli.slide_base64,
            slide_dir: cli.slide_dir.clone(),
            device_index: cli.device_index,
            aac_decoder: cli.aac_decoder,
        }
    }

    pub fn device_options(&self) -> DeviceOptions {
        DeviceOptions {
            index: self.device_index,
            center_freq_hz: self.center_freq_hz,
            gain: self.gain,
            hardware_agc: self.hardware_agc,
            driver_agc: self.driver_agc,
            software_agc: self.software_agc,
            silent: self.silent,
        }
    }
}

// Mirrors DABstar's DabProcessor orchestration order:
// SampleReader -> TimeSyncer -> PhaseReference -> OfdmDecoder -> FIC / MSC split.
pub struct DabProcessor {
    config: ReceiverConfig,
    fic_decoder: FicDecoder,
    fib_decoder: FibDecoder,
    msc_handler: MscHandler,
    phase_reference: PhaseReference,
    time_syncer: TimeSyncer,
    ofdm_decoder: OfdmDecoder,
}

impl DabProcessor {
    pub fn new(config: ReceiverConfig) -> Self {
        Self {
            fic_decoder: FicDecoder::default(),
            fib_decoder: FibDecoder::default(),
            msc_handler: MscHandler::new(
                DEFAULT_DAB_PLUS_BITRATE,
                DecoderFactory::create(config.aac_decoder),
            ),
            phase_reference: PhaseReference::default(),
            time_syncer: TimeSyncer::default(),
            ofdm_decoder: OfdmDecoder::default(),
            config,
        }
    }

    pub fn run(&mut self, metadata: &mut MetadataWriter, pcm: &mut PcmOutput) -> Result<()> {
        info!("receive chain initialized");
        let device = match RtlSdrDevice::open(&self.config.device_options()) {
            Ok(device) => device,
            Err(err) => {
                warn!(error = %err, "RTL-SDR input unavailable; returning without PCM output");
                return Ok(());
            }
        };

        metadata.write_service(self.config.sid, self.config.label.as_deref().unwrap_or(""))?;

        let mut reader = SampleReader::new(device);
        let iq = reader.read_iq_block(2 * 196_608)?;
        let sync_start = match self.time_syncer.push(&iq) {
            Some(start) => start,
            None => {
                warn!("no DAB null-symbol sync marker found in current capture window");
                return Ok(());
            }
        };

        if sync_start + TU >= iq.len() {
            warn!("capture window ended before the first synchronized OFDM symbol");
            return Ok(());
        }

        self.ofdm_decoder
            .store_reference_symbol_0(&iq[sync_start..sync_start + TU]);

        let frame = &iq[sync_start..];
        for ofdm_symbol_idx in 1usize..76 {
            let symbol_start = TU + (ofdm_symbol_idx - 1) * TS;
            let symbol_end = symbol_start + TS;
            if symbol_end > frame.len() {
                break;
            }

            let symbol = &frame[symbol_start..symbol_end];
            let phase_corr = self.phase_reference.analyze(symbol);
            let _fine_freq_error_hz = self.phase_reference.last_freq_error_hz();
            let soft_bits = self.ofdm_decoder.process_symbol(symbol);

            if ofdm_symbol_idx <= 3 {
                for fib in self.fic_decoder.push_soft_bits(&soft_bits) {
                    self.fib_decoder.process_fib(&fib);
                }
            } else {
                let samples = self.msc_handler.process_block(&soft_bits)?;
                if !samples.is_empty() {
                    pcm.write_interleaved(&samples)?;
                }
            }

            if ofdm_symbol_idx == 1 {
                info!(phase_corr_rad = phase_corr, "OFDM decode started");
            }
        }

        if let Some(label) = self.fib_decoder.ensemble_label() {
            metadata.write_ensemble(0, label)?;
        }
        if let Some(label) = self.fib_decoder.first_service_label() {
            metadata.write_service(self.config.sid, label)?;
        }
        if let Some(dl) = self.msc_handler.last_dynamic_label() {
            metadata.write_dynamic_label(dl)?;
        }
        let _slide_base64_requested = self.config.slide_base64;
        if let Some(dir) = &self.config.slide_dir {
            let _ = metadata.save_slide_to_dir(dir, ".keep", &[]);
        }

        info!(channel = %self.config.channel, fic_decode_ratio = self.fic_decoder.decode_ratio_percent(), service_count = self.fib_decoder.service_count(), "receive pass completed");
        Ok(())
    }
}
