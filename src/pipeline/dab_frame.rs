/// In-memory DAB frame: one multiplex frame as produced by `DabPipeline`.
///
/// This type carries the logical content of one DAB multiplex frame as defined in
/// ETSI EN 300 401 §3.2, with no serialisation overhead.
///
/// # Layout (ETSI EN 300 401 §3.2)
/// - FIC data   : 3 FIBs × 32 bytes = 96 bytes, Mode I  (§3.2.1)
/// - CIF counter: two-part counter hi/lo                             (§14.1)
/// - Subchannels: one `SubchannelFrame` per active sub-channel        (§7)
use std::sync::Arc;

use smallvec::SmallVec;

/// Maximum active sub-channels in a typical DAB multiplex.
/// Using inline storage up to 8 avoids heap allocation for most real-world ensembles.
const INLINE_SUBCH: usize = 8;

/// In-memory representation of one DAB frame post-Viterbi post-protection.
///
/// Sent from `DabPipeline` (OFDM thread) to the audio thread via a bounded channel.
#[derive(Debug)]
pub struct DabFrame {
    /// Three Fast Information Blocks (FIBs), each 32 bytes, Mode I.
    /// ETSI EN 300 401 §3.2.1 — packed bytes ready for `FicDecoder::process()`.
    pub fic_data: Box<[u8; 96]>,

    /// High part of the CIF counter (0..=19).  ETSI EN 300 401 §14.1.
    pub cif_count_hi: u8,

    /// Low part of the CIF counter (0..=249).  ETSI EN 300 401 §14.1.
    pub cif_count_lo: u8,

    /// Active sub-channels in this frame.
    /// Typical ensembles carry 6–12 sub-channels; inline storage avoids heap allocation.
    pub subchannels: SmallVec<[SubchannelFrame; INLINE_SUBCH]>,

    /// Set to `true` when the OFDM frame sequencer detected a block sequence
    /// discontinuity (`SyncLost`) immediately before this frame was assembled.
    ///
    /// The audio thread should call `SuperframeFilter::reset()` when this flag
    /// is set so the 5-CIF rolling window starts fresh from post-resync data.
    /// Without a reset, the window would mix pre-dropout CIFs with new ones,
    /// causing up to 5 consecutive Fire-code failures before re-alignment.
    /// (ETSI TS 102 563 §5 — DAB+ superframe structure)
    pub sync_lost: bool,
}

impl DabFrame {
    /// Create a new frame with the given FIC bytes and CIF counter.
    /// Sub-channels are pushed afterwards with `push_subchannel`.
    pub fn new(fic_data: [u8; 96], cif_count_hi: u8, cif_count_lo: u8) -> Self {
        DabFrame {
            fic_data: Box::new(fic_data),
            cif_count_hi,
            cif_count_lo,
            subchannels: SmallVec::new(),
            sync_lost: false,
        }
    }

    /// Append a sub-channel payload to this frame.
    pub fn push_subchannel(&mut self, subchid: u8, data: Arc<[u8]>) {
        self.subchannels.push(SubchannelFrame { subchid, data });
    }

    /// Return the payload for the given sub-channel ID, if present in this frame.
    pub fn subchannel_data(&self, subchid: u8) -> Option<&Arc<[u8]>> {
        self.subchannels
            .iter()
            .find(|s| s.subchid == subchid)
            .map(|s| &s.data)
    }
}

/// Payload for one active DAB sub-channel, post-deconvolution post-descramble.
///
/// `data` is reference-counted (`Arc<[u8]>`) so the audio thread and the PAD decoder
/// can each hold a reference without copying the bytes.
#[derive(Debug, Clone)]
pub struct SubchannelFrame {
    /// Sub-channel ID 0..63.  ETSI EN 300 401 §7.
    pub subchid: u8,

    /// Audio payload bytes, ready for `SuperframeFilter` (DAB+).
    pub data: Arc<[u8]>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    // ── construction ──────────────────────────────────────────────────────────

