// DAB Pipeline - adapted from eti-generator.cpp (eti-cmdline)
//
// `DabPipeline` drives the OFDM → FIC/CIF processing chain and emits
// `DabFrame` values via a bounded channel.

use tracing::warn;

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{self, TrySendError};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;

use crate::pipeline::dab_frame::{DabFrame, SubchannelFrame};
use crate::pipeline::dab_params::DabParams;
use crate::pipeline::fic_handler::FicHandler;
use crate::pipeline::protection::{EepProtection, Protection, UepProtection};
use rayon::prelude::*;
use smallvec::SmallVec;

/// Callback invoked with ensemble name and EId when FIC is decoded.
type EnsembleCb = Option<Arc<dyn Fn(&str, u32) + Send + Sync>>;
/// Callback invoked with programme name and SId when FIC is decoded.
type ProgramCb = Option<Arc<dyn Fn(&str, i32) + Send + Sync>>;
/// Callback invoked with FIC quality percentage after each FIC frame.
type FicQualityCb = Option<Arc<dyn Fn(i16) + Send + Sync>>;

const RING_CAPACITY: usize = 512;
const INLINE_SUBCH: usize = 8;

struct RingSlot {
    blkno: i16,
    data: Vec<i16>,
}

struct SpscRing {
    slots: Vec<std::cell::UnsafeCell<RingSlot>>,
    write_pos: AtomicUsize,
    read_pos: AtomicUsize,
    slot_size: usize,
    /// Paired with `wait_condvar` so the consumer can block instead of
    /// spinning with `sleep(100 µs)` when the ring is empty.
    wait_mutex: Mutex<()>,
    wait_condvar: Condvar,
    /// Set by `notify()` (shutdown path) to unblock `wait_non_empty()` even
    /// when no data has been pushed into the ring.
    wake_requested: AtomicBool,
}

unsafe impl Sync for SpscRing {}
unsafe impl Send for SpscRing {}

impl SpscRing {
    fn new(slot_size: usize) -> Self {
        let mut slots = Vec::with_capacity(RING_CAPACITY);
        for _ in 0..RING_CAPACITY {
            slots.push(std::cell::UnsafeCell::new(RingSlot {
                blkno: 0,
                data: vec![0i16; slot_size],
            }));
        }
        SpscRing {
            slots,
            write_pos: AtomicUsize::new(0),
            read_pos: AtomicUsize::new(0),
            slot_size,
            wait_mutex: Mutex::new(()),
            wait_condvar: Condvar::new(),
            wake_requested: AtomicBool::new(false),
        }
    }

    fn try_push(&self, blkno: i16, src: &[i16]) -> bool {
        let wp = self.write_pos.load(Ordering::Relaxed);
        let rp = self.read_pos.load(Ordering::Acquire);
        let next = (wp + 1) % RING_CAPACITY;
        if next == rp {
            return false;
        }
        let slot = unsafe { &mut *self.slots[wp].get() };
        slot.blkno = blkno;
        let len = src.len().min(self.slot_size);
        slot.data[..len].copy_from_slice(&src[..len]);
        self.write_pos.store(next, Ordering::Release);
        // Wake the consumer if it is waiting in wait_non_empty().
        // Calling notify_one() without holding wait_mutex is intentional:
        // wait_non_empty() re-checks write_pos under Acquire ordering AFTER
        // it acquires wait_mutex, so a missed notification at most delays the
        // consumer by one additional push cycle (~1.25 ms for DAB Mode I),
        // which is far better than the previous 100 µs busy-poll.
        self.wait_condvar.notify_one();
        true
    }

    /// Block the calling thread until at least one slot is available to pop,
    /// or until `notify()` is called (e.g. on shutdown).
    fn wait_non_empty(&self) {
        let guard = self.wait_mutex.lock().unwrap();
        drop(
            self.wait_condvar
                .wait_while(guard, |_| {
                    // Re-check under the mutex so we never miss a wakeup that
                    // arrived between try_pop() returning None and this call.
                    // Also exit when wake_requested is set (shutdown path).
                    self.write_pos.load(Ordering::Acquire) == self.read_pos.load(Ordering::Relaxed)
                        && !self.wake_requested.load(Ordering::Acquire)
                })
                .unwrap(),
        );
    }

    /// Wake the consumer unconditionally — used on shutdown so a thread
    /// blocked in `wait_non_empty()` can observe `running == false` and exit.
    fn notify(&self) {
        self.wake_requested.store(true, Ordering::Release);
        self.wait_condvar.notify_one();
    }

