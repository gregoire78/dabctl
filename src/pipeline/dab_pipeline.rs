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

use tracing::warn;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, TrySendError};
use std::sync::Arc;
use std::thread;

use crate::pipeline::dab_constants::ChannelData;
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

/// Mode I carries 18 MSC blocks per CIF — ETSI EN 300 401 §14.1.
const NUMBER_OF_BLOCKS_PER_CIF: usize = 18;

/// Time de-interleaving delay table — ETSI EN 300 401 §12.3, Table 22.
const INTERLEAVE_MAP: [usize; 16] = [0, 8, 4, 12, 2, 10, 6, 14, 1, 9, 5, 13, 3, 11, 7, 15];

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

struct PipelineThreadContext {
    processing: Arc<AtomicBool>,
    params: DabParams,
    sender: mpsc::SyncSender<DabFrame>,
    ensemble_cb: EnsembleCb,
    program_cb: ProgramCb,
    fic_quality_cb: FicQualityCb,
}

struct FicFrameAssembler {
    fib_input: Vec<i16>,
    fibs_bytes: Vec<u8>,
}

impl FicFrameAssembler {
    fn new(bits_per_block: usize) -> Self {
        Self {
            fib_input: vec![0i16; 3 * bits_per_block],
            fibs_bytes: vec![0u8; 4 * 768],
        }
    }

    fn store_block(&mut self, blkno: i16, bdata: &[i16], bits_per_block: usize) -> bool {
        let offset = (blkno - 2) as usize * bits_per_block;
        let copy_len = bits_per_block.min(bdata.len());
        self.fib_input[offset..offset + copy_len].copy_from_slice(&bdata[..copy_len]);
        blkno == 4
    }

    fn decode_slots(&mut self, fic_handler: &mut FicHandler) -> [[u8; 96]; 4] {
        let mut valid = [false; 4];
        self.fibs_bytes.fill(0);
        fic_handler.process_fic_block(&self.fib_input, &mut self.fibs_bytes, &mut valid);
        replicate_fic_across_cifs(&self.fibs_bytes)
    }

    #[cfg(test)]
    fn current_bits(&self) -> &[i16] {
        &self.fib_input
    }
}

struct CifAssembler {
    cif_in: Vec<i16>,
    cif_vector: Vec<Vec<i16>>,
    fib_vector: Vec<[u8; 96]>,
    temp: Vec<i16>,
    index_out: usize,
    amount: usize,
    minor: u32,
    cif_count_hi: i16,
    cif_count_lo: i16,
    sync_just_lost: bool,
}

impl CifAssembler {
    fn new(bits_per_block: usize) -> Self {
        let cif_buf_size = NUMBER_OF_BLOCKS_PER_CIF * bits_per_block;
        Self {
            cif_in: vec![0i16; cif_buf_size],
            cif_vector: vec![vec![0i16; cif_buf_size]; 16],
            fib_vector: vec![[0u8; 96]; 16],
            temp: vec![0i16; cif_buf_size],
            index_out: 0,
            amount: 0,
            minor: 0,
            cif_count_hi: -1,
            cif_count_lo: -1,
            sync_just_lost: false,
        }
    }

    fn store_msc_block(&mut self, blkno: i16, bdata: &[i16], bits_per_block: usize) -> bool {
        let cif_index = ((blkno - 5) as usize) % NUMBER_OF_BLOCKS_PER_CIF;
        let offset = cif_index * bits_per_block;
        let copy_len = bits_per_block.min(bdata.len());
        self.cif_in[offset..offset + copy_len].copy_from_slice(&bdata[..copy_len]);
        cif_index == NUMBER_OF_BLOCKS_PER_CIF - 1
    }

    fn record_fic_slots(&mut self, fic_slots: [[u8; 96]; 4]) {
        for (i, slot_data) in fic_slots.into_iter().enumerate() {
            self.fib_vector[(self.index_out + i) & 0x0F] = slot_data;
        }
    }

    fn update_cif_counter(&mut self, hi: i16, lo: i16) {
        self.cif_count_hi = hi;
        self.cif_count_lo = lo;
        self.minor = 0;
    }

    fn note_sync_loss(&mut self) {
        self.minor = 0;
        self.sync_just_lost = true;
    }

    #[cfg(test)]
    fn finish_cif(&mut self, current_cif: &[i16]) -> Option<DabFrame> {
        let copy_len = self.cif_in.len().min(current_cif.len());
        self.cif_in[..copy_len].copy_from_slice(&current_cif[..copy_len]);
        self.finish_loaded_cif()
    }

