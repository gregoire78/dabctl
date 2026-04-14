// COFDM time de-interleaver — ETSI EN 300 401 §12.3
//
// DAB uses a depth-16 time interleaver at the CIF level to spread burst errors
// (e.g. from fast-fading or impulse noise) across 16 consecutive CIFs (~384 ms
// in Mode I).  The OFDM receiver must reverse this interleaving before handing
// the soft bits to the Viterbi decoder.
//
// Algorithm (Table 22, ETSI EN 300 401 §12.3):
//   For each sample position i in the CIF:
//     slot_offset[i] = DELAY_TABLE[i mod 16]
//   output[i] = ring[(write_ptr + slot_offset[i]) mod 16][i]
//
// The circular ring is filled one slot per CIF.  Output is withheld for the
// first DEPTH cycles so that every delay tap has valid data.
//
// Note on delay semantics:
//   Adding offset D to the write pointer in a ring that fills forward means
//   reading the value written (DEPTH − D) CIFs ago (for D > 0), since slot
//   (wp + D) mod 16 was last written DEPTH − D pushes before the current one.
//   DELAY_TABLE[0] = 0 → reads the current CIF (0 CIFs ago).
//   DELAY_TABLE[1] = 8 → reads 16 − 8 = 8 CIFs ago.

/// Time-interleaving depth as specified by ETSI EN 300 401 §12.3.
const DEPTH: usize = 16;

/// Per-position slot offset — ETSI EN 300 401 §12.3, Table 22.
///
/// `slot_offset[i] = DELAY_TABLE[i % 16]`.
/// The read slot is `(write_ptr + slot_offset) % DEPTH`.
const DELAY_TABLE: [usize; DEPTH] = [0, 8, 4, 12, 2, 10, 6, 14, 1, 9, 5, 13, 3, 11, 7, 15];

/// COFDM CIF-level time de-interleaver (ETSI EN 300 401 §12.3).
///
/// # Usage
///
/// ```text
/// let mut deintlv = TimeDeInterleaver::new(cif_size);
/// let mut out = vec![0i16; cif_size];
/// loop {
///     fill(&mut cif_in);  // receive one CIF of soft bits
///     if deintlv.push_cif(&cif_in, &mut out) {
///         // `out` now contains the de-interleaved soft bits
///         viterbi.decode(&out);
///     }
/// }
/// ```
pub struct TimeDeInterleaver {
    /// Number of i16 soft-bit samples per CIF.
    cif_size: usize,
    /// Circular ring of `DEPTH` CIF snapshots.
    ring: Vec<Vec<i16>>,
    /// Next write slot in the ring (wraps modulo DEPTH).
    write_ptr: usize,
    /// CIFs pushed so far, capped at DEPTH once the ring is full.
    amount: usize,
}

impl TimeDeInterleaver {
    /// Create a new de-interleaver sized for `cif_size` soft-bit samples per CIF.
    pub fn new(cif_size: usize) -> Self {
        TimeDeInterleaver {
            cif_size,
            ring: vec![vec![0i16; cif_size]; DEPTH],
            write_ptr: 0,
            amount: 0,
        }
    }

    /// Push one CIF of soft bits and (once the ring is full) fill `out` with
    /// the de-interleaved result.
    ///
    /// Returns `true` when `out` contains valid de-interleaved data, `false`
    /// during the warm-up phase (first DEPTH − 1 pushes).
    ///
    /// `cif_in` and `out` must both be at least `cif_size` elements long.
    ///
    /// # Panics
    ///
    /// Panics in debug mode if `cif_in.len() < cif_size` or
    /// `out.len() < cif_size`.
    pub fn push_cif(&mut self, cif_in: &[i16], out: &mut [i16]) -> bool {
        debug_assert!(
            cif_in.len() >= self.cif_size,
            "cif_in too short: {} < {}",
            cif_in.len(),
            self.cif_size
        );
        debug_assert!(
            out.len() >= self.cif_size,
            "out too short: {} < {}",
            out.len(),
            self.cif_size
        );

        let wp = self.write_ptr;

        // Write the current CIF into the ring **before** reading delayed taps
        // so that positions with slot_offset=0 (DELAY_TABLE[i%16] == 0) yield
        // the just-written value, matching the reference implementation order.
        self.ring[wp].copy_from_slice(&cif_in[..self.cif_size]);

        // Assemble de-interleaved output from DEPTH different ring slots.
        for i in 0..self.cif_size {
            let offset = DELAY_TABLE[i & (DEPTH - 1)];
            let read_slot = (wp + offset) & (DEPTH - 1);
            out[i] = self.ring[read_slot][i];
        }

        // Advance write pointer (wraps modulo DEPTH).
        self.write_ptr = (wp + 1) & (DEPTH - 1);

        // Track the number of CIFs pushed, capped at DEPTH once the ring is
        // full.  Output is valid only when DEPTH CIFs have been seen.
        if self.amount < DEPTH {
            self.amount += 1;
        }
        self.amount >= DEPTH
    }

