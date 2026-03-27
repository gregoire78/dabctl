// Frequency interleaver - converted from freq-interleaver.cpp (eti-cmdline)
// Copyright (C) 2013 Jan van Katwijk - Lazy Chair Computing
// Section 14.6 of the DAB standard

use crate::support::dab_params::DabParams;

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
    for i in 0..t_u {
        if tmp[i] == t_u as i16 / 2 {
            continue;
        }
        if tmp[i] < lwb || tmp[i] > upb {
            continue;
        }
        result.push(tmp[i] - t_u as i16 / 2);
    }
    result
}

impl FreqInterleaver {
    pub fn new(params: &DabParams) -> Self {
        let t_u = params.t_u as usize;
        let carriers = params.k as i16;

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
