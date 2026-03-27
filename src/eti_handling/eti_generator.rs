// ETI Generator - converted from eti-generator.cpp (eti-cmdline)
// Copyright (C) 2016 .. 2020 Jan van Katwijk - Lazy Chair Computing


use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::thread;

use crate::eti_handling::fic_handler::FicHandler;
use crate::eti_handling::protection::{EepProtection, Protection, UepProtection};
use crate::support::dab_params::DabParams;

static CRC_TAB_1021: [u16; 256] = [
    0x0000, 0x1021, 0x2042, 0x3063, 0x4084, 0x50a5, 0x60c6, 0x70e7,
    0x8108, 0x9129, 0xa14a, 0xb16b, 0xc18c, 0xd1ad, 0xe1ce, 0xf1ef,
    0x1231, 0x0210, 0x3273, 0x2252, 0x52b5, 0x4294, 0x72f7, 0x62d6,
    0x9339, 0x8318, 0xb37b, 0xa35a, 0xd3bd, 0xc39c, 0xf3ff, 0xe3de,
    0x2462, 0x3443, 0x0420, 0x1401, 0x64e6, 0x74c7, 0x44a4, 0x5485,
    0xa56a, 0xb54b, 0x8528, 0x9509, 0xe5ee, 0xf5cf, 0xc5ac, 0xd58d,
    0x3653, 0x2672, 0x1611, 0x0630, 0x76d7, 0x66f6, 0x5695, 0x46b4,
    0xb75b, 0xa77a, 0x9719, 0x8738, 0xf7df, 0xe7fe, 0xd79d, 0xc7bc,
    0x48c4, 0x58e5, 0x6886, 0x78a7, 0x0840, 0x1861, 0x2802, 0x3823,
    0xc9cc, 0xd9ed, 0xe98e, 0xf9af, 0x8948, 0x9969, 0xa90a, 0xb92b,
    0x5af5, 0x4ad4, 0x7ab7, 0x6a96, 0x1a71, 0x0a50, 0x3a33, 0x2a12,
    0xdbfd, 0xcbdc, 0xfbbf, 0xeb9e, 0x9b79, 0x8b58, 0xbb3b, 0xab1a,
    0x6ca6, 0x7c87, 0x4ce4, 0x5cc5, 0x2c22, 0x3c03, 0x0c60, 0x1c41,
    0xedae, 0xfd8f, 0xcdec, 0xddcd, 0xad2a, 0xbd0b, 0x8d68, 0x9d49,
    0x7e97, 0x6eb6, 0x5ed5, 0x4ef4, 0x3e13, 0x2e32, 0x1e51, 0x0e70,
    0xff9f, 0xefbe, 0xdfdd, 0xcffc, 0xbf1b, 0xaf3a, 0x9f59, 0x8f78,
    0x9188, 0x81a9, 0xb1ca, 0xa1eb, 0xd10c, 0xc12d, 0xf14e, 0xe16f,
    0x1080, 0x00a1, 0x30c2, 0x20e3, 0x5004, 0x4025, 0x7046, 0x6067,
    0x83b9, 0x9398, 0xa3fb, 0xb3da, 0xc33d, 0xd31c, 0xe37f, 0xf35e,
    0x02b1, 0x1290, 0x22f3, 0x32d2, 0x4235, 0x5214, 0x6277, 0x7256,
    0xb5ea, 0xa5cb, 0x95a8, 0x8589, 0xf56e, 0xe54f, 0xd52c, 0xc50d,
    0x34e2, 0x24c3, 0x14a0, 0x0481, 0x7466, 0x6447, 0x5424, 0x4405,
    0xa7db, 0xb7fa, 0x8799, 0x97b8, 0xe75f, 0xf77e, 0xc71d, 0xd73c,
    0x26d3, 0x36f2, 0x0691, 0x16b0, 0x6657, 0x7676, 0x4615, 0x5634,
    0xd94c, 0xc96d, 0xf90e, 0xe92f, 0x99c8, 0x89e9, 0xb98a, 0xa9ab,
    0x5844, 0x4865, 0x7806, 0x6827, 0x18c0, 0x08e1, 0x3882, 0x28a3,
    0xcb7d, 0xdb5c, 0xeb3f, 0xfb1e, 0x8bf9, 0x9bd8, 0xabbb, 0xbb9a,
    0x4a75, 0x5a54, 0x6a37, 0x7a16, 0x0af1, 0x1ad0, 0x2ab3, 0x3a92,
    0xfd2e, 0xed0f, 0xdd6c, 0xcd4d, 0xbdaa, 0xad8b, 0x9de8, 0x8dc9,
    0x7c26, 0x6c07, 0x5c64, 0x4c45, 0x3ca2, 0x2c83, 0x1ce0, 0x0cc1,
    0xef1f, 0xff3e, 0xcf5d, 0xdf7c, 0xaf9b, 0xbfba, 0x8fd9, 0x9ff8,
    0x6e17, 0x7e36, 0x4e55, 0x5e74, 0x2e93, 0x3eb2, 0x0ed1, 0x1ef0,
];

