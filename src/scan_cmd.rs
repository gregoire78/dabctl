// scan_cmd.rs — DAB channel scan: discover ensembles and services.
//
// Runs the OFDM + FIC pipeline on a single channel and prints every
// ensemble name and every audio service found within the detection
// timeout.  No audio output.  Exit code 0 = at least one service found,
// 1 = no DAB signal, 2 = signal found but FIC not decoded (too weak).

use std::io::IsTerminal;
use std::sync::atomic::{AtomicBool, AtomicI16, AtomicI32, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

use clap::Args;
use tracing::{error, info, warn};
use tracing_subscriber::EnvFilter;

use dabctl::device::rtlsdr_handler::{GainMode, RtlsdrHandler};
use dabctl::pipeline::band_handler;
use dabctl::pipeline::dab_constants::BAND_III;
use dabctl::pipeline::dab_frame::DabFrame;
use dabctl::pipeline::dab_pipeline::DabPipeline;
use dabctl::pipeline::ofdm::ofdm_processor::OfdmProcessor;

#[derive(Args, Debug)]
pub struct ScanArgs {
    /// DAB channel to scan (e.g., 5A, 6C, 11C, 12C)
    #[arg(short = 'C', long = "channel")]
    channel: String,

    /// Gain as percentage (0..100). If omitted, software AGC (SAGC) is used.
    #[arg(short = 'G', long = "gain", conflicts_with = "hardware_agc")]
    gain: Option<i16>,

    /// Use the RTL-SDR chip's built-in hardware AGC.
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

    /// RTL-SDR device index
    #[arg(long = "device-index", default_value_t = 0)]
    device_index: u32,

    /// Silent mode (no log output)
    #[arg(long = "silent")]
    silent: bool,
}

pub fn run(args: ScanArgs) {
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
    let fic_ok = Arc::new(AtomicI32::new(0));
    let fic_total = Arc::new(AtomicI32::new(0));

    let running_ctrlc = running.clone();
    ctrlc::set_handler(move || {
        warn!("Signal caught, terminating!");
        running_ctrlc.store(false, Ordering::SeqCst);
    })
    .expect("Error setting Ctrl-C handler");

    let channel = args.channel.to_uppercase();
    let freq = match band_handler::frequency(BAND_III, &channel) {
        Some(f) => f,
        None => {
            error!(
                "Unknown DAB channel '{}' — valid examples: 5A, 6C, 11C, 12C",
                channel
            );
            std::process::exit(4);
        }
    };

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

    // Channel used only to keep the pipeline thread alive; frames are discarded.
    let (frame_tx, frame_rx) = std::sync::mpsc::sync_channel::<DabFrame>(4);

    // ── Callbacks: log every discovery at INFO level ──────────────────────────
    let er = ensemble_recognized.clone();
    let ensemble_cb: Option<Arc<dyn Fn(&str, u32) + Send + Sync>> =
        Some(Arc::new(move |name: &str, eid: u32| {
            if !er.load(Ordering::SeqCst) {
                info!("Ensemble found: {} (EId 0x{:04X})", name.trim(), eid);
                er.store(true, Ordering::SeqCst);
            }
        }));

    // Collect services in a shared list so we can deduplicate and print them.
    let services: Arc<Mutex<Vec<(String, i32)>>> = Arc::new(Mutex::new(Vec::new()));
    let svcs = services.clone();
    let program_cb: Option<Arc<dyn Fn(&str, i32) + Send + Sync>> =
        Some(Arc::new(move |name: &str, sid: i32| {
            let label = name.trim().to_string();
            if label.is_empty() {
                return;
            }
            let mut list = svcs.lock().unwrap();
            if !list.iter().any(|(n, s)| *s == sid || n == &label) {
                info!("  Service: {} (SId 0x{:04X})", label, sid as u32);
                list.push((label, sid));
            }
        }));

    let fok = fic_ok.clone();
    let ftot = fic_total.clone();
    let fic_quality_cb: Option<Arc<dyn Fn(i16, i16) + Send + Sync>> =
        Some(Arc::new(move |success: i16, total: i16| {
            fok.fetch_add(i32::from(success), Ordering::Relaxed);
            ftot.fetch_add(i32::from(total), Ordering::Relaxed);
        }));

    let pipeline = DabPipeline::new(1, frame_tx, ensemble_cb, program_cb, fic_quality_cb);

    let ts = time_synced.clone();
    let sn = signal_noise.clone();
    let mut ofdm_processor = OfdmProcessor::new(1, 2, running.clone());
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

    let rtlsdr_running = input_device.reader_running();

    // ── Thread 1: OFDM → DabPipeline ─────────────────────────────────────────
    let ofdm_thread = thread::spawn(move || {
        let mut pl = pipeline;
        ofdm_processor.run(&input_device, &mut pl);
        (input_device, pl)
    });

    // ── Thread 2 (main): drain frames — we only care about FIC callbacks ─────
    // Drop frames immediately; the DabPipeline callbacks do the work.
    let running2 = running.clone();
    thread::spawn(move || {
        while running2.load(Ordering::SeqCst) {
            match frame_rx.recv_timeout(std::time::Duration::from_millis(50)) {
                Ok(_) => {}
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
            }
        }
    });

    // ── Wait for OFDM time sync ───────────────────────────────────────────────
    info!("Scanning channel {} for DAB services...", channel);
    let mut time_sync_time = args.sync_time;
    while !time_synced.load(Ordering::SeqCst) && time_sync_time > 0 {
        thread::sleep(std::time::Duration::from_secs(1));
        time_sync_time -= 1;
        if !running.load(Ordering::SeqCst) {
            break;
        }
    }

    if !time_synced.load(Ordering::SeqCst) {
        warn!("No DAB signal found on channel {}", channel);
        running.store(false, Ordering::SeqCst);
        rtlsdr_running.store(false, Ordering::SeqCst);
        let _ = ofdm_thread.join();
        std::process::exit(1);
    }

    info!("DAB signal detected on channel {}", channel);

    // ── Wait for ensemble/service detection ──────────────────────────────────
    let mut detect_time = args.detect_time;
    while detect_time > 0 && running.load(Ordering::SeqCst) {
        thread::sleep(std::time::Duration::from_secs(1));
        detect_time -= 1;
        // Stop early once ensemble and at least one service have been found.
        if ensemble_recognized.load(Ordering::SeqCst) {
            let count = services.lock().unwrap().len();
            if count > 0 && detect_time <= args.detect_time / 2 {
                break;
            }
        }
    }

    // ── Summary ───────────────────────────────────────────────────────────────
    let total_fic_ok = fic_ok.load(Ordering::Relaxed);
    let total_fic = fic_total.load(Ordering::Relaxed);

    if total_fic > 0 && total_fic_ok == 0 {
        let snr = signal_noise.load(Ordering::SeqCst);
        warn!(
            "FIC decoding failed on channel {} (snr={} dB) — \
             signal too weak, check antenna or increase gain with -G",
            channel, snr
        );
        running.store(false, Ordering::SeqCst);
        rtlsdr_running.store(false, Ordering::SeqCst);
        let _ = ofdm_thread.join();
        std::process::exit(2);
    }

    let found = services.lock().unwrap().len();
    if found == 0 {
        warn!("No services found on channel {}", channel);
    } else {
        info!("{} service(s) found on channel {}", found, channel);
        info!("To play a service: dabctl play -C {} -s <SId>", channel);
    }

    running.store(false, Ordering::SeqCst);
    rtlsdr_running.store(false, Ordering::SeqCst);
    let _ = ofdm_thread.join();
}