    fn try_pop(&self) -> Option<(i16, &[i16])> {
        let rp = self.read_pos.load(Ordering::Relaxed);
        let wp = self.write_pos.load(Ordering::Acquire);
        if rp == wp {
            return None;
        }
        let slot = unsafe { &*self.slots[rp].get() };
        Some((slot.blkno, &slot.data[..self.slot_size]))
    }

    fn pop_commit(&self) {
        let rp = self.read_pos.load(Ordering::Relaxed);
        self.read_pos
            .store((rp + 1) % RING_CAPACITY, Ordering::Release);
    }
}

const CU_SIZE: usize = 4 * 16;

/// OFDM → FIC + CIF processing pipeline.
///
/// Receives OFDM soft-bits, performs FIC Viterbi decoding (ETSI EN 300 401 §11),
/// CIF interleaving (§14.6) and EEP/UEP protection, then emits one `DabFrame`
/// per completed CIF via a bounded mpsc channel.
pub struct DabPipeline {
    ring: Arc<SpscRing>,
    thread_handle: Option<thread::JoinHandle<()>>,
    running: Arc<AtomicBool>,
    processing: Arc<AtomicBool>,
    bits_per_block: usize,
    ensemble_cb: EnsembleCb,
    program_cb: ProgramCb,
    fic_quality_cb: FicQualityCb,
}

impl DabPipeline {
    pub fn new(
        dab_mode: u8,
        sender: mpsc::SyncSender<DabFrame>,
        ensemble_cb: EnsembleCb,
        program_cb: ProgramCb,
        fic_quality_cb: FicQualityCb,
    ) -> Self {
        let params = DabParams::new(dab_mode);
        let bits_per_block = 2 * params.get_carriers();
        let running = Arc::new(AtomicBool::new(true));
        let processing = Arc::new(AtomicBool::new(false));
        let ring = Arc::new(SpscRing::new(bits_per_block));

        let r = running.clone();
        let p = processing.clone();
        let ring_rx = ring.clone();

        let ecb = ensemble_cb.clone();
        let pcb = program_cb.clone();
        let fqcb = fic_quality_cb.clone();
        let thread_handle = thread::spawn(move || {
            Self::run_loop(ring_rx, r, p, params, sender, ecb, pcb, fqcb);
        });

        DabPipeline {
            ring,
            thread_handle: Some(thread_handle),
            running,
            processing,
            bits_per_block,
            ensemble_cb,
            program_cb,
            fic_quality_cb,
        }
    }

    pub fn new_frame(&self) {}

    pub fn process_block(&self, softbits: &[i16], blkno: i16) {
        let copy_len = softbits.len().min(self.bits_per_block);
        if !self.ring.try_push(blkno, &softbits[..copy_len]) {
            warn!(blkno, "OFDM ring buffer full, dropping block");
        }
    }

    pub fn start_processing(&self) {
        self.processing.store(true, Ordering::SeqCst);
    }

    pub fn processing_flag(&self) -> Arc<AtomicBool> {
        self.processing.clone()
    }

    pub fn reset(&mut self, sender: mpsc::SyncSender<DabFrame>) {
        self.running.store(false, Ordering::SeqCst);
        self.ring.notify(); // unblock the consumer so it exits cleanly
        if let Some(handle) = self.thread_handle.take() {
            let _ = handle.join();
        }

        let ring = Arc::new(SpscRing::new(self.bits_per_block));
        self.ring = ring.clone();
        self.running.store(true, Ordering::SeqCst);
        self.processing.store(false, Ordering::SeqCst);

        let r = self.running.clone();
        let p = self.processing.clone();
        let params = DabParams::new(1);
        let ecb = self.ensemble_cb.clone();
        let pcb = self.program_cb.clone();
        let fqcb = self.fic_quality_cb.clone();
        self.thread_handle = Some(thread::spawn(move || {
            Self::run_loop(ring, r, p, params, sender, ecb, pcb, fqcb);
        }));
    }