    /// Reset the de-interleaver to its initial state (zero ring, zero counters).
    ///
    /// Call this when the OFDM sync is lost and a fresh ensemble is being
    /// acquired, to avoid mixing stale data with the new stream.
    pub fn reset(&mut self) {
        for slot in &mut self.ring {
            slot.fill(0);
        }
        self.write_ptr = 0;
        self.amount = 0;
    }

    /// `true` once the warm-up phase is complete and `push_cif` returns valid
    /// de-interleaved data.
    pub fn is_ready(&self) -> bool {
        self.amount >= DEPTH
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── 1. Warm-up guard ────────────────────────────────────────────────────

    #[test]
    fn warmup_returns_false_for_first_15_cifs() {
        let size = 32;
        let mut d = TimeDeInterleaver::new(size);
        let cif = vec![0i16; size];
        let mut out = vec![0i16; size];
        for k in 0..15 {
            assert!(
                !d.push_cif(&cif, &mut out),
                "push_cif should return false during warm-up (CIF {})",
                k
            );
        }
        // 16th CIF should be the first valid output.
        assert!(d.push_cif(&cif, &mut out));
    }

    // ── 2. is_ready is false until after the 16th push ─────────────────────

    #[test]
    fn is_ready_after_warmup() {
        let size = 16;
        let mut d = TimeDeInterleaver::new(size);
        let cif = vec![0i16; size];
        let mut out = vec![0i16; size];
        assert!(!d.is_ready());
        for _ in 0..15 {
            d.push_cif(&cif, &mut out);
        }
        // 15 pushes: ring not full yet.
        assert!(!d.is_ready());
        // 16th push fills the ring.
        d.push_cif(&cif, &mut out);
        assert!(d.is_ready());
    }

    // ── 3. Delay-0 positions reproduce the just-written value ───────────────

    #[test]
    fn delay_zero_positions_read_current_cif() {
        // Positions where i % 16 == 0 have DELAY_TABLE[0] = 0, so the read
        // slot equals the write slot → always returns the current CIF value.
        let size = 32; // two full DELAY_TABLE periods
        let mut d = TimeDeInterleaver::new(size);
        let mut out = vec![0i16; size];

        // Warm-up with zeros.
        let zeros = vec![0i16; size];
        for _ in 0..15 {
            d.push_cif(&zeros, &mut out);
        }

        // Push a distinctive CIF with value 42 at all delay-0 positions.
        let mut cif = vec![0i16; size];
        for i in (0..size).step_by(DEPTH) {
            // i % 16 == 0 → DELAY_TABLE[0] = 0 → read current slot
            cif[i] = 42;
        }
        assert!(d.push_cif(&cif, &mut out));

        for i in (0..size).step_by(DEPTH) {
            assert_eq!(
                out[i], 42,
                "delay-0 position {}: expected 42, got {}",
                i, out[i]
            );
        }
    }

    // ── 4. Actual delay is (DEPTH − slot_offset) CIFs for offset > 0 ────────

    #[test]
    fn slot_offset_8_reads_value_from_8_cifs_ago() {
        // DELAY_TABLE[1] = 8. Positions where i % 16 == 1 read the slot
        // (write_ptr + 8) % 16. In a forward-filling ring this slot was written
        // DEPTH − 8 = 8 CIFs before the current push.
        let size = 32;
        let mut d = TimeDeInterleaver::new(size);
        let mut out = vec![0i16; size];
        let zeros = vec![0i16; size];

        // The 16th push is the first valid output (write_ptr = 15 at that point).
        // Slot (15 + 8) % 16 = 7 was written at push index 7 (0-based),
        // which is 8 pushes before push index 15.  Place the sentinel there.
        // Push 0..6: zeros (write_ptr 0..6)
        for _ in 0..7 {
            d.push_cif(&zeros, &mut out);
        }
        // Push 7: write 77 at positions i%16 == 1 (write_ptr = 7)
        let mut special = vec![0i16; size];
        for i in 0..size {
            if i % DEPTH == 1 {
                special[i] = 77;
            }
        }
        d.push_cif(&special, &mut out);

        // Push 8..14: zeros (write_ptr 8..14)
        for _ in 0..7 {
            d.push_cif(&zeros, &mut out);
        }

        // Push 15 (write_ptr=15): first valid output.
        assert!(d.push_cif(&zeros, &mut out));
        for i in 0..size {
            if i % DEPTH == 1 {
                assert_eq!(
                    out[i], 77,
                    "slot_offset=8 position {}: expected 77, got {}",
                    i, out[i]
                );
            }
        }
    }

    // ── 5. Permutation property: all 16 offsets produce distinct read slots ─

    #[test]
    fn all_sixteen_slot_offsets_map_to_distinct_ring_slots() {
        // Feed 16 CIFs k=0..15 each with all samples = k.
        // On the 16th push (write_ptr=15, first valid), verify:
        //   out[i] = ring[(15 + DELAY_TABLE[i%16]) % 16][i]
        //          = (15 + DELAY_TABLE[i%16]) % 16
        // because ring[j][i] == j for all j (CIF k was written to slot k).
        let size = DEPTH;
        let mut d = TimeDeInterleaver::new(size);
        let mut out = vec![0i16; size];

        for k in 0i16..DEPTH as i16 {
            let cif = vec![k; size];
            d.push_cif(&cif, &mut out);
        }
        // The 16th push (k=15) was the first valid output.
        for i in 0..size {
            let expected = ((15 + DELAY_TABLE[i % DEPTH]) % DEPTH) as i16;
            assert_eq!(
                out[i], expected,
                "position {}: expected {} got {}",
                i, expected, out[i]
            );
        }
        // Verify that the 16 read-slot values are all distinct (permutation).
        let slots: std::collections::HashSet<i16> = out.iter().copied().collect();
        assert_eq!(slots.len(), DEPTH, "read slots must be a permutation of 0..15");
    }

    // ── 6. reset() clears state ────────────────────────────────────────────

    #[test]
    fn reset_clears_ring_and_warmup() {
        let size = 16;
        let mut d = TimeDeInterleaver::new(size);
        let mut out = vec![0i16; size];
        let cif = vec![99i16; size];

        // Fully warm up.
        for _ in 0..16 {
            d.push_cif(&cif, &mut out);
        }
        assert!(d.is_ready());

        d.reset();
        assert!(!d.is_ready());

        // After reset: first 15 pushes must return false again.
        let zeros = vec![0i16; size];
        for k in 0..15 {
            assert!(
                !d.push_cif(&zeros, &mut out),
                "should be in warm-up after reset (CIF {})",
                k
            );
        }
        // 16th push returns true and ring is fresh (zeros).
        assert!(d.push_cif(&zeros, &mut out));
        assert!(out.iter().all(|&v| v == 0), "ring should be zeroed after reset");
    }

    // ── 7. Delay-0 stable across multiple frames ───────────────────────────

    #[test]
    fn delay_zero_stable_across_multiple_frames() {
        let size = 16;
        let mut d = TimeDeInterleaver::new(size);
        let mut out = vec![0i16; size];

        // Warm up.
        for _ in 0..15 {
            d.push_cif(&vec![0i16; size], &mut out);
        }

        for frame in 0i16..10 {
            let mut cif = vec![0i16; size];
            cif[0] = frame; // position 0: DELAY_TABLE[0] = 0 → reads current CIF
            d.push_cif(&cif, &mut out);
            assert_eq!(
                out[0], frame,
                "frame {}: delay-0 at pos 0 should be {}",
                frame, frame
            );
        }
    }

    // ── 8. Distinct CIF sequence: delay-0 always tracks current frame ───────

    #[test]
    fn distinct_cif_sequence_delay_zero_tracks_current() {
        let size = DEPTH;
        let mut d = TimeDeInterleaver::new(size);
        let mut out = vec![0i16; size];

        for k in 0i16..32 {
            let cif = vec![k; size];
            let valid = d.push_cif(&cif, &mut out);
            if valid {
                // Position 0 has DELAY_TABLE[0]=0: always reads the current CIF.
                assert_eq!(out[0], k, "pos 0 delay-0: expected {} got {}", k, out[0]);
            }
        }
    }
}
