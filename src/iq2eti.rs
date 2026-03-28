// iq2eti subcommand - RTL-SDR IQ → ETI stream
// Extracted from the original main.rs

use std::io::Write;
use std::sync::atomic::{AtomicBool, AtomicI16, Ordering};
use std::sync::Arc;
use std::thread;

use clap::Args;
use tracing::{error, info, warn};
use tracing_subscriber::EnvFilter;

use eti_rtlsdr_rust::dab_constants::BAND_III;
use eti_rtlsdr_rust::device::rtlsdr_handler::RtlsdrHandler;
use eti_rtlsdr_rust::eti_handling::eti_generator::{EtiGenerator, EtiWriterFn};
use eti_rtlsdr_rust::ofdm::ofdm_processor::OfdmProcessor;
use eti_rtlsdr_rust::support::band_handler;

#[derive(Args, Debug)]
pub struct Iq2etiArgs {
    /// DAB channel (e.g., 5A, 6C, 11C, 12C)
    #[arg(short = 'C', long = "channel", default_value = "11C")]
    channel: String,

    /// Gain as percentage (0..100)
    #[arg(short = 'G', long = "gain", default_value_t = 50)]
    gain: i16,

    /// PPM correction
    #[arg(short = 'p', long = "ppm", default_value_t = 0)]
    ppm: i32,

    /// Auto-gain
    #[arg(short = 'Q', long = "autogain")]
    autogain: bool,

    /// Time sync timeout in seconds
    #[arg(short = 'd', long = "sync-time", default_value_t = 5)]
    sync_time: i16,

    /// Ensemble detection timeout in seconds
    #[arg(short = 'D', long = "detect-time", default_value_t = 10)]
    detect_time: i16,

    /// Output file (default stdout, use - for stdout)
    #[arg(short = 'O', long = "output")]
    output: Option<String>,

    /// Record time in seconds (-1 = infinite)
    #[arg(short = 't', long = "record-time", default_value_t = -1)]
    record_time: i32,

    /// Silent mode
    #[arg(short = 'S', long = "silent")]
    silent: bool,

    /// RTL-SDR device index
    #[arg(long = "device-index", default_value_t = 0)]
    device_index: u32,
}