fn calc_crc(data: &[u8], initial: u16) -> u16 {
    let mut crc = initial;
    for &byte in data {
        let temp = ((byte as u16) ^ (crc >> 8)) & 0xff;
        crc = CRC_TAB_1021[temp as usize] ^ (crc << 8);
    }
    crc & 0xffff
}

const CU_SIZE: usize = 4 * 16;

struct BufferElement {
    blkno: i16,
    data: Vec<i16>,
}

pub type EtiWriterFn = Box<dyn Fn(&[u8]) + Send>;

pub struct EtiGenerator {
    tx: mpsc::SyncSender<BufferElement>,
    thread_handle: Option<thread::JoinHandle<()>>,
    running: Arc<AtomicBool>,
    processing: Arc<AtomicBool>,
    bits_per_block: usize,
}

impl EtiGenerator {
    pub fn new(
        dab_mode: u8,
        eti_writer: EtiWriterFn,
        ensemble_cb: Option<Box<dyn Fn(&str, u32) + Send>>,
        program_cb: Option<Box<dyn Fn(&str, i32) + Send>>,
    ) -> Self {
        let params = DabParams::new(dab_mode);
        let bits_per_block = 2 * params.get_carriers();
        let running = Arc::new(AtomicBool::new(true));
        let processing = Arc::new(AtomicBool::new(false));

        let (tx, rx) = mpsc::sync_channel::<BufferElement>(512);

        let r = running.clone();
        let p = processing.clone();

        let thread_handle = thread::spawn(move || {
            Self::run_loop(rx, r, p, params, eti_writer, ensemble_cb, program_cb);
        });

        EtiGenerator {
            tx,
            thread_handle: Some(thread_handle),
            running,
            processing,
            bits_per_block,
        }
    }

    pub fn new_frame(&self) {
        // Empty in C++ - placeholder for frame boundary notification
    }

    pub fn process_block(&self, softbits: &[i16], blkno: i16) {
        let mut data = vec![0i16; self.bits_per_block];
        let copy_len = softbits.len().min(self.bits_per_block);
        data[..copy_len].copy_from_slice(&softbits[..copy_len]);
        let _ = self.tx.try_send(BufferElement { blkno, data });
    }

    pub fn start_processing(&self) {
        eprintln!("yes, here we go");
        self.processing.store(true, Ordering::SeqCst);
    }

    /// Get a handle to the processing flag so external code can trigger processing start
    pub fn processing_flag(&self) -> Arc<AtomicBool> {
        self.processing.clone()
    }

    pub fn reset(&mut self, eti_writer: EtiWriterFn) {
        self.running.store(false, Ordering::SeqCst);
        // Drop old sender to unblock the receiver
        let (tx, rx) = mpsc::sync_channel::<BufferElement>(512);
        self.tx = tx;

        if let Some(handle) = self.thread_handle.take() {
            let _ = handle.join();
        }

        self.running.store(true, Ordering::SeqCst);
        self.processing.store(false, Ordering::SeqCst);

        let r = self.running.clone();
        let p = self.processing.clone();
        let params = DabParams::new(1);

        self.thread_handle = Some(thread::spawn(move || {
            Self::run_loop(rx, r, p, params, eti_writer, None, None);
        }));
    }