    #[allow(clippy::too_many_arguments)]
    fn run_loop(
        ring: Arc<SpscRing>,
        running: Arc<AtomicBool>,
        processing: Arc<AtomicBool>,
        params: DabParams,
        sender: mpsc::SyncSender<DabFrame>,
        ensemble_cb: EnsembleCb,
        program_cb: ProgramCb,
        fic_quality_cb: FicQualityCb,
    ) {
        let bits_per_block = 2 * params.get_carriers();
        // Mode I: 18 MSC blocks per CIF — ETSI EN 300 401 §14.1
        let number_of_blocks_per_cif: usize = 18;
        // CIF time-interleaving map — ETSI EN 300 401 §14.6
        let interleave_map: [usize; 16] = [0, 8, 4, 12, 2, 10, 6, 14, 1, 9, 5, 13, 3, 11, 7, 15];

        let mut cif_in = vec![0i16; 55296];
        let mut cif_vector = vec![vec![0i16; 55296]; 16];
        let mut fib_vector = vec![[0u8; 96]; 16];
        let mut fib_valid = [false; 16];
        let mut fib_input = vec![0i16; 3 * bits_per_block];

        let mut prot_table: Vec<Option<Protection>> = (0..64).map(|_| None).collect();
        let mut descrambler: Vec<Option<Vec<u8>>> = (0..64).map(|_| None).collect();

        let mut index_out: usize = 0;
        let mut expected_block: i16 = 2;
        let mut amount: usize = 0;
        let mut minor: u32 = 0;
        let mut cif_count_hi: i16 = -1;
        let mut cif_count_lo: i16 = -1;
        let mut temp = vec![0i16; 55296];

        let mut my_fic_handler = FicHandler::new(&params);
        my_fic_handler.fib_processor.ensemble_name_cb = ensemble_cb;
        my_fic_handler.fib_processor.program_name_cb = program_cb;

        let mut fibs_bytes = vec![0u8; 4 * 768];

        while running.load(Ordering::SeqCst) {
            let (blkno, bdata) = match ring.try_pop() {
                Some(v) => v,
                None => {
                    // Block until the OFDM thread pushes a block, eliminating
                    // the previous 100 µs busy-poll sleep.
                    ring.wait_non_empty();
                    continue;
                }
            };

            if blkno != expected_block {
                warn!("got {}, expected {}", blkno, expected_block);
                // Reset only the within-frame position; the CIF interleaver
                // history (index_out, amount) is NOT reset: the 16-CIF sliding
                // window survives a single dropped block, and resetting it would
                // cause ~360 ms of unnecessary warm-up latency (15 × 24 ms).
                expected_block = 2;
                minor = 0;
                ring.pop_commit();
                continue;
            }

            expected_block += 1;
            if expected_block > params.get_l() as i16 {
                expected_block = 2;
            }

            // FIC blocks 2..4 — ETSI EN 300 401 §3.2.1
            if (2..=4).contains(&blkno) {
                // Reset quality counters at the start of each FIC frame so
                // get_fic_quality() reflects only the current frame, not a
                // cumulative average that masks recent degradation.
                if blkno == 2 {
                    my_fic_handler.reset_quality_counters();
                }
                let offset = (blkno - 2) as usize * bits_per_block;
                let copy_len = bits_per_block.min(bdata.len());
                fib_input[offset..offset + copy_len].copy_from_slice(&bdata[..copy_len]);

                if blkno == 4 {
                    let mut valid = [false; 4];
                    fibs_bytes.fill(0);
                    my_fic_handler.process_fic_block(&fib_input, &mut fibs_bytes, &mut valid);

                    for i in 0..4 {
                        fib_valid[(index_out + i) & 0x0F] = valid[i];
                        let slot = &mut fib_vector[(index_out + i) & 0x0F];
                        for (j, s) in slot.iter_mut().enumerate().take(96) {
                            let base = i * 768 + 8 * j;
                            *s = fibs_bytes[base..base + 8]
                                .iter()
                                .fold(0u8, |acc, &b| (acc << 1) | (b & 1));
                        }
                    }
                    minor = 0;
                    let (hi, lo) = my_fic_handler.get_cif_count();
                    cif_count_hi = hi;
                    cif_count_lo = lo;
                    if let Some(ref cb) = fic_quality_cb {
                        cb(my_fic_handler.get_fic_quality());
                    }
                }
                ring.pop_commit();
                continue;
            }

            // MSC blocks — ETSI EN 300 401 §14.6
            let cif_index = ((blkno - 5) as usize) % number_of_blocks_per_cif;
            let offset = cif_index * bits_per_block;
            let copy_len = bits_per_block.min(bdata.len());
            cif_in[offset..offset + copy_len].copy_from_slice(&bdata[..copy_len]);
            ring.pop_commit();

            if cif_index == number_of_blocks_per_cif - 1 {
                #[allow(clippy::manual_memcpy)]
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

                // Adjusted CIF counter — ETSI EN 300 401 §14.1
                let (adj_hi, adj_lo) = {
                    let mut lo = cif_count_lo as i32 + minor as i32;
                    let mut hi = cif_count_hi as i32;
                    if lo >= 250 {
                        lo %= 250;
                        hi += 1;
                    }
                    if hi >= 20 {
                        hi = 20;
                    }
                    (hi as u8, lo as u8)
                };

                let mut frame = DabFrame::new(fib_vector[index_out], adj_hi, adj_lo);

                if processing.load(Ordering::SeqCst) {
                    frame.subchannels = process_cif_to_frames(
                        &temp,
                        &my_fic_handler,
                        &mut prot_table,
                        &mut descrambler,
                    );
                }

                // During startup (processing = false) use a non-blocking try_send to
                // avoid stalling this thread while frame_rx is not yet drained.
                // Once processing starts (processing = true), use a blocking send so
                // the pipeline applies natural back-pressure on the downstream consumer.
                let send_err = if processing.load(Ordering::SeqCst) {
                    sender.send(frame).is_err()
                } else {
                    matches!(sender.try_send(frame), Err(TrySendError::Disconnected(_)))
                };
                if send_err {
                    return;
                }

                index_out = (index_out + 1) & 0x0F;
                minor += 1;
            }
        }
    }
}

