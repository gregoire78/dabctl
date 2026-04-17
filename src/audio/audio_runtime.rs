use crate::audio::aac_decoder::{AacDecoder, AudioFormat};
use crate::audio::fic_decoder::FicDecoder;
use crate::audio::pad_decoder::PadDecoder;
use crate::audio::pad_output::PadOutput;
use crate::audio::silence_filler::{
    advance_silence_deadline, silence_deadline_after_good_au, SilenceBuffer,
};
use crate::audio::superframe::{
    AccessUnit, PadData, SuperframeFilter, SuperframeFormat, SuperframeResult,
};
use crate::pcm_writer::PcmWriter;
use crate::pipeline::dab_frame::DabFrame;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicI16, AtomicI32, Ordering};
use std::sync::Arc;
use std::thread;
use tracing::{debug, error, info, warn};

#[cfg(feature = "fdk-aac")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecoderPreference {
    Faad2,
    FdkAac,
}

#[cfg(not(feature = "fdk-aac"))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecoderPreference {
    Faad2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ServiceSelection {
    Pending,
    UnsupportedDab { sid: u16 },
    DabPlus { sid: u16, subchid: u8 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct AudioFollowUpPlan {
    flush_deferred_silence: bool,
    missing_aus_to_fill: usize,
    fallback_silence_aus: usize,
}

#[derive(Clone)]
pub struct AudioCounters {
    pub frames_in: Arc<AtomicI32>,
    pub frames_no_subch: Arc<AtomicI32>,
    pub sync_ok: Arc<AtomicI32>,
    pub sync_fail: Arc<AtomicI32>,
    pub aus_decoded: Arc<AtomicI32>,
    pub silence_aus: Arc<AtomicI32>,
    pub service_events: Arc<AtomicI32>,
    pub dls_events: Arc<AtomicI32>,
    pub slide_events: Arc<AtomicI32>,
    pub rs_corrected: Arc<AtomicI32>,
    pub rs_uncorrectable: Arc<AtomicI32>,
    pub au_crc_fail: Arc<AtomicI32>,
}

pub struct AudioStatusSnapshot {
    pub snr: i16,
    pub fib_quality: i32,
    pub frames: i32,
    pub no_subch: i32,
    pub sync_ok: i32,
    pub sync_fail: i32,
    pub aus: i32,
    pub silence_aus: i32,
    pub service_events: i32,
    pub dls_events: i32,
    pub slide_events: i32,
    pub rs_corrected: i32,
    pub rs_uncorrectable: i32,
    pub au_crc_fail: i32,
    pub freq_offset_hz: i32,
    pub mppm: i32,
    pub gain_db_x10: i32,
}

pub struct AudioProcessorConfig {
    pub target_sid: u16,
    pub target_label: Option<String>,
    pub slide_dir: Option<PathBuf>,
    pub slide_base64: bool,
    pub disable_dyn_fic: bool,
    pub no_silence_fill: bool,
    pub decoder_preference: DecoderPreference,
}

pub struct StatusThreadConfig {
    pub running: Arc<AtomicBool>,
    pub signal_noise: Arc<AtomicI16>,
    pub fic_ok: Arc<AtomicI32>,
    pub fic_total: Arc<AtomicI32>,
    pub fic_quality_percent: Arc<AtomicI16>,
    pub freq_offset_hz: Arc<AtomicI32>,
    pub tuned_freq_hz: i32,
    pub gain_tenths: Arc<AtomicI32>,
}

pub struct AudioFrameProcessor {
    config: AudioProcessorConfig,
    counters: AudioCounters,
    fic_decoder: FicDecoder,
    pad_decoder: PadDecoder,
    pad_output: PadOutput,
    superframe: SuperframeFilter,
    aac_decoder: Option<AacDecoder>,
    active_format: Option<SuperframeFormat>,
    au_count: usize,
    silence_next: std::time::Instant,
    silence_buffer: SilenceBuffer,
    current_subchid: Option<u8>,
    selected_service_sid: Option<u16>,
    ensemble_announced: bool,
    service_announced: bool,
    unsupported_service_warned: bool,
}

impl Default for AudioCounters {
    fn default() -> Self {
        Self::new()
    }
}

impl AudioCounters {
    pub fn new() -> Self {
        Self {
            frames_in: Arc::new(AtomicI32::new(0)),
            frames_no_subch: Arc::new(AtomicI32::new(0)),
            sync_ok: Arc::new(AtomicI32::new(0)),
            sync_fail: Arc::new(AtomicI32::new(0)),
            aus_decoded: Arc::new(AtomicI32::new(0)),
            silence_aus: Arc::new(AtomicI32::new(0)),
            service_events: Arc::new(AtomicI32::new(0)),
            dls_events: Arc::new(AtomicI32::new(0)),
            slide_events: Arc::new(AtomicI32::new(0)),
            rs_corrected: Arc::new(AtomicI32::new(0)),
            rs_uncorrectable: Arc::new(AtomicI32::new(0)),
            au_crc_fail: Arc::new(AtomicI32::new(0)),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn snapshot_and_reset(
        &self,
        snr: i16,
        fib_quality: i32,
        freq_offset_hz: i32,
        tuned_freq_hz: i32,
        gain_db_x10: i32,
    ) -> AudioStatusSnapshot {
        let sync_ok = self.sync_ok.swap(0, Ordering::SeqCst);
        let offset_hz = if sync_ok > 0 { freq_offset_hz } else { 0 };
        let mppm = if sync_ok > 0 && tuned_freq_hz > 0 {
            ((offset_hz as i64) * 1_000_000_000i64 / (tuned_freq_hz as i64)) as i32
        } else {
            0
        };

        AudioStatusSnapshot {
            snr,
            fib_quality,
            frames: self.frames_in.swap(0, Ordering::SeqCst),
            no_subch: self.frames_no_subch.swap(0, Ordering::SeqCst),
            sync_ok,
            sync_fail: self.sync_fail.swap(0, Ordering::SeqCst),
            aus: self.aus_decoded.swap(0, Ordering::SeqCst),
            silence_aus: self.silence_aus.swap(0, Ordering::SeqCst),
            service_events: self.service_events.swap(0, Ordering::SeqCst),
            dls_events: self.dls_events.swap(0, Ordering::SeqCst),
            slide_events: self.slide_events.swap(0, Ordering::SeqCst),
            rs_corrected: self.rs_corrected.swap(0, Ordering::SeqCst),
            rs_uncorrectable: self.rs_uncorrectable.swap(0, Ordering::SeqCst),
            au_crc_fail: self.au_crc_fail.swap(0, Ordering::SeqCst),
            freq_offset_hz: offset_hz,
            mppm,
            gain_db_x10,
        }
    }
}

impl AudioStatusSnapshot {
    pub fn metadata_blackout(&self) -> bool {
        self.sync_fail > self.sync_ok && self.dls_events == 0 && self.slide_events == 0
    }
}

pub fn spawn_status_thread(counters: AudioCounters, config: StatusThreadConfig) {
    thread::spawn(move || {
        let StatusThreadConfig {
            running: status_run,
            signal_noise,
            fic_ok,
            fic_total,
            fic_quality_percent,
            freq_offset_hz,
            tuned_freq_hz,
            gain_tenths,
        } = config;
        const DROPOUT_WARN_SECS: u32 = 5;
        let mut consecutive_dropout_s: u32 = 0;

        while status_run.load(Ordering::SeqCst) {
            let gain_t = gain_tenths.load(Ordering::Relaxed);
            let gain_db_x10 = if gain_t >= 0 { gain_t } else { 0 };
            let _ = fic_ok.swap(0, Ordering::SeqCst);
            let _ = fic_total.swap(0, Ordering::SeqCst);
            let snapshot = counters.snapshot_and_reset(
                signal_noise.load(Ordering::SeqCst),
                i32::from(fic_quality_percent.load(Ordering::SeqCst)),
                freq_offset_hz.load(Ordering::Relaxed),
                tuned_freq_hz,
                gain_db_x10,
            );

            debug!(
                snr = snapshot.snr,
                fib_quality = snapshot.fib_quality,
                frames = snapshot.frames,
                no_subch = snapshot.no_subch,
                sync_ok = snapshot.sync_ok,
                sync_fail = snapshot.sync_fail,
                aus = snapshot.aus,
                silence_aus = snapshot.silence_aus,
                rs_corrected = snapshot.rs_corrected,
                rs_uncorrectable = snapshot.rs_uncorrectable,
                au_crc_fail = snapshot.au_crc_fail,
                service_events = snapshot.service_events,
                dls_events = snapshot.dls_events,
                slide_events = snapshot.slide_events,
                metadata_blackout = snapshot.metadata_blackout(),
                freq_offset_hz = snapshot.freq_offset_hz,
                mppm = snapshot.mppm,
                gain_db_x10 = snapshot.gain_db_x10,
                "status"
            );

            if snapshot.sync_fail > snapshot.sync_ok {
                consecutive_dropout_s += 1;
                if consecutive_dropout_s == DROPOUT_WARN_SECS {
                    let hint = if snapshot.freq_offset_hz == 0 && snapshot.snr < 6 {
                        " — weak RF signal, check antenna"
                    } else if snapshot.freq_offset_hz.abs() > 500 {
                        " — large frequency offset, try -p to adjust PPM"
                    } else {
                        " — OFDM sync lost, audio interrupted"
                    };
                    warn!(
                        "Signal degraded for {} s (snr={} dB, freq_offset={} Hz){}",
                        consecutive_dropout_s, snapshot.snr, snapshot.freq_offset_hz, hint,
                    );
                    if snapshot.metadata_blackout() {
                        warn!("Metadata blackout during dropout: no DLS/slide events in last 1 s");
                    }
                }
            } else {
                if consecutive_dropout_s >= DROPOUT_WARN_SECS {
                    info!(
                        "Signal recovered after {} s of degraded reception",
                        consecutive_dropout_s
                    );
                }
                consecutive_dropout_s = 0;
            }

            thread::sleep(std::time::Duration::from_secs(1));
        }
    });
}

impl AudioFrameProcessor {
    pub fn new(config: AudioProcessorConfig, counters: AudioCounters) -> Self {
        let mut pad_decoder = PadDecoder::new();
        pad_decoder.set_mot_app_type(12);
        Self {
            pad_output: PadOutput::new(config.slide_dir.clone(), config.slide_base64),
            config,
            counters,
            fic_decoder: FicDecoder::new(),
            pad_decoder,
            superframe: SuperframeFilter::new(),
            aac_decoder: None,
            active_format: None,
            au_count: 0,
            silence_next: std::time::Instant::now(),
            silence_buffer: SilenceBuffer::new(2),
            current_subchid: None,
            selected_service_sid: None,
            ensemble_announced: false,
            service_announced: false,
            unsupported_service_warned: false,
        }
    }

    pub fn process_frame(&mut self, frame: DabFrame, pcm_out: &PcmWriter) {
        self.fic_decoder.process(frame.fic_data.as_ref());
        self.announce_ensemble_if_ready();
        self.select_subchannel_if_needed();
        self.announce_service_if_ready();

        let Some(subchid) = self.current_subchid else {
            return;
        };

        self.counters.frames_in.fetch_add(1, Ordering::Relaxed);
        let Some(subchannel_data) = frame.subchannel_data(subchid) else {
            self.counters.frames_no_subch.fetch_add(1, Ordering::SeqCst);
            return;
        };

        self.process_selected_subchannel(frame.sync_lost, subchannel_data.as_ref(), pcm_out);
    }

    fn announce_ensemble_if_ready(&mut self) {
        if self.ensemble_announced {
            return;
        }

        if let Some(ref ens) = self.fic_decoder.ensemble {
            if !ens.label.is_empty() {
                self.pad_output
                    .write_ensemble(&ens.label, &ens.short_label, ens.eid);
                info!("ensemble ready: {} (0x{:04X})", ens.label.trim(), ens.eid);
                self.ensemble_announced = true;
            }
        }
    }

    fn select_subchannel_if_needed(&mut self) {
        if self.current_subchid.is_some() {
            return;
        }

        match select_service_subchid(
            &self.fic_decoder,
            self.config.target_sid,
            self.config.target_label.as_deref(),
        ) {
            ServiceSelection::Pending => {}
            ServiceSelection::UnsupportedDab { sid } => {
                if !self.unsupported_service_warned {
                    warn!(
                        "Service 0x{:04X} is DAB (MP2), only DAB+ is supported — skipping",
                        sid
                    );
                    self.unsupported_service_warned = true;
                }
            }
            ServiceSelection::DabPlus { sid, subchid } => {
                self.selected_service_sid = Some(sid);
                self.current_subchid = Some(subchid);
                info!("selected DAB+ sub-channel {}", subchid);
            }
        }
    }

    fn announce_service_if_ready(&mut self) {
        if self.service_announced {
            return;
        }

        let Some(selected_sid) = self.selected_service_sid else {
            return;
        };

        if let Some(svc) = self.fic_decoder.services.get(&selected_sid) {
            if !svc.label.is_empty() {
                self.pad_output
                    .write_service(&svc.label, &svc.short_label, svc.sid);
                self.counters.service_events.fetch_add(1, Ordering::Relaxed);
                info!("service ready: {} (0x{:04X})", svc.label.trim(), svc.sid);
                self.service_announced = true;
            }
        }
    }

    fn process_selected_subchannel(
        &mut self,
        sync_lost: bool,
        subchannel_data: &[u8],
        pcm_out: &PcmWriter,
    ) {
        if sync_lost {
            self.superframe.reset();
            debug!("superframe accumulator reset after OFDM sync loss");
        }

        let result = self.superframe.feed(subchannel_data);
        self.record_superframe_counters(&result);

        if let Some(ref fmt) = result.format {
            self.configure_decoder_if_needed(fmt);
        }

        let mut decoded_this_frame = 0usize;
        if let Some(dec) = self.aac_decoder.as_mut() {
            decoded_this_frame = Self::decode_access_units(
                dec,
                &result.access_units,
                pcm_out,
                &self.counters,
                &mut self.silence_next,
            );
        }

        let now = std::time::Instant::now();
        let plan = plan_audio_follow_up(
            &result,
            self.au_count,
            decoded_this_frame,
            self.config.no_silence_fill,
            now >= self.silence_next,
        );

        if plan.flush_deferred_silence {
            Self::push_frames(pcm_out, &self.counters, self.silence_buffer.flush(), true);
        }

        if let Some(dec) = self.aac_decoder.as_mut() {
            Self::emit_silence_fill(dec, plan.missing_aus_to_fill, pcm_out, &self.counters);
            Self::queue_fallback_silence(dec, plan.fallback_silence_aus, &mut self.silence_buffer);
            if plan.fallback_silence_aus > 0 {
                self.silence_next = advance_silence_deadline(self.silence_next, now);
            }
        }

        Self::push_frames(pcm_out, &self.counters, self.silence_buffer.tick(), true);
        self.emit_pad_metadata(&result.pad_data);
    }

    fn record_superframe_counters(&self, result: &SuperframeResult) {
        if result.sync_ok {
            self.counters.sync_ok.fetch_add(1, Ordering::Relaxed);
        }
        if result.sync_fail {
            self.counters.sync_fail.fetch_add(1, Ordering::Relaxed);
        }
        if result.rs_corrected > 0 {
            self.counters
                .rs_corrected
                .fetch_add(result.rs_corrected as i32, Ordering::Relaxed);
        }
        if result.rs_uncorrectable {
            self.counters
                .rs_uncorrectable
                .fetch_add(1, Ordering::Relaxed);
        }
        if result.au_crc_fail > 0 {
            self.counters
                .au_crc_fail
                .fetch_add(result.au_crc_fail as i32, Ordering::Relaxed);
        }
    }

    fn configure_decoder_if_needed(&mut self, format: &SuperframeFormat) {
        self.au_count = format.number_of_access_units();
        if !should_refresh_decoder(
            self.active_format.as_ref(),
            self.aac_decoder.is_some(),
            format,
        ) {
            return;
        }

        let summary = build_audio_format_summary(format);
        let asc = format.build_asc();
        info!(
            "audio format detected: {} {} kHz {} ch",
            format.codec_name(),
            summary.sample_rate / 1000,
            summary.channels
        );

        match self.build_aac_decoder(&asc, summary.channels) {
            Ok(dec) => {
                info!(
                    "aac decoder ready: {} Hz {} ch",
                    summary.sample_rate, summary.channels
                );
                self.aac_decoder = Some(dec);
                self.active_format = Some(format.clone());
            }
            Err(e) => error!("AAC decoder init failed: {}", e),
        }
    }

    fn build_aac_decoder(&self, asc: &[u8], _expected_channels: u8) -> Result<AacDecoder, String> {
        #[cfg(not(feature = "fdk-aac"))]
        {
            AacDecoder::new(asc)
        }

        #[cfg(feature = "fdk-aac")]
        {
            match self.config.decoder_preference {
                DecoderPreference::Faad2 => AacDecoder::new_faad2(asc),
                DecoderPreference::FdkAac => AacDecoder::new_fdk_aac(asc, _expected_channels),
            }
        }
    }

    fn decode_access_units(
        dec: &mut AacDecoder,
        access_units: &[AccessUnit],
        pcm_out: &PcmWriter,
        counters: &AudioCounters,
        silence_next: &mut std::time::Instant,
    ) -> usize {
        let mut decoded_this_frame = 0usize;
        for au in access_units {
            if let Some(pcm) = dec.decode_frame(&au.data) {
                decoded_this_frame += 1;
                counters.aus_decoded.fetch_add(1, Ordering::Relaxed);
                pcm_out.push(pcm);
                *silence_next = silence_deadline_after_good_au(std::time::Instant::now());
            }
        }
        decoded_this_frame
    }

    fn emit_silence_fill(
        dec: &mut AacDecoder,
        count: usize,
        pcm_out: &PcmWriter,
        counters: &AudioCounters,
    ) {
        for _ in 0..count {
            if let Some(sil) = dec.decode_or_silence(None) {
                if pcm_out.push(sil) {
                    counters.silence_aus.fetch_add(1, Ordering::Relaxed);
                }
            }
        }
    }

    fn queue_fallback_silence(
        dec: &mut AacDecoder,
        count: usize,
        silence_buffer: &mut SilenceBuffer,
    ) {
        for _ in 0..count {
            if let Some(sil) = dec.decode_or_silence(None) {
                silence_buffer.push(sil);
            }
        }
    }

    fn push_frames<I>(
        pcm_out: &PcmWriter,
        counters: &AudioCounters,
        frames: I,
        count_as_silence: bool,
    ) where
        I: IntoIterator<Item = Vec<i16>>,
    {
        for frame in frames {
            if pcm_out.push(frame) && count_as_silence {
                counters.silence_aus.fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    fn emit_pad_metadata(&mut self, pad_data: &[PadData]) {
        for pad in pad_data {
            let pad_result =
                self.pad_decoder
                    .process_full(&pad.xpad, pad.xpad.len(), true, &pad.fpad);
            if let Some(ref label) = pad_result.dynamic_label {
                self.pad_output.write_dl(&label.text);
                self.counters.dls_events.fetch_add(1, Ordering::Relaxed);
                if !self.config.disable_dyn_fic {
                    debug!("dynamic label: {}", label.text);
                }
            }
            if let Some(ref slide) = pad_result.slide {
                self.pad_output.write_slide(slide);
                self.counters.slide_events.fetch_add(1, Ordering::Relaxed);
                if !self.config.disable_dyn_fic {
                    debug!(
                        "slideshow image: {} ({}, {} bytes)",
                        slide.content_name,
                        slide.mime_type(),
                        slide.data.len()
                    );
                }
            }
        }
    }
}

fn select_service_subchid(
    decoder: &FicDecoder,
    target_sid: u16,
    target_label: Option<&str>,
) -> ServiceSelection {
    if let Some(label) = target_label {
        if let Some(service) = decoder.find_service_by_label(label) {
            return selection_from_sid(decoder, service.sid);
        }
    }
    selection_from_sid(decoder, target_sid)
}

fn selection_from_sid(decoder: &FicDecoder, sid: u16) -> ServiceSelection {
    if let Some(audio) = decoder.find_audio_service(sid) {
        if audio.dab_plus {
            ServiceSelection::DabPlus {
                sid,
                subchid: audio.subchid,
            }
        } else {
            ServiceSelection::UnsupportedDab { sid }
        }
    } else {
        ServiceSelection::Pending
    }
}

fn build_audio_format_summary(format: &SuperframeFormat) -> AudioFormat {
    AudioFormat {
        sample_rate: format.sample_rate(),
        channels: format.channels(),
    }
}

fn should_refresh_decoder(
    current_format: Option<&SuperframeFormat>,
    has_decoder: bool,
    next_format: &SuperframeFormat,
) -> bool {
    !has_decoder || current_format != Some(next_format)
}

fn plan_audio_follow_up(
    result: &SuperframeResult,
    au_count: usize,
    decoded_this_frame: usize,
    no_silence_fill: bool,
    silence_deadline_reached: bool,
) -> AudioFollowUpPlan {
    let missing_aus_to_fill = if result.sync_ok && !no_silence_fill {
        au_count.saturating_sub(decoded_this_frame)
    } else {
        0
    };

    let fallback_silence_aus = if result.sync_fail && !no_silence_fill && silence_deadline_reached {
        au_count.max(1)
    } else {
        0
    };

    AudioFollowUpPlan {
        flush_deferred_silence: result.sync_ok,
        missing_aus_to_fill,
        fallback_silence_aus,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::fic_decoder::{AudioService, ServiceInfo};
    use std::collections::HashMap;

    #[test]
    fn metadata_blackout_requires_failures_and_no_events() {
        let snap = AudioStatusSnapshot {
            snr: 13,
            fib_quality: 90,
            frames: 40,
            no_subch: 0,
            sync_ok: 1,
            sync_fail: 5,
            aus: 0,
            silence_aus: 5,
            service_events: 0,
            dls_events: 0,
            slide_events: 0,
            rs_corrected: 0,
            rs_uncorrectable: 0,
            au_crc_fail: 0,
            freq_offset_hz: 0,
            mppm: 0,
            gain_db_x10: 207,
        };
        assert!(snap.metadata_blackout());
    }

    #[test]
    fn select_service_subchid_prefers_dab_plus_audio() {
        let mut decoder = FicDecoder::new();
        decoder.services.insert(
            0xF201,
            ServiceInfo {
                sid: 0xF201,
                label: "France Inter".to_string(),
                short_label: "FR INT".to_string(),
                primary_subchid: Some(5),
                audio_components: {
                    let mut map = HashMap::new();
                    map.insert(
                        5,
                        AudioService {
                            subchid: 5,
                            dab_plus: true,
                        },
                    );
                    map
                },
            },
        );

        assert_eq!(
            select_service_subchid(&decoder, 0xF201, None),
            ServiceSelection::DabPlus {
                sid: 0xF201,
                subchid: 5,
            }
        );
    }

    #[test]
    fn select_service_subchid_rejects_classic_dab() {
        let mut decoder = FicDecoder::new();
        decoder.services.insert(
            0xF201,
            ServiceInfo {
                sid: 0xF201,
                label: "Legacy".to_string(),
                short_label: "LEG".to_string(),
                primary_subchid: Some(3),
                audio_components: {
                    let mut map = HashMap::new();
                    map.insert(
                        3,
                        AudioService {
                            subchid: 3,
                            dab_plus: false,
                        },
                    );
                    map
                },
            },
        );

        assert_eq!(
            select_service_subchid(&decoder, 0xF201, None),
            ServiceSelection::UnsupportedDab { sid: 0xF201 }
        );
    }

    #[test]
    fn snapshot_and_reset_clears_counters() {
        let counters = AudioCounters::new();
        counters.frames_in.store(12, Ordering::SeqCst);
        counters.sync_ok.store(4, Ordering::SeqCst);
        counters.sync_fail.store(2, Ordering::SeqCst);
        let snap = counters.snapshot_and_reset(14, 80, -35, 199_360_000, 207);
        assert_eq!(snap.frames, 12);
        assert_eq!(snap.sync_ok, 4);
        assert_eq!(snap.sync_fail, 2);
        assert_eq!(snap.freq_offset_hz, -35);
        assert_eq!(snap.fib_quality, 80);
        assert_eq!(counters.frames_in.load(Ordering::SeqCst), 0);
        assert_eq!(counters.sync_ok.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn snapshot_and_reset_keeps_nonzero_fic_confidence_through_blackout() {
        let counters = AudioCounters::new();
        let snap = counters.snapshot_and_reset(13, 70, 0, 199_360_000, 207);
        assert_eq!(snap.fib_quality, 70);
    }

    #[test]
    fn build_audio_format_summary_uses_superframe_values() {
        let format = SuperframeFormat {
            dac_rate: true,
            sbr_flag: true,
            aac_channel_mode: true,
            ps_flag: false,
            mpeg_surround_config: 0,
        };
        let summary = build_audio_format_summary(&format);
        assert_eq!(summary.sample_rate, 48_000);
        assert_eq!(summary.channels, 2);
    }

    #[test]
    fn select_service_subchid_uses_label_when_available() {
        let mut decoder = FicDecoder::new();
        decoder.services.insert(
            0xF201,
            ServiceInfo {
                sid: 0xF201,
                label: "France Inter".to_string(),
                short_label: "FR INT".to_string(),
                primary_subchid: Some(9),
                audio_components: {
                    let mut map = HashMap::new();
                    map.insert(
                        9,
                        AudioService {
                            subchid: 9,
                            dab_plus: true,
                        },
                    );
                    map
                },
            },
        );

        assert_eq!(
            select_service_subchid(&decoder, 0xFFFF, Some("France Inter")),
            ServiceSelection::DabPlus {
                sid: 0xF201,
                subchid: 9,
            }
        );
    }

    #[test]
    fn audio_follow_up_plan_flushes_and_fills_missing_aus_on_sync_ok() {
        let result = SuperframeResult {
            access_units: Vec::new(),
            pad_data: Vec::new(),
            format: None,
            sync_ok: true,
            sync_fail: false,
            rs_corrected: 0,
            rs_uncorrectable: false,
            au_crc_fail: 0,
        };

        let plan = plan_audio_follow_up(&result, 3, 1, false, false);
        assert!(plan.flush_deferred_silence);
        assert_eq!(plan.missing_aus_to_fill, 2);
        assert_eq!(plan.fallback_silence_aus, 0);
    }

    #[test]
    fn audio_follow_up_plan_rate_limits_sync_fail_silence() {
        let result = SuperframeResult {
            access_units: Vec::new(),
            pad_data: Vec::new(),
            format: None,
            sync_ok: false,
            sync_fail: true,
            rs_corrected: 0,
            rs_uncorrectable: false,
            au_crc_fail: 0,
        };

        let waiting = plan_audio_follow_up(&result, 3, 0, false, false);
        assert_eq!(waiting.fallback_silence_aus, 0);

        let due = plan_audio_follow_up(&result, 3, 0, false, true);
        assert_eq!(due.fallback_silence_aus, 3);
    }

    #[test]
    fn should_refresh_decoder_only_when_needed() {
        let format = SuperframeFormat {
            dac_rate: true,
            sbr_flag: true,
            aac_channel_mode: true,
            ps_flag: false,
            mpeg_surround_config: 0,
        };

        assert!(should_refresh_decoder(None, false, &format));
        assert!(should_refresh_decoder(Some(&format), false, &format));
        assert!(!should_refresh_decoder(Some(&format), true, &format));

        let changed = SuperframeFormat {
            dac_rate: false,
            ..format.clone()
        };
        assert!(should_refresh_decoder(Some(&format), true, &changed));
    }
}
