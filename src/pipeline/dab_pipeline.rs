// DAB Pipeline — adapted from eti-generator.cpp (eti-cmdline)
//
// `DabPipeline` drives the OFDM → FIC/CIF processing chain and emits
// `DabFrame` values via a bounded mpsc channel.
//
// Key design decisions vs. the previous Rust implementation:
// - The hand-rolled lock-free `SpscRing` is replaced by a standard
//   `mpsc::sync_channel`, which is safe, simpler, and equally efficient.
// - Subchannel deconvolution is sequential (matching the C++ reference
//   which compiles with `__PARALLEL__ 0`).
// - `Vec<SubchannelFrame>` replaces `SmallVec` — no external dependency.

use tracing::{trace, warn};

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, TrySendError};
use std::sync::Arc;
use std::thread;

use crate::pipeline::dab_frame::{DabFrame, SubchannelFrame};
use crate::pipeline::dab_params::DabParams;
use crate::pipeline::fic_handler::FicHandler;
use crate::pipeline::protection::{EepProtection, Protection, UepProtection};

/// Callback invoked with ensemble name and EId when FIC is decoded.
type EnsembleCb = Option<Arc<dyn Fn(&str, u32) + Send + Sync>>;
/// Callback invoked with programme name and SId when FIC is decoded.
type ProgramCb = Option<Arc<dyn Fn(&str, i32) + Send + Sync>>;
/// Callback invoked with `(success, total)` FIB CRC counts after each FIC frame.
///
/// The caller is responsible for accumulating these values over a reporting
/// window and computing the quality ratio from the summed counts.
type FicQualityCb = Option<Arc<dyn Fn(i16, i16) + Send + Sync>>;

/// Capacity of the OFDM block channel (OFDM thread → pipeline thread).
/// 512 slots ≈ 6–7 DAB frames of back-pressure (Mode I: 76 blocks/frame).
const RING_CAPACITY: usize = 512;

/// Bytes per capacity unit: 64 bits × 4 carriers = 64 samples.
/// ETSI EN 300 401 §7 — one CU = 64 bits.
const CU_SIZE: usize = 4 * 16;

// ─────────────────────────────────────────────────────────────────────────────
// OFDM frame sync state machine
// ─────────────────────────────────────────────────────────────────────────────

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

// ─────────────────────────────────────────────────────────────────────────────
// DabPipeline
// ─────────────────────────────────────────────────────────────────────────────

/// OFDM → FIC + CIF processing pipeline.
///
/// Receives OFDM soft-bits, performs FIC Viterbi decoding (ETSI EN 300 401 §11),
/// CIF time de-interleaving (§12.3) and EEP/UEP protection (§14.6), then emits
/// one `DabFrame` per completed CIF via a bounded mpsc channel.
pub struct DabPipeline {
    /// Producer end of the OFDM block channel.
    /// Dropping it (via `reset` or `drop`) signals the background thread to exit.
    block_tx: Option<mpsc::SyncSender<(i16, Vec<i16>)>>,
    thread_handle: Option<thread::JoinHandle<()>>,
    /// Shared with the background thread; set by `start_processing()`.
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
        let processing = Arc::new(AtomicBool::new(false));
        let (tx, rx) = mpsc::sync_channel(RING_CAPACITY);

        let p = processing.clone();
        let ecb = ensemble_cb.clone();
        let pcb = program_cb.clone();
        let fqcb = fic_quality_cb.clone();
        let thread_handle = thread::spawn(move || {
            Self::run_loop(rx, p, params, sender, ecb, pcb, fqcb);
        });