pub fn run(args: Iq2etiArgs) {
    let filter = if args.silent {
        EnvFilter::new("off")
    } else {
        EnvFilter::new("info")
    };
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .with_writer(std::io::stderr)
        .init();

    let run = Arc::new(AtomicBool::new(false));
    let time_synced = Arc::new(AtomicBool::new(false));
    let ensemble_recognized = Arc::new(AtomicBool::new(false));
    let signal_noise = Arc::new(AtomicI16::new(0));
    let fic_success = Arc::new(AtomicI16::new(0));
    let running = Arc::new(AtomicBool::new(true));

    let run_ctrlc = run.clone();
    let running_ctrlc = running.clone();
    ctrlc::set_handler(move || {
        warn!("Signal caught, terminating!");
        run_ctrlc.store(false, Ordering::SeqCst);
        running_ctrlc.store(false, Ordering::SeqCst);
    })
    .expect("Error setting Ctrl-C handler");

    let channel = args.channel.to_uppercase();
    let freq = band_handler::frequency(BAND_III, &channel);
    if freq == -1 {
        error!("Cannot handle channel {}", channel);
        std::process::exit(4);
    }
    let frequency = freq as u32;
    info!("tunedFrequency = {}", frequency);

    let eti_writer: EtiWriterFn = if let Some(ref path) = args.output {
        if path == "-" {
            let counter = Arc::new(std::sync::atomic::AtomicU32::new(0));
            Box::new(move |data: &[u8]| {
                let stdout = std::io::stdout();
                let mut lock = stdout.lock();
                let _ = lock.write_all(data);
                let _ = lock.flush();
                let c = counter.fetch_add(1, Ordering::Relaxed);
                info!(frame = c, "ETI frame written");
            })
        } else {
            let file = std::fs::File::create(path).expect("Cannot create output file");
            let file = Arc::new(std::sync::Mutex::new(std::io::BufWriter::new(file)));
            let counter = Arc::new(std::sync::atomic::AtomicU32::new(0));
            Box::new(move |data: &[u8]| {
                let mut f = file.lock().unwrap();
                let _ = f.write_all(data);
                let c = counter.fetch_add(1, Ordering::Relaxed);
                info!(frame = c, "ETI frame written");
            })
        }
    } else {
        let counter = Arc::new(std::sync::atomic::AtomicU32::new(0));
        Box::new(move |data: &[u8]| {
            let stdout = std::io::stdout();
            let mut lock = stdout.lock();
            let _ = lock.write_all(data);
            let _ = lock.flush();
            let c = counter.fetch_add(1, Ordering::Relaxed);
            info!(frame = c, "ETI frame written");
        })
    };

    let mut input_device = match RtlsdrHandler::new(frequency, args.ppm, args.gain, args.autogain, args.device_index) {
        Ok(dev) => dev,
        Err(e) => {
            error!("Installing device failed: {}", e);
            std::process::exit(3);
        }
    };

    let er = ensemble_recognized.clone();
    let ensemble_cb: Option<Box<dyn Fn(&str, u32) + Send>> = Some(Box::new(move |name: &str, _eid: u32| {
        if !er.load(Ordering::SeqCst) {
            info!("ensemble {} detected", name);
            er.store(true, Ordering::SeqCst);
        }
    }));
    let program_cb: Option<Box<dyn Fn(&str, i32) + Send>> = Some(Box::new(move |name: &str, sid: i32| {
        info!("program\t {}\t 0x{:X} is in the list", name.trim(), sid);
    }));
    let fs = fic_success.clone();
    let fic_quality_cb: Option<Box<dyn Fn(i16) + Send>> = Some(Box::new(move |quality: i16| {
        fs.store(quality, Ordering::SeqCst);
    }));
    let eti_generator = EtiGenerator::new(1, eti_writer, ensemble_cb, program_cb, fic_quality_cb);
    let eti_processing_flag = eti_generator.processing_flag();

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

    let ofdm_thread = thread::spawn(move || {
        let mut eti_gen = eti_generator;
        ofdm_processor.run(&input_device, &mut eti_gen);
        (input_device, eti_gen)
    });

    let mut time_sync_time = args.sync_time;
    while !time_synced.load(Ordering::SeqCst) && time_sync_time > 0 {
        thread::sleep(std::time::Duration::from_secs(1));
        time_sync_time -= 1;
    }

    if !time_synced.load(Ordering::SeqCst) {
        warn!("There does not seem to be a DAB signal here");
        running.store(false, Ordering::SeqCst);
        let _ = ofdm_thread.join();
        std::process::exit(1);
    }
    info!("there might be a DAB signal here");

    let mut freq_sync_time = args.detect_time;
    while freq_sync_time > 0 {
        info!("still at most {} seconds to wait", freq_sync_time);
        thread::sleep(std::time::Duration::from_secs(1));
        freq_sync_time -= 1;
        if ensemble_recognized.load(Ordering::SeqCst) {
            break;
        }
    }

    info!("Starting ETI processing...");
    eti_processing_flag.store(true, Ordering::SeqCst);

    run.store(true, Ordering::SeqCst);
    let mut record_time = args.record_time;
    while run.load(Ordering::SeqCst) && (record_time == -1 || record_time > 0) {
        info!(
            snr = signal_noise.load(Ordering::SeqCst),
            fib_quality = fic_success.load(Ordering::SeqCst),
            "status"
        );
        thread::sleep(std::time::Duration::from_secs(1));
        if record_time != -1 {
            record_time -= 1;
        }
    }

    running.store(false, Ordering::SeqCst);
    info!("terminating");

    let result = ofdm_thread.join();
    if let Ok((mut dev, _eti)) = result {
        dev.stop_reader();
    }
}
