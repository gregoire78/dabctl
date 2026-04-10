/// Time-domain interleaving delays for each byte lane (lane = byte_index mod 16).
///
/// Each value is the number of frames the byte in that lane must be delayed.
/// ETSI EN 300 401 §12.3, Table 21.
const INTERLEAVE_DELAYS: [usize; 16] = [0, 8, 4, 12, 2, 10, 6, 14, 1, 9, 5, 13, 3, 11, 7, 15];

pub struct CifInterleaver {
    history: [Vec<u8>; 16],
    write_index: usize,
    filled: usize,
}

impl Default for CifInterleaver {
    fn default() -> Self {
        Self::new()
    }
}

impl CifInterleaver {
    pub fn new() -> Self {
        Self {
            history: std::array::from_fn(|_| Vec::new()),
            write_index: 0,
            filled: 0,
        }
    }

    /// Push the current CIF payload and return the interleaved output once the
    /// history is fully primed (first 16 calls return `None`).
    ///
    /// Each byte at position `i` in the output is taken from the frame that was
    /// pushed `INTERLEAVE_DELAYS[i & 0x0F]` calls ago (ETSI EN 300 401 §12.3).
    pub fn push_and_interleave(&mut self, payload: &[u8]) -> Option<Vec<u8>> {
        self.history[self.write_index].clear();
        self.history[self.write_index].extend_from_slice(payload);

        if self.filled < 16 {
            self.filled += 1;
        }

        let out = if self.filled == 16 {
            let len = payload.len();
            let mut tmp = vec![0u8; len];
            for i in 0..len {
                let lane = i & 0x0F;
                let delay = INTERLEAVE_DELAYS[lane];
                // history[write_index] is the most recent frame (delay 0).
                // To reach the frame `delay` steps back we subtract from write_index
                // in the circular buffer: (write_index + 16 - delay) & 0x0F.
                let src_idx = (self.write_index + 16 - delay) & 0x0F;
                let src_vec = &self.history[src_idx];
                if i < src_vec.len() {
                    tmp[i] = src_vec[i];
                }
            }
            Some(tmp)
        } else {
            None
        };

        self.write_index = (self.write_index + 1) & 0x0F;
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn returns_none_before_primed() {
        let mut ci = CifInterleaver::new();
        for _ in 0..15 {
            assert!(ci.push_and_interleave(&[1, 2, 3, 4]).is_none());
        }
    }

    #[test]
    fn returns_some_after_primed() {
        let mut ci = CifInterleaver::new();
        for i in 0..16u8 {
            let result = ci.push_and_interleave(&[i; 8]);
            if i < 15 {
                assert!(result.is_none());
            } else {
                let out = result.expect("must produce output on 16th push");
                assert_eq!(out.len(), 8);
            }
        }
    }

    #[test]
    fn output_length_matches_input() {
        let mut ci = CifInterleaver::new();
        for i in 0..20u8 {
            let payload = vec![i; 32];
            if let Some(out) = ci.push_and_interleave(&payload) {
                assert_eq!(out.len(), 32);
            }
        }
    }

    /// Verify that byte lane `l` in the output carries the value pushed
    /// `INTERLEAVE_DELAYS[l]` frames ago, not `l` frames ago (regression
    /// against the wrong `(write_index + delay)` formula).
    #[test]
    fn correct_delay_per_lane() {
        let mut ci = CifInterleaver::new();
        // Push 20 frames of 16 bytes each where every byte equals the frame number.
        let mut results = Vec::new();
        for frame in 0u8..20 {
            let payload = [frame; 16];
            if let Some(out) = ci.push_and_interleave(&payload) {
                results.push((frame, out));
            }
        }
        // For each output frame, byte at lane `l` must equal the value pushed
        // INTERLEAVE_DELAYS[l] frames before the current frame.
        for (frame, out) in &results {
            for lane in 0..16usize {
                let delay = INTERLEAVE_DELAYS[lane] as u8;
                let expected = frame.saturating_sub(delay);
                assert_eq!(
                    out[lane], expected,
                    "lane {lane}: expected frame {expected}, got {}",
                    out[lane]
                );
            }
        }
    }
}
