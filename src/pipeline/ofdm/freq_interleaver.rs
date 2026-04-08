// Frequency interleaver - converted from freq-interleaver.cpp (eti-cmdline)
// Section 14.6 of the DAB standard

use crate::pipeline::dab_params::DabParams;

pub struct FreqInterleaver {
    perm_table: Vec<i16>,
}

fn create_mapper(t_u: usize, v1: i16, lwb: i16, upb: i16) -> Vec<i16> {
    let mut tmp = vec![0i16; t_u];
    tmp[0] = 0;
    for i in 1..t_u {
        tmp[i] = ((13i32 * tmp[i - 1] as i32 + v1 as i32) % t_u as i32) as i16;
    }

    let mut result = Vec::with_capacity(t_u);
    for &t in tmp.iter().take(t_u) {
        if t == t_u as i16 / 2 {
            continue;
        }
        if t < lwb || t > upb {
            continue;
        }
        result.push(t - t_u as i16 / 2);
    }
    result
}

impl FreqInterleaver {
    pub fn new(params: &DabParams) -> Self {
        let t_u = params.t_u as usize;
        let carriers = params.k;

        let perm_table = match params.dab_mode {
            1 => create_mapper(t_u, 511, 256, 256 + carriers),
            2 => create_mapper(t_u, 127, 64, 64 + carriers),
            3 => create_mapper(t_u, 63, 32, 32 + carriers),
            4 => create_mapper(t_u, 255, 128, 128 + carriers),
            _ => create_mapper(t_u, 511, 256, 256 + carriers),
        };

        FreqInterleaver { perm_table }
    }

    /// Map interleaved carrier index to actual carrier index
    /// Returns value in range -K/2 .. K/2 (excluding 0)
    #[inline]
    pub fn map_in(&self, n: usize) -> i16 {
        self.perm_table[n]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::dab_params::DabParams;

    #[test]
    fn mode1_size() {
        let params = DabParams::new(1);
        let fi = FreqInterleaver::new(&params);
        for i in 0..params.k as usize {
            let _mapped = fi.map_in(i);
        }
    }

    #[test]
    fn mode1_range() {
        let params = DabParams::new(1);
        let fi = FreqInterleaver::new(&params);
        let half_k = params.k / 2;
        for i in 0..params.k as usize {
            let m = fi.map_in(i);
            assert!(
                m >= -half_k && m <= half_k,
                "map_in({}) = {} out of range",
                i,
                m
            );
            assert_ne!(m, 0, "DC carrier should never appear");
        }
    }

    #[test]
    fn mode1_no_duplicates() {
        let params = DabParams::new(1);
        let fi = FreqInterleaver::new(&params);
        let k = params.k as usize;
        let mut seen = std::collections::HashSet::new();
        for i in 0..k {
            let m = fi.map_in(i);
            assert!(seen.insert(m), "Duplicate mapping at index {}: {}", i, m);
        }
        assert_eq!(seen.len(), k);
    }

    #[test]
    fn all_modes() {
        for mode in [1, 2, 3, 4] {
            let params = DabParams::new(mode);
            let fi = FreqInterleaver::new(&params);
            let mut set = std::collections::HashSet::new();
            for i in 0..params.k as usize {
                set.insert(fi.map_in(i));
            }
            assert_eq!(
                set.len(),
                params.k as usize,
                "Mode {} should have K unique mappings",
                mode
            );
        }
    }
}
