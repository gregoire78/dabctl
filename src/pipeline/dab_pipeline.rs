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

/// Wraps a value in its own 64-byte cache line so two adjacent hot
/// atomics (e.g. write_pos / read_pos) don't share a line and cause
/// mutual invalidation between the producer and consumer threads.
#[repr(align(64))]
struct CachePadded<T>(T);

struct RingSlot {
    blkno: i16,
    data: Vec<i16>,
}

struct SpscRing {
    slots: Vec<std::cell::UnsafeCell<RingSlot>>,
    /// Written only by the producer thread.
    write_pos: CachePadded<AtomicUsize>,
    /// Written only by the consumer thread.
    read_pos: CachePadded<AtomicUsize>,
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
            write_pos: CachePadded(AtomicUsize::new(0)),
            read_pos: CachePadded(AtomicUsize::new(0)),
            slot_size,
            wait_mutex: Mutex::new(()),
            wait_condvar: Condvar::new(),
            wake_requested: AtomicBool::new(false),
        }
    }

    fn try_push(&self, blkno: i16, src: &[i16]) -> bool {
        let wp = self.write_pos.0.load(Ordering::Relaxed);
        let rp = self.read_pos.0.load(Ordering::Acquire);
        let next = (wp + 1) % RING_CAPACITY;
        if next == rp {
            return false;
        }
        let slot = unsafe { &mut *self.slots[wp].get() };
        slot.blkno = blkno;
        let len = src.len().min(self.slot_size);
        slot.data[..len].copy_from_slice(&src[..len]);
        self.write_pos.0.store(next, Ordering::Release);
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
                    self.write_pos.0.load(Ordering::Acquire)
                        == self.read_pos.0.load(Ordering::Relaxed)
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
        let rp = self.read_pos.0.load(Ordering::Relaxed);
        let wp = self.write_pos.0.load(Ordering::Acquire);
        if rp == wp {
            return None;
        }
        let slot = unsafe { &*self.slots[rp].get() };
        Some((slot.blkno, &slot.data[..self.slot_size]))
    }

    fn pop_commit(&self) {
        let rp = self.read_pos.0.load(Ordering::Relaxed);
        self.read_pos
            .0
            .store((rp + 1) % RING_CAPACITY, Ordering::Release);
    }
}

const CU_SIZE: usize = 4 * 16;

/// Outcome returned by [`OfdmFrameSync::advance`].
#[derive(Debug, PartialEq, Eq)]
enum SyncAction {
    /// Block is in sequence — proceed with normal processing.
    Process,
    /// First mismatch detected: caller must log a WARN and discard the block.
    SyncLost,
    /// Still hunting for block 2 after a previous sync loss: discard silently.
    Discard,
    /// Block 2 received while in resync mode: sync is restored, process it.
    SyncRestored,
}

/// Tracks OFDM frame block sequencing state (blocks 2..L per DAB frame).
///
/// Encapsulates `expected_block` and `resyncing` so the logic is independently
/// testable without spinning up the full pipeline thread.
struct OfdmFrameSync {
    expected_block: i16,
    resyncing: bool,
    l: i16,
}

impl OfdmFrameSync {
    fn new(l: i16) -> Self {
        OfdmFrameSync {
            expected_block: 2,
            resyncing: false,
            l,
        }
    }