        DabPipeline {
            block_tx: Some(tx),
            thread_handle: Some(thread_handle),
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

    /// Push one OFDM soft-bit block into the pipeline.
    ///
    /// Non-blocking: if the channel is full the block is dropped and a WARN
    /// is logged. Under normal operation at DAB Mode I the consumer outpaces
    /// the producer, so this path is never exercised.
    pub fn process_block(&self, softbits: &[i16], blkno: i16) {
        let copy_len = softbits.len().min(self.bits_per_block);
        let data = softbits[..copy_len].to_vec();
        if let Some(ref tx) = self.block_tx {
            if let Err(TrySendError::Full(_)) = tx.try_send((blkno, data)) {
                warn!(blkno, "OFDM ring buffer full, dropping block");
            }
            // Err(TrySendError::Disconnected) means the thread already exited.
            // This can only happen during shutdown; no action needed.
        }
    }

    /// Signal the background thread that ensemble detection is complete and
    /// subchannel decoding should begin.
    pub fn start_processing(&self) {
        self.processing.store(true, Ordering::Release);
    }

    /// Return a shared handle to the `processing` flag.
    ///
    /// Callers that need to set the flag from a different thread (e.g. the OFDM
    /// synchroniser) can store this `Arc` and call `store(true, …)` directly,
    /// bypassing the normal `start_processing()` call.
    pub fn processing_flag(&self) -> Arc<AtomicBool> {
        self.processing.clone()
    }

    /// Tear down the current pipeline thread and start a fresh one.
    ///
    /// Called when the user tunes to a new channel so all accumulated state
    /// (FIC, CIF interleaver history, protection tables) is discarded.
    pub fn reset(&mut self, sender: mpsc::SyncSender<DabFrame>) {
        // Dropping block_tx signals the background thread to exit cleanly.
        drop(self.block_tx.take());
        if let Some(handle) = self.thread_handle.take() {
            let _ = handle.join();
        }
        self.processing.store(false, Ordering::Release);

        let params = DabParams::new(self.dab_mode);
        let (tx, rx) = mpsc::sync_channel(RING_CAPACITY);
        self.block_tx = Some(tx);

        let p = self.processing.clone();
        let ecb = self.ensemble_cb.clone();
        let pcb = self.program_cb.clone();
        let fqcb = self.fic_quality_cb.clone();
        self.thread_handle = Some(thread::spawn(move || {
            Self::run_loop(rx, p, params, sender, ecb, pcb, fqcb);
        }));
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Background thread
    // ─────────────────────────────────────────────────────────────────────────

    #[allow(clippy::too_many_arguments)]
    fn run_loop(
        rx: mpsc::Receiver<(i16, Vec<i16>)>,
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
        // Time de-interleaving delay table — ETSI EN 300 401 §12.3, Table 22
        let interleave_map: [usize; 16] = [0, 8, 4, 12, 2, 10, 6, 14, 1, 9, 5, 13, 3, 11, 7, 15];

        let cif_buf_size = number_of_blocks_per_cif * bits_per_block;
        let mut cif_in = vec![0i16; cif_buf_size];
        // Circular history for time de-interleaving: 16 successive CIF snapshots.
        let mut cif_vector = vec![vec![0i16; cif_buf_size]; 16];
        // Circular FIB history: 4 FIBs packed into 96 bytes per slot.
        let mut fib_vector = vec![[0u8; 96]; 16];
        let mut fib_input = vec![0i16; 3 * bits_per_block];

        let mut prot_table: Vec<Option<Protection>> = (0..64).map(|_| None).collect();
        let mut descrambler: Vec<Option<Vec<u8>>> = (0..64).map(|_| None).collect();

        let mut index_out: usize = 0;
        let mut frame_sync = OfdmFrameSync::new(params.get_l() as i16);
        // Number of CIFs accumulated so far; output is withheld until 15 full
        // cycles have passed so the interleaver history is valid.
        let mut amount: usize = 0;
        // Offset applied to the FIC-decoded CIF counter per emitted frame.
        let mut minor: u32 = 0;
        let mut cif_count_hi: i16 = -1;
        let mut cif_count_lo: i16 = -1;
        // De-interleaved output buffer reused each CIF to avoid per-frame allocation.
        let mut temp = vec![0i16; cif_buf_size];
        // Set when OfdmFrameSync reports SyncLost; propagated to the next DabFrame
        // so the audio thread can reset its superframe accumulator.
        let mut sync_just_lost = false;

        let mut my_fic_handler = FicHandler::new(&params);
        my_fic_handler.fib_processor.ensemble_name_cb = ensemble_cb;
        my_fic_handler.fib_processor.program_name_cb = program_cb;

        // Scratch buffer for raw FIB bits (4 FIBs × 768 soft-bits each).
        let mut fibs_bytes = vec![0u8; 4 * 768];

        while let Ok((blkno, bdata)) = rx.recv() {
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
                    sync_just_lost = true;
                    continue;
                }
                SyncAction::Discard => continue,
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

                    // Pack FIB soft-bits into packed bytes and store in the
                    // circular history buffer — one slot per FIB.
                    for i in 0..4 {
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
                        let (success, total) = my_fic_handler.get_fic_counts();
                        trace!(fic_ok = success, fic_total = total, "FIC frame decoded");
                        cb(success, total);
                    }
                }
                continue;
            }

            // MSC blocks — ETSI EN 300 401 §14.6
            let cif_index = ((blkno - 5) as usize) % number_of_blocks_per_cif;
            let offset = cif_index * bits_per_block;
            let copy_len = bits_per_block.min(bdata.len());
            cif_in[offset..offset + copy_len].copy_from_slice(&bdata[..copy_len]);

            // Emit one DabFrame when the last block of a CIF is received.
            if cif_index == number_of_blocks_per_cif - 1 {
                // Time de-interleaving — ETSI EN 300 401 §12.3.
                //
                // Write the current CIF slot FIRST so that D=0 positions
                // (i % 16 == 0) read back the just-written current-frame data,
                // not the 16-frame-old value that occupied the same circular
                // slot. This matches the reference C++ order in RunProcessor.cpp.
                #[allow(clippy::manual_memcpy)]
                for i in 0..(3072 * 18) {
                    let idx = interleave_map[i & 0x0F];
                    cif_vector[index_out & 0x0F][i] = cif_in[i];
                    temp[i] = cif_vector[(index_out + idx) & 0x0F][i];
                }

                // Withhold output until the 16-slot interleaver history is full.
                if amount < 15 {
                    amount += 1;
                    index_out = (index_out + 1) & 0x0F;
                    minor = 0;
                    continue;
                }

                // No valid FIC counter yet — cannot compute ETI CIF counter.
                if cif_count_hi < 0 || cif_count_lo < 0 {
                    continue;
                }

                // Adjusted CIF counter — ETSI EN 300 401 §14.1
                // CIFCountHigh ∈ [0, 19], CIFCountLow ∈ [0, 249]; both wrap modulo.
                let (adj_hi, adj_lo) = adjust_cif_counter(cif_count_hi, cif_count_lo, minor);
                let mut frame = DabFrame::new(fib_vector[index_out], adj_hi, adj_lo);

                // Propagate OFDM sync-loss to the audio thread so it can reset
                // SuperframeFilter before accumulating post-resync CIFs.
                if sync_just_lost {
                    frame.sync_lost = true;
                    sync_just_lost = false;
                }

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
        // Dropping block_tx signals the background thread to exit cleanly.
        drop(self.block_tx.take());
        if let Some(handle) = self.thread_handle.take() {
            let _ = handle.join();
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// CIF processing helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Deconvolve, descramble and pack all active sub-channels from one CIF.
///
/// Returns one `SubchannelFrame` per active sub-channel, in sub-channel ID order.
///
/// ETSI EN 300 401 §11 — MSC sub-channel structure.
fn process_cif_to_frames(
    input: &[i16],
    fic_handler: &FicHandler,
    prot_table: &mut [Option<Protection>],
    descrambler: &mut [Option<Vec<u8>>],
) -> Vec<SubchannelFrame> {
    let mut frames = Vec::new();

    for i in 0..64 {
        let data = fic_handler.get_channel_info(i);
        if !data.in_use {
            continue;
        }

        // Lazily initialise protection and descrambler tables on first use.
        if prot_table[i].is_none() {
            let out_size = data.bitrate as usize * 24;

            prot_table[i] = Some(if data.uep_flag {
                Protection::Uep(UepProtection::new(data.bitrate, data.protlev))
            } else {
                Protection::Eep(EepProtection::new(data.bitrate, data.protlev))
            });

            // PRBS descrambler initialisation — ETSI EN 300 401 §11.1
            // Shift register initialised to all-ones; feedback taps at positions 8 and 4.
            let mut shift_register = [1u8; 9];
            let mut desc = vec![0u8; out_size];
            for d in desc.iter_mut() {
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
        let byte_size = out_size / 8;

        let mut bit_buf = vec![0u8; out_size];
        if let Some(ref mut p) = prot_table[i] {
            p.deconvolve(&input[start..start + size], &mut bit_buf);
        }
        if let Some(ref d) = descrambler[i] {
            for (b, x) in bit_buf.iter_mut().zip(d.iter()) {
                *b ^= x;
            }
        }

        let mut packed = vec![0u8; byte_size];
        pack_bits(&bit_buf, &mut packed);
        frames.push(SubchannelFrame {
            subchid: data.id as u8,
            data: Arc::from(packed.as_slice()),
        });
    }

    frames
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
    // Propagate all carries: minor can exceed 250 in pathological cases where
    // many MSC CIFs are processed before the next FIC frame.
    hi += lo / 250;
    lo %= 250;
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

    // ── adjust_cif_counter ───────────────────────────────────────────────────
    // ETSI EN 300 401 §14.1: CIFCountLow ∈ [0,249], CIFCountHigh ∈ [0,19].

    #[test]
    fn cif_counter_no_overflow() {
        assert_eq!(adjust_cif_counter(0, 0, 0), (0, 0));
        assert_eq!(adjust_cif_counter(5, 100, 10), (5, 110));
    }

    #[test]
    fn cif_counter_lo_overflow_increments_hi() {
        assert_eq!(adjust_cif_counter(0, 249, 1), (1, 0));
    }

    #[test]
    fn cif_counter_lo_overflow_large_minor() {
        assert_eq!(adjust_cif_counter(3, 0, 500), (5, 0));
        assert_eq!(adjust_cif_counter(3, 0, 501), (5, 1));
    }

    #[test]
    fn cif_counter_hi_wraps_at_20() {
        assert_eq!(adjust_cif_counter(19, 249, 1), (0, 0));
    }

    #[test]
    fn cif_counter_hi_wraps_correctly_across_20() {
        assert_eq!(adjust_cif_counter(19, 0, 0), (19, 0));
        assert_eq!(adjust_cif_counter(19, 249, 1), (0, 0));
    }

    #[test]
    fn cif_counter_max_values_stay_in_range() {
        let (hi, lo) = adjust_cif_counter(19, 249, 249);
        assert!(hi < 20, "hi={hi} out of range");
        assert!(lo < 250, "lo={lo} out of range");
    }

    // ── pack_bits edge cases ─────────────────────────────────────────────────

    #[test]
    fn pack_bits_partial_tail_is_ignored() {
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

    // ── OfdmFrameSync ────────────────────────────────────────────────────────

    /// L for DAB Mode I is 76 (blocks 2..=76).
    const L: i16 = 76;

    #[test]
    fn sync_normal_sequence_produces_process() {
        let mut s = OfdmFrameSync::new(L);
        for blkno in 2..=L {
            assert_eq!(s.advance(blkno), SyncAction::Process, "blkno={blkno}");
        }
        assert_eq!(s.advance(2), SyncAction::Process);
    }

    #[test]
    fn sync_first_mismatch_emits_sync_lost() {
        let mut s = OfdmFrameSync::new(L);
        assert_eq!(s.advance(2), SyncAction::Process);
        assert_eq!(s.advance(99), SyncAction::SyncLost);
    }

    #[test]
    fn sync_second_mismatch_while_resyncing_emits_discard() {
        let mut s = OfdmFrameSync::new(L);
        s.advance(2);
        s.advance(99); // SyncLost — now resyncing, expected_block reset to 2
        assert_eq!(s.advance(3), SyncAction::Discard);
        assert_eq!(s.advance(50), SyncAction::Discard);
    }

    #[test]
    fn sync_block_2_while_resyncing_emits_sync_restored() {
        let mut s = OfdmFrameSync::new(L);
        s.advance(2);
        s.advance(99); // SyncLost
        s.advance(5); // Discard
        assert_eq!(s.advance(2), SyncAction::SyncRestored);
    }

    #[test]
    fn sync_resumes_normal_after_restored() {
        let mut s = OfdmFrameSync::new(L);
        s.advance(2); // Process
        s.advance(99); // SyncLost
        s.advance(2); // SyncRestored — expected_block is now 3
        assert_eq!(s.advance(3), SyncAction::Process);
        assert_eq!(s.advance(4), SyncAction::Process);
    }

    #[test]
    fn sync_only_one_sync_lost_per_loss_event() {
        let mut s = OfdmFrameSync::new(L);
        s.advance(2);
        let first = s.advance(99);
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
        for blkno in 2..=L {
            assert_eq!(s.advance(blkno), SyncAction::Process);
        }
        assert_eq!(s.advance(2), SyncAction::Process);
        assert_eq!(s.advance(3), SyncAction::Process);
    }

    // ── Time de-interleaving (ETSI EN 300 401 §12.3) ────────────────────────
    //
    // The interleave_map table assigns a delay D to each of the 16 byte lanes.
    // The write-before-read order is critical: for D=0 (lane 0, every i % 16 == 0)
    // the output must contain the CURRENT frame's sample, not the value that
    // occupied the same circular slot 16 frames earlier.

    /// The delay for lane 0 must be 0 — ETSI EN 300 401 §12.3, Table 22.
    #[test]
    fn interleave_map_delay_zero_at_lane_zero() {
        let interleave_map: [usize; 16] = [0, 8, 4, 12, 2, 10, 6, 14, 1, 9, 5, 13, 3, 11, 7, 15];
        assert_eq!(
            interleave_map[0], 0,
            "lane 0 must have delay 0 (ETSI EN 300 401 §12.3)"
        );
    }

    /// The delay table must be a permutation of 0..=15 — ETSI EN 300 401 §12.3.
    #[test]
    fn interleave_map_is_permutation_of_0_to_15() {
        let interleave_map: [usize; 16] = [0, 8, 4, 12, 2, 10, 6, 14, 1, 9, 5, 13, 3, 11, 7, 15];
        let mut sorted = interleave_map;
        sorted.sort();
        assert_eq!(
            sorted.to_vec(),
            (0..16).collect::<Vec<usize>>(),
            "interleave_map must be a permutation of 0..=15"
        );
    }

    /// D=0 positions (i % 16 == 0): output must equal the current frame's value.
    /// This verifies the write-before-read order is correct.
    #[test]
    fn time_deinterleave_d0_returns_current_frame_data() {
        // ETSI EN 300 401 §12.3: delay D=0 → output sample = current input sample.
        let interleave_map: [usize; 16] = [0, 8, 4, 12, 2, 10, 6, 14, 1, 9, 5, 13, 3, 11, 7, 15];
        let n = 32usize; // two full 16-lane rows
        let mut cif_vector = vec![vec![0i16; n]; 16];
        let mut temp = vec![0i16; n];
        let index_out = 0usize;

        // Pre-fill slot 0 with a sentinel to prove it is overwritten before the read.
        cif_vector[0].fill(-1);

        let cif_in: Vec<i16> = (100..100 + n as i16).collect();

        // Correct order: write THEN read (fixes the bug).
        for i in 0..n {
            let idx = interleave_map[i & 0x0F];
            cif_vector[index_out & 0x0F][i] = cif_in[i]; // write first
            temp[i] = cif_vector[(index_out + idx) & 0x0F][i]; // read second
        }

        // All D=0 positions must contain the current-frame value.
        for i in (0..n).step_by(16) {
            assert_eq!(
                temp[i], cif_in[i],
                "D=0 at position {i}: expected {} (current frame), got {}",
                cif_in[i], temp[i]
            );
        }
    }

    /// Regression: the old read-before-write order returned stale data for D=0.
    #[test]
    fn time_deinterleave_buggy_read_before_write_returns_stale_data() {
        let interleave_map: [usize; 16] = [0, 8, 4, 12, 2, 10, 6, 14, 1, 9, 5, 13, 3, 11, 7, 15];
        let n = 16usize;
        let mut cif_vector = vec![vec![0i16; n]; 16];
        let mut temp_buggy = vec![0i16; n];
        let index_out = 0usize;

        // Pre-fill slot 0 with a sentinel (simulates stale data from 16 frames ago).
        cif_vector[0].fill(-1);

        let cif_in: Vec<i16> = (100..100 + n as i16).collect();

        // Buggy (old) order: read BEFORE write.
        for i in 0..n {
            let idx = interleave_map[i & 0x0F];
            temp_buggy[i] = cif_vector[(index_out + idx) & 0x0F][i]; // reads stale -1
            cif_vector[index_out & 0x0F][i] = cif_in[i];
        }

        // Old code at D=0 (i=0): reads the stale sentinel, not the current frame.
        assert_ne!(
            temp_buggy[0], cif_in[0],
            "buggy order must NOT return current-frame value at D=0"
        );
        assert_eq!(
            temp_buggy[0], -1,
            "buggy order must return the stale sentinel at D=0"
        );
    }

    /// Non-zero delays must read from historical frames, not the current one.
    /// Verifies that the circular history buffer is used correctly.
    #[test]
    fn time_deinterleave_non_zero_delay_reads_history() {
        // For lane 1: delay D=8 (interleave_map[1] = 8).
        // After 9 frames the D=8 output at lane 1 must equal the value written
        // to that position 8 frames ago.
        let interleave_map: [usize; 16] = [0, 8, 4, 12, 2, 10, 6, 14, 1, 9, 5, 13, 3, 11, 7, 15];
        let n = 16usize;
        let mut cif_vector = vec![vec![0i16; n]; 16];
        let mut temp = vec![0i16; n];

        // Feed 9 frames; each frame writes a distinctive value (frame_no × 10).
        for frame in 0usize..9 {
            let index_out = frame & 0x0F;
            let frame_value = frame as i16 * 10;
            let cif_in = vec![frame_value; n];

            for i in 0..n {
                let idx = interleave_map[i & 0x0F];
                cif_vector[index_out][i] = cif_in[i];
                temp[i] = cif_vector[(index_out + idx) & 0x0F][i];
            }
        }

        // At frame 8 (index_out=8): i=1, idx=8 → cif_vector[(8+8)&0xF][1] = cif_vector[0][1].
        // Frame 0 wrote value 0, so temp[1] must be 0.
        assert_eq!(
            temp[1], 0,
            "D=8 after 9 frames: expected frame-0 value (0), got {}",
            temp[1]
        );
    }

    /// The maximum delay in the table is 15: the last position that becomes
    /// valid is filled after 16 frames of history.
    #[test]
    fn time_deinterleave_max_delay_15_reads_oldest_frame() {
        // Lane 15: delay D=15 (interleave_map[15] = 15).
        // interleave_map: [0,8,4,12,2,10,6,14,1,9,5,13,3,11,7,15]
        //                   0 1 2  3 4  5 6  7 8 9 ....            15
        let interleave_map: [usize; 16] = [0, 8, 4, 12, 2, 10, 6, 14, 1, 9, 5, 13, 3, 11, 7, 15];
        assert_eq!(interleave_map[15], 15, "sanity: lane 15 must have delay 15");

        let n = 16usize;
        let mut cif_vector = vec![vec![0i16; n]; 16];
        let mut temp = vec![0i16; n];

        // Feed 16 frames.
        for frame in 0usize..16 {
            let index_out = frame & 0x0F;
            let frame_value = frame as i16 * 10;
            let cif_in = vec![frame_value; n];
            for i in 0..n {
                let idx = interleave_map[i & 0x0F];
                cif_vector[index_out][i] = cif_in[i];
                temp[i] = cif_vector[(index_out + idx) & 0x0F][i];
            }
        }

        // At frame 15 (index_out=15): i=15, idx=15 → cif_vector[(15+15)&0xF][15]
        // = cif_vector[14][15]. Frame 14 wrote value 140.
        assert_eq!(
            temp[15], 140,
            "D=15 after 16 frames: expected 140 (frame 14 value), got {}",
            temp[15]
        );
    }
}
