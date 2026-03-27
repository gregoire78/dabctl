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

    /// Push current CIF-like payload and return interleaved payload once history is primed.
    pub fn push_and_interleave(&mut self, payload: &[u8]) -> Option<Vec<u8>> {
        self.history[self.write_index].clear();
        self.history[self.write_index].extend_from_slice(payload);

        if self.filled < 16 {
            self.filled += 1;
        }

        let interleave_map: [usize; 16] = [0, 8, 4, 12, 2, 10, 6, 14, 1, 9, 5, 13, 3, 11, 7, 15];
        let out = if self.filled == 16 {
            let len = payload.len();
            let mut tmp = vec![0u8; len];
            for i in 0..len {
                let lane = i & 0x0F;
                let src = interleave_map[lane];
                let src_idx = (self.write_index + src) & 0x0F;
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
    fn new() {
        let _ci = CifInterleaver::new();
    }

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
        for i in 0..16 {
            let result = ci.push_and_interleave(&[i as u8; 8]);
            if i < 15 {
                assert!(result.is_none());
            } else {
                assert!(result.is_some());
                assert_eq!(result.unwrap().len(), 8);
            }
        }
    }

    #[test]
    fn output_length_matches_input() {
        let mut ci = CifInterleaver::new();
        for i in 0..20 {
            let payload = vec![i as u8; 32];
            if let Some(out) = ci.push_and_interleave(&payload) {
                assert_eq!(out.len(), 32);
            }
        }
    }
}