    /// Advance the state machine for the received `blkno`.
    ///
    /// Returns a [`SyncAction`] that tells the caller how to handle the block.
    /// On [`SyncAction::SyncLost`] the caller is responsible for logging the
    /// WARN (so the log message can include the block numbers).
    fn advance(&mut self, blkno: i16) -> SyncAction {
        if blkno != self.expected_block {
            self.expected_block = 2;
            if self.resyncing {
                return SyncAction::Discard;
            }
            self.resyncing = true;
            return SyncAction::SyncLost;
        }

        self.expected_block += 1;
        if self.expected_block > self.l {
            self.expected_block = 2;
        }

        if self.resyncing {
            self.resyncing = false;
            return SyncAction::SyncRestored;
        }

        SyncAction::Process
    }
}

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
    dab_mode: u8,
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
            dab_mode,
            bits_per_block,
            ensemble_cb,
            program_cb,
            fic_quality_cb,
        }
    }

    /// Called at the start of each OFDM null symbol / new DAB frame.
    /// Retained as a hook for future per-frame bookkeeping (e.g. AGC reset).
    pub fn new_frame(&self) {}

    pub fn process_block(&self, softbits: &[i16], blkno: i16) {
        let copy_len = softbits.len().min(self.bits_per_block);
        if !self.ring.try_push(blkno, &softbits[..copy_len]) {
            warn!(blkno, "OFDM ring buffer full, dropping block");
        }
    }

    pub fn start_processing(&self) {
        self.processing.store(true, Ordering::Release);
    }

    pub fn processing_flag(&self) -> Arc<AtomicBool> {
        self.processing.clone()
    }

    pub fn reset(&mut self, sender: mpsc::SyncSender<DabFrame>) {
        self.running.store(false, Ordering::Release);
        self.ring.notify(); // unblock the consumer so it exits cleanly
        if let Some(handle) = self.thread_handle.take() {
            let _ = handle.join();
        }

        let ring = Arc::new(SpscRing::new(self.bits_per_block));
        self.ring = ring.clone();
        self.running.store(true, Ordering::Release);
        self.processing.store(false, Ordering::Release);

        let r = self.running.clone();
        let p = self.processing.clone();
        let params = DabParams::new(self.dab_mode);
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

        let cif_buf_size = number_of_blocks_per_cif * bits_per_block;
        let mut cif_in = vec![0i16; cif_buf_size];
        let mut cif_vector = vec![vec![0i16; cif_buf_size]; 16];
        let mut fib_vector = vec![[0u8; 96]; 16];
        let mut fib_valid = [false; 16];
        let mut fib_input = vec![0i16; 3 * bits_per_block];

        let mut prot_table: Vec<Option<Protection>> = (0..64).map(|_| None).collect();
        let mut descrambler: Vec<Option<Vec<u8>>> = (0..64).map(|_| None).collect();

        let mut index_out: usize = 0;
        let mut frame_sync = OfdmFrameSync::new(params.get_l() as i16);
        let mut amount: usize = 0;
        let mut minor: u32 = 0;
        let mut cif_count_hi: i16 = -1;
        let mut cif_count_lo: i16 = -1;
        let mut temp = vec![0i16; cif_buf_size];

        let mut my_fic_handler = FicHandler::new(&params);
        my_fic_handler.fib_processor.ensemble_name_cb = ensemble_cb;
        my_fic_handler.fib_processor.program_name_cb = program_cb;

        let mut fibs_bytes = vec![0u8; 4 * 768];

        while running.load(Ordering::Acquire) {
            let (blkno, bdata) = match ring.try_pop() {
                Some(v) => v,
                None => {
                    // Block until the OFDM thread pushes a block, eliminating
                    // the previous 100 µs busy-poll sleep.
                    ring.wait_non_empty();
                    continue;
                }
            };

            match frame_sync.advance(blkno) {
                SyncAction::Process => {}
                SyncAction::SyncRestored => {
                    tracing::debug!(blkno, "OFDM frame sync restored");
                }
                SyncAction::SyncLost => {
                    // CIF interleaver history (index_out, amount) is NOT reset:
                    // the 16-CIF sliding window survives a single dropped block.
                    // Resetting it would cause ~360 ms of unnecessary warm-up
                    // latency (15 × 24 ms).
                    warn!(blkno, "OFDM frame sync lost, resyncing");
                    minor = 0;
                    ring.pop_commit();
                    continue;
                }
                SyncAction::Discard => {
                    ring.pop_commit();
                    continue;
                }
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
                // CIFCountHigh ∈ [0, 19], CIFCountLow ∈ [0, 249]; both wrap modulo.
                let (adj_hi, adj_lo) = adjust_cif_counter(cif_count_hi, cif_count_lo, minor);

                let mut frame = DabFrame::new(fib_vector[index_out], adj_hi, adj_lo);

                // Snapshot `processing` once so the subchannel fill and the
                // send-mode decision are always consistent for the same frame.
                let is_processing = processing.load(Ordering::Acquire);
                if is_processing {
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
                let send_err = if is_processing {
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
        self.running.store(false, Ordering::Release);
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
    // Single pass: initialise new sub-channels and collect active jobs.
    // Merging the previous two 0..64 loops halves the get_channel_info() calls.
    // ETSI EN 300 401 §11
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
        if !data.in_use {
            continue;
        }

        // Lazily initialise protection and descrambler tables on first use.
        if prot_table[i].is_none() {
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

        let start = data.start_cu as usize * CU_SIZE;
        let size = data.size as usize * CU_SIZE;
        let out_size = data.bitrate as usize * 24;
        jobs.push(SubchJob {
            idx: i,
            subchid: data.id as u8,
            start,
            size,
            out_size,
            byte_size: out_size / 8,
        });
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

/// Compute the adjusted CIF counter for an ETI frame.
///
/// Applies the offset `minor` to the decoded FIC counter `(hi, lo)` and wraps
/// both fields within their legal ranges per ETSI EN 300 401 §14.1:
/// - `CIFCountLow`  ∈ [0, 249]
/// - `CIFCountHigh` ∈ [0,  19]
fn adjust_cif_counter(cif_count_hi: i16, cif_count_lo: i16, minor: u32) -> (u8, u8) {
    let mut lo = cif_count_lo as i32 + minor as i32;
    let mut hi = cif_count_hi as i32;
    if lo >= 250 {
        lo %= 250;
        hi += 1;
    }
    hi %= 20;
    (hi as u8, lo as u8)
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

    // ── adjust_cif_counter ───────────────────────────────────────────────────
    // ETSI EN 300 401 §14.1: CIFCountLow ∈ [0,249], CIFCountHigh ∈ [0,19].

    #[test]
    fn cif_counter_no_overflow() {
        // No carry: lo stays below 250, hi unchanged.
        assert_eq!(adjust_cif_counter(0, 0, 0), (0, 0));
        assert_eq!(adjust_cif_counter(5, 100, 10), (5, 110));
    }

    #[test]
    fn cif_counter_lo_overflow_increments_hi() {
        // lo = 249 + 1 = 250 → lo wraps to 0, hi increments by 1.
        assert_eq!(adjust_cif_counter(0, 249, 1), (1, 0));
    }

    #[test]
    fn cif_counter_lo_overflow_large_minor() {
        // lo = 0 + 500 = 500 → 500 % 250 = 0, hi += 1.
        assert_eq!(adjust_cif_counter(3, 0, 500), (4, 0));
    }

    #[test]
    fn cif_counter_hi_wraps_at_20() {
        // hi = 19, lo carries → hi becomes 20 → wraps to 0.
        assert_eq!(adjust_cif_counter(19, 249, 1), (0, 0));
    }

    #[test]
    fn cif_counter_hi_wraps_correctly_across_20() {
        // hi = 19, lo = 0, minor = 0 — no carry, hi stays at 19.
        assert_eq!(adjust_cif_counter(19, 0, 0), (19, 0));
        // hi = 19, carry from lo pushes hi to 20 → wraps to 0.
        assert_eq!(adjust_cif_counter(19, 249, 1), (0, 0));
    }

    #[test]
    fn cif_counter_max_values_stay_in_range() {
        // Verify output is always within [0,19] × [0,249] for boundary inputs.
        let (hi, lo) = adjust_cif_counter(19, 249, 249);
        assert!(hi < 20, "hi={hi} out of range");
        assert!(lo < 250, "lo={lo} out of range");
    }

    // ── pack_bits edge cases ─────────────────────────────────────────────────

    #[test]
    fn pack_bits_partial_tail_is_ignored() {
        // 9 bits → only the first byte (8 bits) is packed; the 9th bit is ignored
        // because chunks_exact(8) drops the remainder.
        let bits: Vec<u8> = vec![1, 0, 0, 0, 0, 0, 0, 0, /*tail:*/ 1];
        let mut out = [0u8; 1];
        pack_bits(&bits, &mut out);
        assert_eq!(out[0], 0b10000000);
    }

    #[test]
    fn pack_bits_empty_input_produces_no_output() {
        let bits: Vec<u8> = vec![];
        let mut out = [0u8; 0];
        pack_bits(&bits, &mut out); // must not panic
    }

    // ── SpscRing capacity boundary ───────────────────────────────────────────

    #[test]
    fn spsc_ring_capacity_is_ring_capacity_minus_one() {
        // A ring of RING_CAPACITY slots holds RING_CAPACITY−1 items before
        // try_push returns false (one slot is always kept empty as sentinel).
        let ring = SpscRing::new(1);
        let mut pushed = 0usize;
        for i in 0..RING_CAPACITY + 2 {
            if ring.try_push(i as i16, &[0]) {
                pushed += 1;
            } else {
                break;
            }
        }
        assert_eq!(pushed, RING_CAPACITY - 1);
    }

    // ── OfdmFrameSync ────────────────────────────────────────────────────────

    /// L for DAB Mode I is 76 (blocks 2..=76).
    const L: i16 = 76;

    #[test]
    fn sync_normal_sequence_produces_process() {
        let mut s = OfdmFrameSync::new(L);
        // Blocks 2..=76 should all be Process on a clean start.
        for blkno in 2..=L {
            assert_eq!(s.advance(blkno), SyncAction::Process, "blkno={blkno}");
        }
        // After L the counter wraps back to 2; verify block 2 is again Process.
        assert_eq!(s.advance(2), SyncAction::Process);
    }

    #[test]
    fn sync_first_mismatch_emits_sync_lost() {
        let mut s = OfdmFrameSync::new(L);
        // Block 2 is fine.
        assert_eq!(s.advance(2), SyncAction::Process);
        // Block 99 is unexpected (expected 3) → SyncLost.
        assert_eq!(s.advance(99), SyncAction::SyncLost);
    }

    #[test]
    fn sync_second_mismatch_while_resyncing_emits_discard() {
        let mut s = OfdmFrameSync::new(L);
        s.advance(2); // normal
        s.advance(99); // SyncLost — now resyncing, expected_block reset to 2
                       // Any block that is not 2 while resyncing must be silently discarded.
        assert_eq!(s.advance(3), SyncAction::Discard);
        assert_eq!(s.advance(50), SyncAction::Discard);
    }

    #[test]
    fn sync_block_2_while_resyncing_emits_sync_restored() {
        let mut s = OfdmFrameSync::new(L);
        s.advance(2); // normal
        s.advance(99); // SyncLost
        s.advance(5); // Discard
                      // Receiving block 2 after sync loss must restore synchronisation.
        assert_eq!(s.advance(2), SyncAction::SyncRestored);
    }

    #[test]
    fn sync_resumes_normal_after_restored() {
        let mut s = OfdmFrameSync::new(L);
        s.advance(2); // Process
        s.advance(99); // SyncLost
        s.advance(2); // SyncRestored — expected_block is now 3
                      // Subsequent blocks must behave normally again.
        assert_eq!(s.advance(3), SyncAction::Process);
        assert_eq!(s.advance(4), SyncAction::Process);
    }

    #[test]
    fn sync_only_one_sync_lost_per_loss_event() {
        // Push a long run of wrong blocks and verify SyncLost appears exactly
        // once, then every subsequent mismatch is Discard.
        let mut s = OfdmFrameSync::new(L);
        s.advance(2); // Process
        let first = s.advance(99); // SyncLost
        assert_eq!(first, SyncAction::SyncLost);

        let mut lost_count = 0u32;
        for blkno in [10, 20, 30, 40, 50, 60, 70] {
            match s.advance(blkno) {
                SyncAction::SyncLost => lost_count += 1,
                SyncAction::Discard => {}
                other => panic!("unexpected action {other:?}"),
            }
        }
        assert_eq!(lost_count, 0, "SyncLost must fire only once per loss event");
    }

    #[test]
    fn sync_counter_wraps_at_l() {
        let mut s = OfdmFrameSync::new(L);
        // Advance to the last block.
        for blkno in 2..=L {
            assert_eq!(s.advance(blkno), SyncAction::Process);
        }
        // After wrapping, expected_block is 2 again.
        assert_eq!(s.advance(2), SyncAction::Process);
        assert_eq!(s.advance(3), SyncAction::Process);
    }
}