    #[test]
    fn new_frame_has_correct_fic_length() {
        // ETSI EN 300 401 §3.2.1: FIC = 3 FIBs × 32 bytes = 96 bytes (Mode I)
        let frame = DabFrame::new([0xAB; 96], 0, 0);
        assert_eq!(frame.fic_data.len(), 96);
        assert!(frame.fic_data.iter().all(|&b| b == 0xAB));
    }

    #[test]
    fn cif_counters_stored_correctly() {
        let frame = DabFrame::new([0u8; 96], 3, 117);
        assert_eq!(frame.cif_count_hi, 3);
        assert_eq!(frame.cif_count_lo, 117);
    }

    #[test]
    fn push_and_lookup_subchannel() {
        let payload: Arc<[u8]> = Arc::from(vec![1u8, 2, 3, 4].as_slice());
        let mut frame = DabFrame::new([0u8; 96], 0, 0);
        frame.push_subchannel(5, payload.clone());

        let found = frame.subchannel_data(5).expect("subchid 5 must be present");
        assert_eq!(found.as_ref(), &[1u8, 2, 3, 4]);
        assert!(frame.subchannel_data(6).is_none());
    }

    #[test]
    fn subchid_range_none_for_unknown() {
        // ETSI EN 300 401 §7: sub-channel IDs are 0..63
        let frame = DabFrame::new([0u8; 96], 0, 0);
        for id in [0u8, 63, 64, 255] {
            assert!(frame.subchannel_data(id).is_none());
        }
    }

    // ── SmallVec inline storage ───────────────────────────────────────────────

    #[test]
    fn small_vec_stays_on_stack_for_eight_subchannels() {
        let mut frame = DabFrame::new([0u8; 96], 0, 0);
        for id in 0u8..8 {
            let data: Arc<[u8]> = Arc::from(vec![id; 4].as_slice());
            frame.push_subchannel(id, data);
        }
        assert!(
            !frame.subchannels.spilled(),
            "SmallVec must not spill for 8 sub-channels"
        );
        assert_eq!(frame.subchannels.len(), 8);
    }

    #[test]
    fn arc_data_zero_copy_across_clones() {
        let payload: Arc<[u8]> = Arc::from(vec![0xFFu8; 576].as_slice());
        let mut frame = DabFrame::new([0u8; 96], 0, 0);
        frame.push_subchannel(0, payload.clone());

        // The two Arc instances point to the same allocation
        let retrieved = frame.subchannel_data(0).unwrap();
        assert!(Arc::ptr_eq(retrieved, &payload));
    }

    // ── channel transport ─────────────────────────────────────────────────────

    #[test]
    fn frame_sent_over_mpsc_channel() {
        let payload: Arc<[u8]> = Arc::from(vec![42u8; 192].as_slice());
        let (tx, rx) = mpsc::sync_channel::<DabFrame>(4);

        let mut frame = DabFrame::new([0xCD; 96], 1, 42);
        frame.push_subchannel(7, payload.clone());
        tx.send(frame).unwrap();

        let received = rx.recv().unwrap();
        assert_eq!(received.cif_count_hi, 1);
        assert_eq!(received.cif_count_lo, 42);
        assert_eq!(received.fic_data[0], 0xCD);

        let data = received.subchannel_data(7).unwrap();
        assert_eq!(data.len(), 192);
        assert!(data.iter().all(|&b| b == 42));
    }

    #[test]
    fn multiple_frames_sent_in_order() {
        let (tx, rx) = mpsc::sync_channel::<DabFrame>(4);
        for i in 0u8..4 {
            tx.send(DabFrame::new([i; 96], 0, i)).unwrap();
        }
        for i in 0u8..4 {
            let f = rx.recv().unwrap();
            assert_eq!(f.cif_count_lo, i);
            assert!(f.fic_data.iter().all(|&b| b == i));
        }
    }
}