    fn run_loop(
        rx: mpsc::Receiver<BufferElement>,
        running: Arc<AtomicBool>,
        processing: Arc<AtomicBool>,
        params: DabParams,
        eti_writer: EtiWriterFn,
        ensemble_cb: Option<Box<dyn Fn(&str, u32) + Send>>,
        program_cb: Option<Box<dyn Fn(&str, i32) + Send>>,
    ) {
        let bits_per_block = 2 * params.get_carriers();
        let number_of_blocks_per_cif: usize = 18; // mode I
        let interleave_map: [usize; 16] = [0,8,4,12,2,10,6,14,1,9,5,13,3,11,7,15];

        let mut cif_in = vec![0i16; 55296];
        let mut cif_vector = vec![vec![0i16; 55296]; 16];
        let mut fib_vector = vec![vec![0u8; 96]; 16];
        let mut fib_valid = vec![false; 16];
        let mut fib_input = vec![0i16; 3 * bits_per_block];

        let mut prot_table: Vec<Option<Protection>> = (0..64).map(|_| None).collect();
        let mut descrambler: Vec<Option<Vec<u8>>> = (0..64).map(|_| None).collect();

        let mut index_out: usize = 0;
        let mut expected_block: i16 = 2;
        let mut amount: usize = 0;
        let mut minor: i16 = 0;
        let mut cif_count_hi: i16 = -1;
        let mut cif_count_lo: i16 = -1;
        let mut temp = vec![0i16; 55296];
        let mut the_vector = vec![0u8; 6144];

        let mut my_fic_handler = FicHandler::new(&params);
        my_fic_handler.fib_processor.ensemble_name_cb = ensemble_cb;
        my_fic_handler.fib_processor.program_name_cb = program_cb;

        while running.load(Ordering::SeqCst) {
            let b = match rx.recv_timeout(std::time::Duration::from_millis(100)) {
                Ok(b) => b,
                Err(mpsc::RecvTimeoutError::Timeout) => continue,
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            };

            if b.blkno != expected_block {
                eprintln!("got {}, expected {}", b.blkno, expected_block);
                expected_block = 2;
                index_out = 0;
                amount = 0;
                minor = 0;
                continue;
            }

            expected_block += 1;
            if expected_block > params.get_l() as i16 {
                expected_block = 2;
            }

            // Blocks 2..4 are FIC blocks
            if (2..=4).contains(&b.blkno) {
                let offset = (b.blkno - 2) as usize * bits_per_block;
                let copy_len = bits_per_block.min(b.data.len());
                fib_input[offset..offset + copy_len]
                    .copy_from_slice(&b.data[..copy_len]);

                if b.blkno == 4 {
                    let mut valid = [false; 4];
                    let mut fibs_bytes = vec![0u8; 4 * 768];
                    my_fic_handler.process_fic_block(
                        &fib_input,
                        &mut fibs_bytes,
                        &mut valid,
                    );

                    for i in 0..4 {
                        fib_valid[(index_out + i) & 0x0F] = valid[i];
                        for j in 0..96 {
                            let mut byte_val: u8 = 0;
                            for k in 0..8 {
                                byte_val <<= 1;
                                byte_val |= fibs_bytes[i * 768 + 8 * j + k] & 0x01;
                            }
                            fib_vector[(index_out + i) & 0x0F][j] = byte_val;
                        }
                    }
                    minor = 0;
                    let (hi, lo) = my_fic_handler.get_cif_count();
                    cif_count_hi = hi;
                    cif_count_lo = lo;
                }
                continue;
            }

            // MSC blocks
            let cif_index = ((b.blkno - 5) as usize) % number_of_blocks_per_cif;
            let offset = cif_index * bits_per_block;
            let copy_len = bits_per_block.min(b.data.len());
            cif_in[offset..offset + copy_len]
                .copy_from_slice(&b.data[..copy_len]);

            if cif_index == number_of_blocks_per_cif - 1 {
                // CIF complete - do interleaving
                for i in 0..(3072 * 18) {
                    let idx = interleave_map[i & 0x0F];
                    temp[i] = cif_vector[(index_out + idx) & 0x0F][i];
                    cif_vector[index_out & 0x0F][i] = cif_in[i];
                }

                if amount < 15 {
                    amount += 1;
                    index_out = (index_out + 1) & 0x0F;
                    minor = 0;
                    continue;
                }

                if cif_count_hi < 0 || cif_count_lo < 0 {
                    continue;
                }

                let fill_pointer = init_eti(
                    &mut the_vector,
                    cif_count_hi,
                    cif_count_lo,
                    minor,
                    &my_fic_handler,
                );
                let base = fill_pointer;
                let mut fp = fill_pointer;

                // Copy FIB data
                the_vector[fp..fp + 96]
                    .copy_from_slice(&fib_vector[index_out][..96]);
                fp += 96;

                if processing.load(Ordering::SeqCst) {
                    fp = process_cif(
                        &temp,
                        &mut the_vector,
                        fp,
                        &my_fic_handler,
                        &mut prot_table,
                        &mut descrambler,
                    );

                    // EOF - CRC
                    let crc = calc_crc(&the_vector[base..fp], 0xFFFF);
                    let crc = !crc;
                    the_vector[fp] = ((crc & 0xFF00) >> 8) as u8;
                    fp += 1;
                    the_vector[fp] = (crc & 0xFF) as u8;
                    fp += 1;

                    // EOF - RFU
                    the_vector[fp] = 0xFF; fp += 1;
                    the_vector[fp] = 0xFF; fp += 1;

                    // TIST
                    the_vector[fp] = 0xFF; fp += 1;
                    the_vector[fp] = 0xFF; fp += 1;
                    the_vector[fp] = 0xFF; fp += 1;
                    the_vector[fp] = 0xFF; fp += 1;

                    // Padding
                    for i in fp..6144 {
                        the_vector[i] = 0x55;
                    }

                    eti_writer(&the_vector[..6144]);
                }

                index_out = (index_out + 1) & 0x0F;
                minor += 1;
            }
        }
    }
}

