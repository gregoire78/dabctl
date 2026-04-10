// iq2pcm subcommand - RTL-SDR → PCM audio (direct pipeline, no ETI serialization)
//
// Architecture: two threads
//   Thread OFDM  : RtlsdrHandler → OfdmProcessor → DabPipeline
//                                      │  mpsc::SyncSender<DabFrame>  (capacity 4)
//                                      ↓
//   Thread Audio : DabFrame receiver → FicDecoder → SuperframeFilter → AacDecoder/Mp2Decoder
//                                                → PadDecoder → PadOutput (JSON fd 3)
//
// Back-pressure: the bounded channel (capacity 4 ≈ 100 ms) ensures the OFDM thread
// never allocates an unbounded queue of unprocessed frames when the audio decoder is slow.

use std::io::IsTerminal;
use std::sync::atomic::{AtomicBool, AtomicI16, AtomicI32, Ordering};
use std::sync::Arc;
use std::thread;

use clap::Args;
use tracing::{debug, error, info, warn};
use tracing_subscriber::EnvFilter;

use dabctl::audio::fic_decoder::FicDecoder;
use dabctl::audio::pad_decoder::PadDecoder;
use dabctl::audio::pad_output::PadOutput;
use dabctl::audio::silence_filler::{
    advance_silence_deadline, silence_deadline_after_good_au, SilenceBuffer,
};
use dabctl::audio::superframe::SuperframeFilter;
use dabctl::device::rtlsdr_handler::{GainMode, RtlsdrHandler};
use dabctl::pipeline::band_handler;
use dabctl::pipeline::dab_constants::BAND_III;
use dabctl::pipeline::dab_frame::DabFrame;
use dabctl::pipeline::dab_pipeline::DabPipeline;
use dabctl::pipeline::ofdm::ofdm_processor::OfdmProcessor;

/// Bounded channel capacity in frames (~24 ms per frame → ~100 ms back-pressure).
const CHANNEL_CAPACITY: usize = 4;

#[derive(Args, Debug)]
pub struct Iq2pcmArgs {
    /// DAB channel (e.g., 5A, 6C, 11C, 12C)
    #[arg(short = 'C', long = "channel")]
    channel: String,

    /// Gain as percentage (0..100). If omitted, software AGC (SAGC) is used.
    #[arg(short = 'G', long = "gain", conflicts_with = "hardware_agc")]
    gain: Option<i16>,

    /// Use the RTL-SDR chip's built-in hardware AGC.
    /// By default, software AGC (SAGC) is used. Mutually exclusive with -G.
    #[arg(long = "hardware-agc", conflicts_with = "gain")]
    hardware_agc: bool,

    /// PPM frequency correction
    #[arg(short = 'p', long = "ppm", default_value_t = 0)]
    ppm: i32,

    /// Time sync timeout in seconds
    #[arg(short = 'd', long = "sync-time", default_value_t = 5)]
    sync_time: i16,

    /// Ensemble detection timeout in seconds
    #[arg(short = 'D', long = "detect-time", default_value_t = 10)]
    detect_time: i16,

    /// Service ID to play (hex, e.g., 0xF2F8)
    #[arg(short = 's', long = "sid")]
    sid: String,

    /// Service label to play (matched by name)
    #[arg(short = 'l', long = "label")]
    label: Option<String>,

    /// Disable dynamic FIC messages on stderr
    #[arg(short = 'F', long = "disable-dyn-fic")]
    disable_dyn_fic: bool,

    /// Disable silence fill during radio fades (emit nothing instead of silence)
    #[arg(long = "no-silence-fill")]
    no_silence_fill: bool,

    /// Save slideshow images to this directory
    #[arg(short = 'S', long = "slide-dir")]
    slide_dir: Option<String>,

    /// Output slideshow as base64 JSON on fd 3
    #[arg(long = "slide-base64")]
    slide_base64: bool,

    /// Silent mode (no log output)
    #[arg(long = "silent")]
    silent: bool,

    /// RTL-SDR device index
    #[arg(long = "device-index", default_value_t = 0)]
    device_index: u32,

    /// AAC decoder backend: faad2 or fdk-aac.
    /// Only available when built with `--features fdk-aac`.
    #[cfg(feature = "fdk-aac")]
    #[arg(long = "aac-decoder", default_value = "fdk-aac", value_enum)]
    aac_decoder: AacBackend,
}