    fn finish_loaded_cif(&mut self) -> Option<DabFrame> {
        for i in 0..self.temp.len() {
            let idx = INTERLEAVE_MAP[i & 0x0F];
            self.cif_vector[self.index_out & 0x0F][i] = self.cif_in[i];
            self.temp[i] = self.cif_vector[(self.index_out + idx) & 0x0F][i];
        }

        if self.amount < 15 {
            self.amount += 1;
            self.index_out = (self.index_out + 1) & 0x0F;
            self.minor = 0;
            return None;
        }

        if self.cif_count_hi < 0 || self.cif_count_lo < 0 {
            return None;
        }

        let (adj_hi, adj_lo) = adjust_cif_counter(self.cif_count_hi, self.cif_count_lo, self.minor);
        let mut frame = DabFrame::new(self.fib_vector[self.index_out], adj_hi, adj_lo);
        if self.sync_just_lost {
            frame.sync_lost = true;
            self.sync_just_lost = false;
        }

        self.index_out = (self.index_out + 1) & 0x0F;
        self.minor += 1;
        Some(frame)
    }

    fn deinterleaved_bits(&self) -> &[i16] {
        &self.temp
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

        let context = PipelineThreadContext {
            processing: processing.clone(),
            params,
            sender,
            ensemble_cb: ensemble_cb.clone(),
            program_cb: program_cb.clone(),
            fic_quality_cb: fic_quality_cb.clone(),
        };
        let thread_handle = thread::spawn(move || {
            Self::run_loop(rx, context);
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

        let context = PipelineThreadContext {
            processing: self.processing.clone(),
            params,
            sender,
            ensemble_cb: self.ensemble_cb.clone(),
            program_cb: self.program_cb.clone(),
            fic_quality_cb: self.fic_quality_cb.clone(),
        };
        self.thread_handle = Some(thread::spawn(move || {
            Self::run_loop(rx, context);
        }));
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Background thread
    // ─────────────────────────────────────────────────────────────────────────

    fn handle_fic_block(
        blkno: i16,
        bdata: &[i16],
        bits_per_block: usize,
        fic_assembler: &mut FicFrameAssembler,
        cif_assembler: &mut CifAssembler,
        fic_handler: &mut FicHandler,
        fic_quality_cb: &FicQualityCb,
    ) {
        if blkno == 2 {
            fic_handler.reset_quality_counters();
        }

        if fic_assembler.store_block(blkno, bdata, bits_per_block) {
            cif_assembler.record_fic_slots(fic_assembler.decode_slots(fic_handler));

            let (hi, lo) = fic_handler.get_cif_count();
            cif_assembler.update_cif_counter(hi, lo);
            if let Some(ref cb) = fic_quality_cb {
                let (success, total) = fic_handler.get_fic_counts();
                cb(success, total);
            }
        }
    }

    fn emit_completed_cif(
        processing: &Arc<AtomicBool>,
        sender: &mpsc::SyncSender<DabFrame>,
        cif_assembler: &mut CifAssembler,
        fic_handler: &FicHandler,
        prot_table: &mut [Option<Protection>],
        descrambler: &mut [Option<Vec<u8>>],
    ) -> bool {
        let Some(mut frame) = cif_assembler.finish_loaded_cif() else {
            return false;
        };

        let is_processing = processing.load(Ordering::Acquire);
        if is_processing {
            frame.subchannels = process_cif_to_frames(
                cif_assembler.deinterleaved_bits(),
                fic_handler,
                prot_table,
                descrambler,
            );
        }

        send_frame_to_consumer(sender, is_processing, frame)
    }

    fn run_loop(rx: mpsc::Receiver<(i16, Vec<i16>)>, context: PipelineThreadContext) {
        let PipelineThreadContext {
            processing,
            params,
            sender,
            ensemble_cb,
            program_cb,
            fic_quality_cb,
        } = context;

        let bits_per_block = 2 * params.get_carriers();
        let mut fic_assembler = FicFrameAssembler::new(bits_per_block);
        let mut prot_table: Vec<Option<Protection>> = (0..64).map(|_| None).collect();
        let mut descrambler: Vec<Option<Vec<u8>>> = (0..64).map(|_| None).collect();
        let mut frame_sync = OfdmFrameSync::new(params.get_l() as i16);
        let mut cif_assembler = CifAssembler::new(bits_per_block);

        let mut my_fic_handler = FicHandler::new(&params);
        my_fic_handler.fib_processor.ensemble_name_cb = ensemble_cb;
        my_fic_handler.fib_processor.program_name_cb = program_cb;

        while let Ok((blkno, bdata)) = rx.recv() {
            match frame_sync.advance(blkno) {
                SyncAction::Process => {}
                SyncAction::SyncRestored => {
                    tracing::debug!(blkno, "OFDM frame sync restored");
                }
                SyncAction::SyncLost => {
                    // CIF interleaver history survives a single dropped block;
                    // only the emitted-frame offset and sync marker are reset.
                    warn!(blkno, "OFDM frame sync lost, resyncing");
                    cif_assembler.note_sync_loss();
                    continue;
                }
                SyncAction::Discard => continue,
            }

            if (2..=4).contains(&blkno) {
                Self::handle_fic_block(
                    blkno,
                    &bdata,
                    bits_per_block,
                    &mut fic_assembler,
                    &mut cif_assembler,
                    &mut my_fic_handler,
                    &fic_quality_cb,
                );
                continue;
            }

            if !cif_assembler.store_msc_block(blkno, &bdata, bits_per_block) {
                continue;
            }

            if Self::emit_completed_cif(
                &processing,
                &sender,
                &mut cif_assembler,
                &my_fic_handler,
                &mut prot_table,
                &mut descrambler,
            ) {
                return;
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

struct SubchannelLayout {
    start: usize,
    size: usize,
    out_size: usize,
    byte_size: usize,
}

fn subchannel_layout(data: &ChannelData) -> SubchannelLayout {
    let out_size = data.bitrate as usize * 24;
    SubchannelLayout {
        start: data.start_cu as usize * CU_SIZE,
        size: data.size as usize * CU_SIZE,
        out_size,
        byte_size: out_size / 8,
    }
}

fn build_msc_descrambler(out_size: usize) -> Vec<u8> {
    // PRBS descrambler initialisation — ETSI EN 300 401 §11.1.
    // Shift register initialised to all-ones; feedback taps at positions 8 and 4.
    let mut shift_register = [1u8; 9];
    let mut desc = vec![0u8; out_size];
    for d in &mut desc {
        let bit = shift_register[8] ^ shift_register[4];
        for k in (1..9).rev() {
            shift_register[k] = shift_register[k - 1];
        }
        shift_register[0] = bit;
        *d = bit;
    }
    desc
}

fn build_protection(data: &ChannelData) -> Protection {
    if data.uep_flag {
        Protection::Uep(UepProtection::new(data.bitrate, data.protlev))
    } else {
        Protection::Eep(EepProtection::new(data.bitrate, data.protlev))
    }
}

fn ensure_channel_runtime(
    channel_index: usize,
    data: &ChannelData,
    prot_table: &mut [Option<Protection>],
    descrambler: &mut [Option<Vec<u8>>],
) {
    if prot_table[channel_index].is_none() {
        prot_table[channel_index] = Some(build_protection(data));
    }
    if descrambler[channel_index].is_none() {
        descrambler[channel_index] = Some(build_msc_descrambler(subchannel_layout(data).out_size));
    }
}

fn decode_subchannel_frame(
    input: &[i16],
    data: &ChannelData,
    protection: &mut Protection,
    descrambler: &[u8],
) -> SubchannelFrame {
    let layout = subchannel_layout(data);
    let end = layout.start + layout.size;
    let mut bit_buf = vec![0u8; layout.out_size];
    protection.deconvolve(&input[layout.start..end], &mut bit_buf);

    for (bit, prbs) in bit_buf.iter_mut().zip(descrambler.iter()) {
        *bit ^= prbs;
    }

    let mut packed = vec![0u8; layout.byte_size];
    pack_bits(&bit_buf, &mut packed);
    SubchannelFrame {
        subchid: data.id as u8,
        data: Arc::from(packed.as_slice()),
    }
}

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

    for channel_index in 0..64 {
        let data = fic_handler.get_channel_info(channel_index);
        if !data.in_use {
            continue;
        }

        ensure_channel_runtime(channel_index, &data, prot_table, descrambler);

        if let (Some(protection), Some(prbs)) = (
            prot_table[channel_index].as_mut(),
            descrambler[channel_index].as_deref(),
        ) {
            frames.push(decode_subchannel_frame(input, &data, protection, prbs));
        }
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

/// Pack the four decoded 768-bit FIC chunks into the four 96-byte CIF slots of
/// the current DAB frame.
fn replicate_fic_across_cifs(bits: &[u8]) -> [[u8; 96]; 4] {
    let mut slots = [[0u8; 96]; 4];
    for (slot_idx, slot) in slots.iter_mut().enumerate() {
        let start = slot_idx * 768;
        let end = ((slot_idx + 1) * 768).min(bits.len());
        if end > start {
            pack_bits(&bits[start..end], slot);
        }
    }
    slots
}

fn send_frame_to_consumer(
    sender: &mpsc::SyncSender<DabFrame>,
    is_processing: bool,
    frame: DabFrame,
) -> bool {
    if is_processing {
        sender.send(frame).is_err()
    } else {
        matches!(sender.try_send(frame), Err(TrySendError::Disconnected(_)))
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
    fn msc_descrambler_prefix_matches_reference() {
        let seq = build_msc_descrambler(16);
        assert_eq!(seq, vec![0, 0, 0, 0, 0, 1, 1, 1, 1, 0, 1, 1, 1, 1, 1, 0]);
    }

    #[test]
    fn subchannel_layout_uses_cu_and_bitrate_units() {
        let data = crate::pipeline::dab_constants::ChannelData {
            in_use: true,
            id: 7,
            start_cu: 3,
            uep_flag: false,
            protlev: 0,
            size: 8,
            bitrate: 32,
        };

        let layout = subchannel_layout(&data);
        assert_eq!(layout.start, 3 * CU_SIZE);
        assert_eq!(layout.size, 8 * CU_SIZE);
        assert_eq!(layout.out_size, 32 * 24);
        assert_eq!(layout.byte_size, 32 * 24 / 8);
    }

    #[test]
    fn pack_bits_empty_input_produces_no_output() {
        let bits: Vec<u8> = vec![];
        let mut out = [0u8; 0];
        pack_bits(&bits, &mut out); // must not panic
    }

    #[test]
    fn decoded_fic_four_chunks_are_preserved_per_cif_slot() {
        let mut bits = vec![0u8; 768 * 4];
        for (i, bit) in bits[0..768].iter_mut().enumerate() {
            *bit = (i % 2) as u8;
        }
        for (i, bit) in bits[768..1536].iter_mut().enumerate() {
            *bit = ((i / 2) % 2) as u8;
        }
        for bit in &mut bits[1536..2304] {
            *bit = 1;
        }
        // last quarter intentionally stays at 0

        let slots = replicate_fic_across_cifs(&bits);
        assert_ne!(slots[0], slots[1]);
        assert_ne!(slots[1], slots[2]);
        assert_ne!(slots[2], slots[3]);
    }

    #[test]
    fn process_cif_to_frames_ignores_unused_channels() {
        let params = DabParams::new(1);
        let fic_handler = FicHandler::new(&params);
        let mut prot_table: Vec<Option<Protection>> = (0..64).map(|_| None).collect();
        let mut descrambler: Vec<Option<Vec<u8>>> = (0..64).map(|_| None).collect();
        let frames = process_cif_to_frames(
            &vec![0i16; 18 * 2 * params.get_carriers()],
            &fic_handler,
            &mut prot_table,
            &mut descrambler,
        );
        assert!(frames.is_empty());
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

    #[test]
    fn cif_assembler_waits_for_interleaver_history_before_emitting() {
        let params = DabParams::new(1);
        let bits_per_block = 2 * params.get_carriers();
        let mut assembler = CifAssembler::new(bits_per_block);
        assembler.update_cif_counter(2, 17);

        for warmup_index in 0..15 {
            let frame = assembler.finish_cif(&vec![warmup_index; 18 * bits_per_block]);
            assert!(
                frame.is_none(),
                "warmup CIF {warmup_index} should not emit yet"
            );
        }

        let frame = assembler
            .finish_cif(&vec![123; 18 * bits_per_block])
            .expect("frame should emit after history is full");
        assert_eq!(frame.cif_count_hi, 2);
        assert_eq!(frame.cif_count_lo, 17);
    }

    #[test]
    fn cif_assembler_propagates_sync_loss_only_once() {
        let params = DabParams::new(1);
        let bits_per_block = 2 * params.get_carriers();
        let mut assembler = CifAssembler::new(bits_per_block);
        assembler.update_cif_counter(0, 0);

        for _ in 0..15 {
            let _ = assembler.finish_cif(&vec![0; 18 * bits_per_block]);
        }

        assembler.note_sync_loss();
        let first = assembler
            .finish_cif(&vec![1; 18 * bits_per_block])
            .expect("first post-loss CIF should emit");
        assert!(first.sync_lost);

        let second = assembler
            .finish_cif(&vec![2; 18 * bits_per_block])
            .expect("next CIF should also emit");
        assert!(!second.sync_lost);
    }

    #[test]
    fn fic_frame_assembler_marks_frame_complete_only_on_block_four() {
        let mut assembler = FicFrameAssembler::new(8);
        assert!(!assembler.store_block(2, &[1; 8], 8));
        assert!(!assembler.store_block(3, &[2; 8], 8));
        assert!(assembler.store_block(4, &[3; 8], 8));
    }

    #[test]
    fn fic_frame_assembler_places_blocks_in_order() {
        let mut assembler = FicFrameAssembler::new(4);
        let first = [1, 1, 1, 1];
        let second = [2, 2, 2, 2];
        let third = [3, 3, 3, 3];

        assembler.store_block(2, &first, 4);
        assembler.store_block(3, &second, 4);
        assembler.store_block(4, &third, 4);

        let combined = assembler.current_bits();
        assert_eq!(&combined[0..4], &first);
        assert_eq!(&combined[4..8], &second);
        assert_eq!(&combined[8..12], &third);
    }
}