impl Drop for DabPipeline {
    fn drop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
        // Wake the consumer so it can observe running == false and exit.
        self.ring.notify();
        if let Some(handle) = self.thread_handle.take() {
            let _ = handle.join();
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────

/// Deconvolve, descramble and pack all active sub-channels from one CIF.
/// Returns one `SubchannelFrame` per active sub-channel, in sub-channel ID order.
///
/// # Safety
/// Each parallel job accesses a unique index in `prot_table` and `descrambler`
/// via raw pointers; all accesses are disjoint.
fn process_cif_to_frames(
    input: &[i16],
    fic_handler: &FicHandler,
    prot_table: &mut [Option<Protection>],
    descrambler: &mut [Option<Vec<u8>>],
) -> SmallVec<[SubchannelFrame; INLINE_SUBCH]> {
    // Phase 1: sequential init of new sub-channels — ETSI EN 300 401 §11
    for i in 0..64 {
        let data = fic_handler.get_channel_info(i);
        if data.in_use && prot_table[i].is_none() {
            let bit_rate = data.bitrate as usize;
            let out_size = bit_rate * 24;

            prot_table[i] = Some(if data.uep_flag {
                Protection::Uep(UepProtection::new(data.bitrate, data.protlev))
            } else {
                Protection::Eep(EepProtection::new(data.bitrate, data.protlev))
            });

            let mut shift_register = [1u8; 9];
            let mut desc = vec![0u8; out_size];
            for d in desc.iter_mut().take(out_size) {
                let b = shift_register[8] ^ shift_register[4];
                for k in (1..9).rev() {
                    shift_register[k] = shift_register[k - 1];
                }
                shift_register[0] = b;
                *d = b;
            }
            descrambler[i] = Some(desc);
        }
    }

    // Phase 2: collect jobs
    struct SubchJob {
        idx: usize,
        subchid: u8,
        start: usize,
        size: usize,
        out_size: usize,
        byte_size: usize,
    }

    let mut jobs: SmallVec<[SubchJob; INLINE_SUBCH]> = SmallVec::new();
    for i in 0..64 {
        let data = fic_handler.get_channel_info(i);
        if data.in_use {
            let start = data.start_cu as usize * CU_SIZE;
            let size = data.size as usize * CU_SIZE;
            let bit_rate = data.bitrate as usize;
            let out_size = bit_rate * 24;
            jobs.push(SubchJob {
                idx: i,
                subchid: data.id as u8,
                start,
                size,
                out_size,
                byte_size: out_size / 8,
            });
        }
    }

    if jobs.is_empty() {
        return SmallVec::new();
    }

    // Phase 3: parallel deconvolve + descramble + pack → own Arc<[u8]>
    let input_ref: &[i16] = input;
    let prot_addr = prot_table.as_mut_ptr() as usize;
    let desc_addr = descrambler.as_ptr() as usize;

    let frames: Vec<SubchannelFrame> = jobs
        .par_iter()
        .map(|job| {
            thread_local! {
                static BUF: std::cell::RefCell<Vec<u8>> = const { std::cell::RefCell::new(Vec::new()) };
            }
            BUF.with(|buf| {
                let mut bit_buf = buf.borrow_mut();
                bit_buf.clear();
                bit_buf.resize(job.out_size, 0);

                let prot = unsafe { &mut *(prot_addr as *mut Option<Protection>).add(job.idx) };
                if let Some(ref mut p) = prot {
                    p.deconvolve(&input_ref[job.start..job.start + job.size], &mut bit_buf);
                }

                let desc = unsafe { &*(desc_addr as *const Option<Vec<u8>>).add(job.idx) };
                if let Some(ref d) = desc {
                    for j in 0..job.out_size {
                        bit_buf[j] ^= d[j];
                    }
                }

                let mut packed = vec![0u8; job.byte_size];
                pack_bits(&bit_buf[..job.out_size], &mut packed);

                SubchannelFrame {
                    subchid: job.subchid,
                    data: Arc::from(packed.as_slice()),
                }
            })
        })
        .collect();

    SmallVec::from_vec(frames)
}

/// Pack a bit array (0/1 bytes) into packed bytes.
/// LLVM auto-vectorises this pattern on x86 (SSSE3) and ARM (NEON).
fn pack_bits(bits: &[u8], out: &mut [u8]) {
    for (byte, chunk) in out.iter_mut().zip(bits.chunks_exact(8)) {
        *byte = chunk.iter().fold(0u8, |acc, &b| (acc << 1) | (b & 1));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spsc_ring_push_pop() {
        let ring = SpscRing::new(4);
        assert!(ring.try_pop().is_none());
        assert!(ring.try_push(7, &[1, 2, 3, 4]));
        let (blkno, data) = ring.try_pop().unwrap();
        assert_eq!(blkno, 7);
        assert_eq!(&data[..4], &[1, 2, 3, 4]);
        ring.pop_commit();
        assert!(ring.try_pop().is_none());
    }

    #[test]
    fn spsc_ring_full() {
        let ring = SpscRing::new(2);
        for i in 0..(RING_CAPACITY - 1) {
            assert!(ring.try_push(i as i16, &[0, 0]), "push {} failed", i);
            ring.try_pop();
            ring.pop_commit();
        }
    }

    #[test]
    fn spsc_ring_wraparound() {
        let ring = SpscRing::new(1);
        for round in 0..3 {
            for i in 0..(RING_CAPACITY - 1) {
                assert!(
                    ring.try_push(i as i16, &[round as i16]),
                    "round {} push {} failed",
                    round,
                    i
                );
                let (blkno, data) = ring.try_pop().unwrap();
                assert_eq!(blkno, i as i16);
                assert_eq!(data[0], round as i16);
                ring.pop_commit();
            }
        }
    }

    #[test]
    fn pack_bits_known_pattern() {
        let bits: Vec<u8> = vec![1, 0, 1, 1, 0, 0, 1, 0];
        let mut out = [0u8; 1];
        pack_bits(&bits, &mut out);
        assert_eq!(out[0], 0b10110010);
    }

    #[test]
    fn pack_bits_all_ones() {
        let bits = vec![1u8; 16];
        let mut out = [0u8; 2];
        pack_bits(&bits, &mut out);
        assert_eq!(out, [0xFF, 0xFF]);
    }

    #[test]
    fn pack_bits_all_zeros() {
        let bits = vec![0u8; 8];
        let mut out = [0u8; 1];
        pack_bits(&bits, &mut out);
        assert_eq!(out[0], 0x00);
    }

    #[test]
    fn pipeline_channel_capacity_four() {
        let (tx, rx) = mpsc::sync_channel::<DabFrame>(4);
        for i in 0u8..4 {
            tx.send(DabFrame::new([i; 96], 0, i)).unwrap();
        }
        for i in 0u8..4 {
            let f = rx.recv().unwrap();
            assert_eq!(f.cif_count_lo, i);
        }
    }

    /// A thread blocking in wait_non_empty() must be woken by try_push().
    #[test]
    fn spsc_ring_wait_non_empty_wakes_on_push() {
        use std::sync::Arc;
        let ring = Arc::new(SpscRing::new(4));
        let rx = ring.clone();

        let consumer = std::thread::spawn(move || {
            rx.wait_non_empty();
            rx.try_pop().is_some()
        });

        // Give the consumer thread time to enter wait_non_empty().
        std::thread::sleep(std::time::Duration::from_millis(20));
        ring.try_push(42, &[1, 2, 3, 4]);

        assert!(consumer.join().unwrap());
    }

    /// notify() must unblock wait_non_empty() even when the ring is still empty
    /// (simulates the shutdown path).
    #[test]
    fn spsc_ring_notify_unblocks_wait_on_shutdown() {
        use std::sync::Arc;
        let ring = Arc::new(SpscRing::new(4));
        let rx = ring.clone();

        let consumer = std::thread::spawn(move || {
            rx.wait_non_empty();
            rx.try_pop().is_none() // woken by notify(), no data pushed
        });

        std::thread::sleep(std::time::Duration::from_millis(20));
        ring.notify(); // wake without data (shutdown signal)

        assert!(consumer.join().unwrap()); // is_none() returned true
    }
}