/// Runtime AAC backend selection. Available only with the `fdk-aac` Cargo feature.
#[cfg(feature = "fdk-aac")]
#[derive(Debug, Clone, clap::ValueEnum)]
pub enum AacBackend {
    /// libfaad2 — open-source, permissive licence (default without feature)
    Faad2,
    /// Fraunhofer FDK AAC — higher quality, non-free licence
    #[value(name = "fdk-aac")]
    FdkAac,
}

#[allow(clippy::type_complexity)]
pub fn run(args: Iq2pcmArgs) {
    let filter = if args.silent {
        EnvFilter::new("off")
    } else {
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info,rtl_sdr_rs=warn"))
    };
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .with_ansi(std::io::stderr().is_terminal())
        .with_writer(std::io::stderr)
        .init();

    let running = Arc::new(AtomicBool::new(true));
    let time_synced = Arc::new(AtomicBool::new(false));
    let ensemble_recognized = Arc::new(AtomicBool::new(false));
    let signal_noise = Arc::new(AtomicI16::new(0));
    let fic_success = Arc::new(AtomicI16::new(0));
    let run = Arc::new(AtomicBool::new(false));

    let running_ctrlc = running.clone();
    let run_ctrlc = run.clone();
    ctrlc::set_handler(move || {
        warn!("Signal caught, terminating!");
        running_ctrlc.store(false, Ordering::SeqCst);
        run_ctrlc.store(false, Ordering::SeqCst);
    })
    .expect("Error setting Ctrl-C handler");

    let channel = args.channel.to_uppercase();
    let freq = band_handler::frequency(BAND_III, &channel);
    if freq == -1 {
        error!("Cannot handle channel {}", channel);
        std::process::exit(4);
    }
    debug!("tunedFrequency = {}", freq as u32);

    let gain_mode = if args.hardware_agc {
        GainMode::Hardware
    } else if let Some(pct) = args.gain {
        GainMode::Manual(pct)
    } else {
        GainMode::Software
    };

    let mut input_device =
        match RtlsdrHandler::new(freq as u32, args.ppm, gain_mode, args.device_index) {
            Ok(dev) => dev,
            Err(e) => {
                error!("Installing device failed: {}", e);
                std::process::exit(3);
            }
        };

    // ── Build the in-memory DabFrame channel ──────────────────────────────────
    // Capacity 4: ~100 ms of back-pressure before the OFDM thread blocks.
    let (frame_tx, frame_rx) = std::sync::mpsc::sync_channel::<DabFrame>(CHANNEL_CAPACITY);

    let er = ensemble_recognized.clone();
    let ensemble_cb: Option<std::sync::Arc<dyn Fn(&str, u32) + Send + Sync>> =
        Some(std::sync::Arc::new(move |name: &str, _eid: u32| {
            if !er.load(Ordering::SeqCst) {
                info!("ensemble {} detected", name);
                er.store(true, Ordering::SeqCst);
            }
        }));
    let program_cb: Option<std::sync::Arc<dyn Fn(&str, i32) + Send + Sync>> =
        Some(std::sync::Arc::new(move |name: &str, sid: i32| {
            debug!("program {} (0x{:X}) is in the list", name.trim(), sid);
        }));
    let fs = fic_success.clone();
    let fic_quality_cb: Option<std::sync::Arc<dyn Fn(i16) + Send + Sync>> =
        Some(std::sync::Arc::new(move |quality: i16| {
            fs.store(quality, Ordering::SeqCst);
        }));

    let pipeline = DabPipeline::new(1, frame_tx, ensemble_cb, program_cb, fic_quality_cb);
    let pipeline_processing_flag = pipeline.processing_flag();

    let ts = time_synced.clone();
    let sn = signal_noise.clone();
    let mut ofdm_processor = OfdmProcessor::new(1, 2, 5, running.clone());
    ofdm_processor.set_sync_signal(move |synced| {
        ts.store(synced, Ordering::SeqCst);
    });
    ofdm_processor.set_show_snr(move |snr| {
        sn.store(snr, Ordering::SeqCst);
    });

    if !input_device.restart_reader() {
        error!("Failed to start RTL-SDR reader");
        std::process::exit(5);
    }

    // Capture the RTL-SDR worker's running flag before moving input_device into
    // the OFDM thread.  Setting this to false unblocks read_sync and lets the
    // worker exit, which in turn lets RtlsdrHandler::drop finish without stalling.
    let rtlsdr_running = input_device.reader_running();

    // ── Thread 1: OFDM → DabPipeline ─────────────────────────────────────────
    let ofdm_thread = thread::spawn(move || {
        let mut pl = pipeline;
        ofdm_processor.run(&input_device, &mut pl);
        (input_device, pl)
    });

    // ── Wait for time sync ────────────────────────────────────────────────────
    let mut time_sync_time = args.sync_time;
    while !time_synced.load(Ordering::SeqCst) && time_sync_time > 0 {
        thread::sleep(std::time::Duration::from_secs(1));
        time_sync_time -= 1;
    }
    if !time_synced.load(Ordering::SeqCst) {
        warn!("There does not seem to be a DAB signal here");
        // 1. Disconnect the frame channel so DabPipeline::run_loop unblocks.
        drop(frame_rx);
        // 2. Signal the OFDM processor to stop reading samples.
        running.store(false, Ordering::SeqCst);
        // 3. Signal the RTL-SDR USB worker to stop so its read_sync call exits;
        //    without this, RtlsdrHandler::drop (called after join) would block
        //    waiting for the current USB transfer to complete (~1 s).
        rtlsdr_running.store(false, Ordering::SeqCst);
        let _ = ofdm_thread.join();
        std::process::exit(1);
    }
    info!("there might be a DAB signal here");

    // ── Wait for ensemble detection ───────────────────────────────────────────
    let mut freq_sync_time = args.detect_time;
    while freq_sync_time > 0 {
        debug!("still at most {} seconds to wait", freq_sync_time);
        thread::sleep(std::time::Duration::from_secs(1));
        freq_sync_time -= 1;
        if ensemble_recognized.load(Ordering::SeqCst) {
            break;
        }
    }

    info!("Starting audio processing...");
    pipeline_processing_flag.store(true, Ordering::SeqCst);
    run.store(true, Ordering::SeqCst);

    // ── Thread 2 (main): audio decode loop ───────────────────────────────────
    let target_sid = parse_sid(&args.sid);
    let slide_dir = args.slide_dir.as_ref().map(std::path::PathBuf::from);

    let mut fic_decoder = FicDecoder::new();
    let mut pad_decoder = PadDecoder::new();
    pad_decoder.set_mot_app_type(12);
    let mut pad_output = PadOutput::new(slide_dir, args.slide_base64);
    let mut superframe = SuperframeFilter::new();
    let mut aac_decoder: Option<dabctl::audio::aac_decoder::AacDecoder> = None;
    // Silence fill: once we have decoded at least one AU, store a zero-filled
    // frame of the same size so we can substitute silence during radio fades
    // instead of emitting nothing (which causes buffer underruns downstream).
    // Rate-limited to one superframe period (~120 ms) so we never emit faster
    // than real-time even when sync_fail fires on every CIF frame.
    let mut au_silence: Option<Vec<i16>> = None;
    let mut au_count: usize = 0;
    // Tracks the deadline for the next silence superframe.
    // Initialised lazily on the first real AU output.
    let mut silence_next = std::time::Instant::now();
    // Deferred silence buffer (ETSI TS 102 563 §5.1).
    // Holds silence frames for 2 CIF ticks (~48 ms) before writing them.
    // A sync_ok within that window calls cancel(), preventing the pattern:
    //   AU_OK → silence → AU_OK  (silence interleaved with real audio)
    // instead of the correct:
    //   silence → silence → AU_OK  (silence precedes the recovered audio)
    // holdoff=2 (not 5) keeps the per-fade stream deficit to ~24 ms instead
    // of ~96 ms, which is enough to keep ffmpeg speed ≥ 0.997× even during
    // heavy fading.  holdoff=1 would flush on the very first tick and could
    // still interleave silence with audio during a 24 ms recovery window.
    let mut silence_buffer = SilenceBuffer::new(2);

    let mut current_subchid: Option<u8> = None;
    let mut ensemble_announced = false;
    let mut service_announced = false;

    // Spawn the PCM writer thread so that stdout writes never block the drain loop.
    // The drain loop pushes owned Vec<i16> frames non-blocking; the writer thread
    // absorbs transient pipe stalls without propagating backpressure up to the
    // OFDM ring buffer (ETSI TS 102 563 §5.2).
    let pcm_out = pcm_writer::spawn_pcm_writer(std::io::stdout());

    // Audio pipeline diagnostic counters
    let frames_in = Arc::new(AtomicI32::new(0));
    let frames_no_subch = Arc::new(AtomicI32::new(0));
    let sync_ok = Arc::new(AtomicI32::new(0));
    let sync_fail = Arc::new(AtomicI32::new(0));
    let aus_decoded = Arc::new(AtomicI32::new(0));
    // Counts silence AUs emitted during signal fades (one per AU substituted).
    let silence_aus = Arc::new(AtomicI32::new(0));

    // Monitor thread to log status and respect record_time
    let status_run = run.clone();
    let sn2 = signal_noise.clone();
    let fs2 = fic_success.clone();
    let running2 = running.clone();
    let c_fi = frames_in.clone();
    let c_fns = frames_no_subch.clone();
    let c_sok = sync_ok.clone();
    let c_sfail = sync_fail.clone();
    let c_au = aus_decoded.clone();
    let c_sil = silence_aus.clone();
    thread::spawn(move || {
        while status_run.load(Ordering::SeqCst) {
            let fi = c_fi.swap(0, Ordering::SeqCst);
            let fns = c_fns.swap(0, Ordering::SeqCst);
            let sok = c_sok.swap(0, Ordering::SeqCst);
            let sfail = c_sfail.swap(0, Ordering::SeqCst);
            let au = c_au.swap(0, Ordering::SeqCst);
            let sil = c_sil.swap(0, Ordering::SeqCst);
            debug!(
                snr = sn2.load(Ordering::SeqCst),
                fib_quality = fs2.load(Ordering::SeqCst),
                frames = fi,
                no_subch = fns,
                sync_ok = sok,
                sync_fail = sfail,
                aus = au,
                silence_aus = sil,
                "status"
            );
            thread::sleep(std::time::Duration::from_secs(1));
        }
        running2.store(false, Ordering::SeqCst);
    });

    // Main audio drain loop: receives DabFrame, decodes to PCM.
    // Uses recv_timeout instead of blocking iteration so that Ctrl-C (which sets
    // running=false) can break the loop even when the channel is empty.  Without
    // this, the loop would block forever: the DabPipeline internal thread keeps
    // frame_tx alive until DabPipeline::drop(), which only runs after join(), which
    // only runs after this loop — a deadlock.
    loop {
        if !running.load(Ordering::SeqCst) {
            break;
        }
        let frame = match frame_rx.recv_timeout(std::time::Duration::from_millis(50)) {
            Ok(f) => f,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        };

        // Process FIC to discover services — ETSI EN 300 401 §6.3
        fic_decoder.process(frame.fic_data.as_ref());

        // Announce ensemble on fd 3
        if !ensemble_announced {
            if let Some(ref ens) = fic_decoder.ensemble {
                if !ens.label.is_empty() {
                    pad_output.write_ensemble(&ens.label, &ens.short_label, ens.eid);
                    info!("Ensemble: {} (0x{:04X})", ens.label.trim(), ens.eid);
                    ensemble_announced = true;
                }
            }
        }

        // Resolve which sub-channel to play
        if current_subchid.is_none() {
            let audio_service = fic_decoder.find_audio_service(target_sid);

            if let Some(audio) = audio_service {
                if !audio.dab_plus {
                    warn!(
                        "Service 0x{:04X} is DAB (MP2), only DAB+ is supported — skipping",
                        target_sid
                    );
                    continue;
                }
                current_subchid = Some(audio.subchid);
                info!("Playing sub-channel {} (DAB+)", audio.subchid);
            }
        }

        // Announce service on fd 3
        if !service_announced && current_subchid.is_some() {
            if let Some(svc) = fic_decoder.services.get(&target_sid) {
                if !svc.label.is_empty() {
                    pad_output.write_service(&svc.label, &svc.short_label, svc.sid);
                    info!("Service: {} (0x{:04X})", svc.label.trim(), svc.sid);
                    service_announced = true;
                }
            }
        }

        let subchid = match current_subchid {
            Some(id) => id,
            None => continue,
        };

        frames_in.fetch_add(1, Ordering::Relaxed);

        let subchannel_arc = match frame.subchannel_data(subchid) {
            Some(arc) => arc,
            None => {
                frames_no_subch.fetch_add(1, Ordering::SeqCst);
                continue;
            }
        };
        let subchannel_data: &[u8] = subchannel_arc;

        // ── DAB+ path: superframe → RS → AU → AAC → PCM ──────────────────────
        {
            let result = superframe.feed(subchannel_data);
            if result.sync_ok {
                sync_ok.fetch_add(1, Ordering::Relaxed);
            }
            if result.sync_fail {
                sync_fail.fetch_add(1, Ordering::Relaxed);
            }

            if let Some(ref fmt) = result.format {
                au_count = fmt.number_of_access_units();
                let asc = fmt.build_asc();
                info!(
                    "Format: {} {}kHz {}ch",
                    fmt.codec_name(),
                    fmt.sample_rate() / 1000,
                    fmt.channels()
                );
                {
                    #[cfg(not(feature = "fdk-aac"))]
                    let init = dabctl::audio::aac_decoder::AacDecoder::new(&asc);

                    #[cfg(feature = "fdk-aac")]
                    let init = match args.aac_decoder {
                        AacBackend::Faad2 => {
                            dabctl::audio::aac_decoder::AacDecoder::new_faad2(&asc)
                        }
                        AacBackend::FdkAac => dabctl::audio::aac_decoder::AacDecoder::new_fdk_aac(
                            &asc,
                            fmt.channels(),
                        ),
                    };

                    match init {
                        Ok(dec) => {
                            // Sample rate and channels are not yet populated from the
                            // bitstream at decoder-open time for all backends (fdk-aac
                            // defers this to the first DecodeFrame call). Use the values
                            // parsed from the DAB+ superframe header instead, which are
                            // already available and authoritative at this point.
                            info!(
                                "AAC decoder ready: {}Hz {}ch",
                                fmt.sample_rate(),
                                fmt.channels()
                            );
                            aac_decoder = Some(dec);
                        }
                        Err(e) => error!("AAC decoder init failed: {}", e),
                    }
                }
            }

            // On sync_ok: flush buffered silence BEFORE writing real audio.
            // This keeps the stream chronologically ordered (silence precedes
            // the recovered audio) WITHOUT discarding any silence frames, so
            // the total PCM duration stays equal to wall-clock time (speed = 1.0×).
            if result.sync_ok {
                for sil_frame in silence_buffer.flush() {
                    if pcm_out.push(sil_frame) {
                        silence_aus.fetch_add(1, Ordering::Relaxed);
                    }
                }
            }

            if let Some(ref mut dec) = aac_decoder {
                for au in &result.access_units {
                    if let Some(pcm) = dec.decode_frame(&au.data) {
                        aus_decoded.fetch_add(1, Ordering::Relaxed);
                        // Capture the silence frame shape from the first successful decode.
                        if au_silence.is_none() {
                            au_silence = Some(vec![0i16; pcm.len()]);
                        }
                        pcm_out.push(pcm);
                        silence_next = silence_deadline_after_good_au(std::time::Instant::now());
                    }
                }
                // On sync_fail: push silence into the deferred buffer instead
                // of writing it immediately.  The buffer holds for 5 CIF ticks
                // (~120 ms) before flushing.  If sync_ok arrives within that
                // window, cancel() discards the pending silence so no gap
                // is inserted into the stream (ETSI TS 102 563 §5.1).
                if result.sync_fail && !args.no_silence_fill {
                    let now = std::time::Instant::now();
                    if now >= silence_next {
                        if let Some(ref silence) = au_silence {
                            let n = au_count.max(1);
                            for _ in 0..n {
                                silence_buffer.push(silence.clone());
                            }
                        }
                        silence_next = advance_silence_deadline(silence_next, now);
                    }
                }
            }

            // Flush deferred silence frames that have waited a full hold-off
            // (2 CIF ticks ≈ 48 ms) without a sync_ok cancelling them.
            for sil_frame in silence_buffer.tick() {
                if pcm_out.push(sil_frame) {
                    silence_aus.fetch_add(1, Ordering::Relaxed);
                }
            }

            for pad in &result.pad_data {
                let pad_result =
                    pad_decoder.process_full(&pad.xpad, pad.xpad.len(), true, &pad.fpad);
                if let Some(ref label) = pad_result.dynamic_label {
                    pad_output.write_dl(&label.text);
                    if !args.disable_dyn_fic {
                        debug!("DLS: {}", label.text);
                    }
                }
                if let Some(ref slide) = pad_result.slide {
                    pad_output.write_slide(slide);
                    if !args.disable_dyn_fic {
                        debug!(
                            "Slide: {} ({}, {} bytes)",
                            slide.content_name,
                            slide.mime_type(),
                            slide.data.len()
                        );
                    }
                }
            }
        }
    }

    running.store(false, Ordering::SeqCst);
    // Unblock the USB worker so RtlsdrHandler::drop does not stall on read_sync.
    rtlsdr_running.store(false, Ordering::SeqCst);
    info!("terminating");

    let result = ofdm_thread.join();
    if let Ok((mut dev, _pl)) = result {
        dev.stop_reader();
    }
}