impl Drop for EtiGenerator {
    fn drop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
        if let Some(handle) = self.thread_handle.take() {
            let _ = handle.join();
        }
    }
}

fn init_eti(
    eti: &mut [u8],
    cif_count_hi: i16,
    cif_count_lo: i16,
    minor: i16,
    fic_handler: &FicHandler,
) -> usize {
    let mut cif_lo = cif_count_lo + minor;
    let mut cif_hi = cif_count_hi;
    if cif_lo >= 250 {
        cif_lo %= 250;
        cif_hi += 1;
    }
    if cif_hi >= 20 {
        cif_hi = 20;
    }

    let mut fp: usize = 0;

    // SYNC()
    // ERR
    eti[fp] = 0xFF; fp += 1; // error level 0

    // FSYNC
    if (cif_lo & 1) != 0 {
        eti[fp] = 0xf8; fp += 1;
        eti[fp] = 0xc5; fp += 1;
        eti[fp] = 0x49; fp += 1;
    } else {
        eti[fp] = 0x07; fp += 1;
        eti[fp] = 0x3a; fp += 1;
        eti[fp] = 0xb6; fp += 1;
    }

    // LIDATA()
    // FC()
    eti[fp] = cif_lo as u8; fp += 1; // FCT

    let ficf: u8 = 1;
    let mut nst: u8 = 0;
    let mut fl: u16 = 0;

    for j in 0..64 {
        let data = fic_handler.get_channel_info(j);
        if data.in_use {
            nst += 1;
            fl += (data.bitrate as u16 * 3) / 4; // words
        }
    }

    fl += nst as u16 + 1 + 24; // STC + EOH + MST (FIC data, Mode 1!)

    eti[fp] = (ficf << 7) | nst; fp += 1;

    let fp_val = (((cif_hi as u32 * 250) + cif_lo as u32) % 8) as u8;
    let mid: u8 = 0x01; // Mode 1

    eti[fp] = (fp_val << 5) | (mid << 3) | ((fl >> 8) as u8 & 0x07);
    fp += 1;
    eti[fp] = (fl & 0xFF) as u8; fp += 1;

    // STC()
    for j in 0..64 {
        let data = fic_handler.get_channel_info(j);
        if data.in_use {
            let scid = data.id as u8;
            let sad = data.start_cu as u16;
            let tpl: u8 = if data.uep_flag {
                0x10 | ((data.protlev - 1) as u8)
            } else {
                0x20 | (data.protlev as u8)
            };
            let stl = (data.bitrate as u16 * 3) / 8;

            eti[fp] = (scid << 2) | ((sad >> 8) as u8 & 0x03); fp += 1;
            eti[fp] = (sad & 0xFF) as u8; fp += 1;
            eti[fp] = (tpl << 2) | ((stl >> 8) as u8 & 0x03); fp += 1;
            eti[fp] = (stl & 0xFF) as u8; fp += 1;
        }
    }

    // EOH()
    // MNSC
    eti[fp] = 0xFF; fp += 1;
    eti[fp] = 0xFF; fp += 1;

    // HCRC
    let hcrc = calc_crc(&eti[4..fp], 0xFFFF);
    let hcrc = !hcrc;
    eti[fp] = ((hcrc & 0xFF00) >> 8) as u8; fp += 1;
    eti[fp] = (hcrc & 0xFF) as u8; fp += 1;

    fp
}