use crate::pcm_writer;

/// Parse a SID string (supports "0xF201" and "F201" hex formats).
pub fn parse_sid(s: &str) -> u16 {
    let s = s.trim();
    let hex_str = s
        .strip_prefix("0x")
        .or_else(|| s.strip_prefix("0X"))
        .unwrap_or(s);
    u16::from_str_radix(hex_str, 16).unwrap_or_else(|_| {
        error!("Invalid SID: {}", s);
        std::process::exit(1);
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── SID parsing ───────────────────────────────────────────────────────────

    #[test]
    fn parse_sid_with_0x_prefix() {
        assert_eq!(parse_sid("0xF2F8"), 0xF2F8);
        assert_eq!(parse_sid("0xf2f8"), 0xF2F8);
        assert_eq!(parse_sid("0XF2F8"), 0xF2F8);
    }

    #[test]
    fn parse_sid_without_prefix() {
        assert_eq!(parse_sid("F201"), 0xF201);
        assert_eq!(parse_sid("0001"), 0x0001);
    }

    #[test]
    fn parse_sid_with_whitespace() {
        assert_eq!(parse_sid("  0xF2F8  "), 0xF2F8);
    }

    // ── CLI argument defaults ─────────────────────────────────────────────────

    #[test]
    fn required_args_channel_and_sid() {
        use clap::Parser;
        #[derive(Parser)]
        struct Wrapper {
            #[command(flatten)]
            inner: Iq2pcmArgs,
        }
        let args = Wrapper::parse_from(["test", "-C", "6C", "-s", "0xF2F8"]);
        assert_eq!(args.inner.channel, "6C");
        assert_eq!(args.inner.sid, "0xF2F8");
    }

    #[test]
    fn gain_defaults_to_autogain() {
        use clap::Parser;
        #[derive(Parser)]
        struct Wrapper {
            #[command(flatten)]
            inner: Iq2pcmArgs,
        }
        let args = Wrapper::parse_from(["test", "-C", "6C", "-s", "0xF2F8"]);
        assert!(args.inner.gain.is_none());
    }

    #[test]
    fn gain_explicit_value() {
        use clap::Parser;
        #[derive(Parser)]
        struct Wrapper {
            #[command(flatten)]
            inner: Iq2pcmArgs,
        }
        let args = Wrapper::parse_from(["test", "-C", "6C", "-s", "0xF2F8", "-G", "30"]);
        assert_eq!(args.inner.gain, Some(30));
    }

    // ── Channel capacity ──────────────────────────────────────────────────────

    #[test]
    fn channel_capacity_constant_is_four() {
        assert_eq!(CHANNEL_CAPACITY, 4);
    }

    // ── Silence fill flag ─────────────────────────────────────────────────────

    /// Silence fill is enabled by default; `--no-silence-fill` must not be set
    /// unless explicitly passed on the command line.
    #[test]
    fn no_silence_fill_defaults_to_false() {
        use clap::Parser;
        #[derive(Parser)]
        struct Wrapper {
            #[command(flatten)]
            inner: Iq2pcmArgs,
        }
        let args = Wrapper::parse_from(["test", "-C", "6C", "-s", "0xF2F8"]);
        assert!(!args.inner.no_silence_fill);
    }

    /// Passing `--no-silence-fill` must set the flag so the main loop emits
    /// no audio during radio fades instead of synthetic silence.
    #[test]
    fn no_silence_fill_can_be_enabled() {
        use clap::Parser;
        #[derive(Parser)]
        struct Wrapper {
            #[command(flatten)]
            inner: Iq2pcmArgs,
        }
        let args = Wrapper::parse_from(["test", "-C", "6C", "-s", "0xF2F8", "--no-silence-fill"]);
        assert!(args.inner.no_silence_fill);
    }

    /// `-G` (manual gain) and `--hardware-agc` must be mutually exclusive.
    #[test]
    fn gain_and_hardware_agc_are_mutually_exclusive() {
        use clap::Parser;
        #[derive(Parser)]
        struct Wrapper {
            #[command(flatten)]
            inner: Iq2pcmArgs,
        }
        let result = Wrapper::try_parse_from([
            "test",
            "-C",
            "6C",
            "-s",
            "0xF2F8",
            "-G",
            "50",
            "--hardware-agc",
        ]);
        assert!(
            result.is_err(),
            "clap must reject -G and --hardware-agc together"
        );
    }
}