fn process_cif(
    input: &[i16],
    output: &mut [u8],
    mut offset: usize,
    fic_handler: &FicHandler,
    prot_table: &mut [Option<Protection>],
    descrambler: &mut [Option<Vec<u8>>],
) -> usize {
    for i in 0..64 {
        let data = fic_handler.get_channel_info(i);
        if data.in_use {
            let start = data.start_cu as usize * CU_SIZE;
            let size = data.size as usize * CU_SIZE;
            let bit_rate = data.bitrate as usize;
            let out_size = bit_rate * 24;
            let byte_size = out_size / 8;

            // Create protection + descrambler if needed
            if prot_table[i].is_none() {
                prot_table[i] = Some(if data.uep_flag {
                    Protection::Uep(UepProtection::new(
                        data.bitrate as i16,
                        data.protlev as i16,
                    ))
                } else {
                    Protection::Eep(EepProtection::new(
                        data.bitrate as i16,
                        data.protlev as i16,
                    ))
                });

                // Build descrambler (energy dispersal)
                let mut shift_register = [1u8; 9];
                let mut desc = vec![0u8; out_size];
                for j in 0..out_size {
                    let b = shift_register[8] ^ shift_register[4];
                    for k in (1..9).rev() {
                        shift_register[k] = shift_register[k - 1];
                    }
                    shift_register[0] = b;
                    desc[j] = b;
                }
                descrambler[i] = Some(desc);
            }

            // Deconvolve
            let mut out_vector = vec![0u8; out_size];
            if let Some(ref mut prot) = prot_table[i] {
                prot.deconvolve(&input[start..start + size], &mut out_vector);
            }

            // Descramble (energy dispersal)
            if let Some(ref desc) = descrambler[i] {
                for j in 0..out_size {
                    out_vector[j] ^= desc[j];
                }
            }

            // Pack bits to bytes
            for j in 0..byte_size {
                let mut temp: u8 = 0;
                for k in 0..8 {
                    temp = (temp << 1) | (out_vector[j * 8 + k] & 0x01);
                }
                output[offset + j] = temp;
            }
            offset += byte_size;
        }
    }
    offset
}
