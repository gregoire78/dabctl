// Phase table - converted from phasetable.cpp (eti-cmdline)
// Copyright (C) 2013 Jan van Katwijk - Lazy Chair Computing

use std::f32::consts::PI;

struct PhaseTableElement {
    kmin: i32,
    kmax: i32,
    i: i32,
    n: i32,
}

static MODE_I_TABLE: &[PhaseTableElement] = &[
    PhaseTableElement { kmin: -768, kmax: -737, i: 0, n: 1 },
    PhaseTableElement { kmin: -736, kmax: -705, i: 1, n: 2 },
    PhaseTableElement { kmin: -704, kmax: -673, i: 2, n: 0 },
    PhaseTableElement { kmin: -672, kmax: -641, i: 3, n: 1 },
    PhaseTableElement { kmin: -640, kmax: -609, i: 0, n: 3 },
    PhaseTableElement { kmin: -608, kmax: -577, i: 1, n: 2 },
    PhaseTableElement { kmin: -576, kmax: -545, i: 2, n: 2 },
    PhaseTableElement { kmin: -544, kmax: -513, i: 3, n: 3 },
    PhaseTableElement { kmin: -512, kmax: -481, i: 0, n: 2 },
    PhaseTableElement { kmin: -480, kmax: -449, i: 1, n: 1 },
    PhaseTableElement { kmin: -448, kmax: -417, i: 2, n: 2 },
    PhaseTableElement { kmin: -416, kmax: -385, i: 3, n: 3 },
    PhaseTableElement { kmin: -384, kmax: -353, i: 0, n: 1 },
    PhaseTableElement { kmin: -352, kmax: -321, i: 1, n: 2 },
    PhaseTableElement { kmin: -320, kmax: -289, i: 2, n: 3 },
    PhaseTableElement { kmin: -288, kmax: -257, i: 3, n: 3 },
    PhaseTableElement { kmin: -256, kmax: -225, i: 0, n: 2 },
    PhaseTableElement { kmin: -224, kmax: -193, i: 1, n: 2 },
    PhaseTableElement { kmin: -192, kmax: -161, i: 2, n: 2 },
    PhaseTableElement { kmin: -160, kmax: -129, i: 3, n: 1 },
    PhaseTableElement { kmin: -128, kmax:  -97, i: 0, n: 1 },
    PhaseTableElement { kmin:  -96, kmax:  -65, i: 1, n: 3 },
    PhaseTableElement { kmin:  -64, kmax:  -33, i: 2, n: 1 },
    PhaseTableElement { kmin:  -32, kmax:   -1, i: 3, n: 2 },
    PhaseTableElement { kmin:    1, kmax:   32, i: 0, n: 3 },
    PhaseTableElement { kmin:   33, kmax:   64, i: 3, n: 1 },
    PhaseTableElement { kmin:   65, kmax:   96, i: 2, n: 1 },
    PhaseTableElement { kmin:   97, kmax:  128, i: 1, n: 1 }, // bug fix 2014-09-03
    PhaseTableElement { kmin:  129, kmax:  160, i: 0, n: 2 },
    PhaseTableElement { kmin:  161, kmax:  192, i: 3, n: 2 },
    PhaseTableElement { kmin:  193, kmax:  224, i: 2, n: 1 },
    PhaseTableElement { kmin:  225, kmax:  256, i: 1, n: 0 },
    PhaseTableElement { kmin:  257, kmax:  288, i: 0, n: 2 },
    PhaseTableElement { kmin:  289, kmax:  320, i: 3, n: 2 },
    PhaseTableElement { kmin:  321, kmax:  352, i: 2, n: 3 },
    PhaseTableElement { kmin:  353, kmax:  384, i: 1, n: 3 },
    PhaseTableElement { kmin:  385, kmax:  416, i: 0, n: 0 },
    PhaseTableElement { kmin:  417, kmax:  448, i: 3, n: 2 },
    PhaseTableElement { kmin:  449, kmax:  480, i: 2, n: 1 },
    PhaseTableElement { kmin:  481, kmax:  512, i: 1, n: 3 },
    PhaseTableElement { kmin:  513, kmax:  544, i: 0, n: 3 },
    PhaseTableElement { kmin:  545, kmax:  576, i: 3, n: 3 },
    PhaseTableElement { kmin:  577, kmax:  608, i: 2, n: 3 },
    PhaseTableElement { kmin:  609, kmax:  640, i: 1, n: 0 },
    PhaseTableElement { kmin:  641, kmax:  672, i: 0, n: 3 },
    PhaseTableElement { kmin:  673, kmax:  704, i: 3, n: 0 },
    PhaseTableElement { kmin:  705, kmax:  736, i: 2, n: 1 },
    PhaseTableElement { kmin:  737, kmax:  768, i: 1, n: 1 },
];

static H0: [i8; 32] = [0,2,0,0,0,0,1,1,2,0,0,0,2,2,1,1,
                        0,2,0,0,0,0,1,1,2,0,0,0,2,2,1,1];
static H1: [i8; 32] = [0,3,2,3,0,1,3,0,2,1,2,3,2,3,3,0,
                        0,3,2,3,0,1,3,0,2,1,2,3,2,3,3,0];
static H2: [i8; 32] = [0,0,0,2,0,2,1,3,2,2,0,2,2,0,1,3,
                        0,0,0,2,0,2,1,3,2,2,0,2,2,0,1,3];
static H3: [i8; 32] = [0,1,2,1,0,3,3,2,2,3,2,1,2,1,3,2,
                        0,1,2,1,0,3,3,2,2,3,2,1,2,1,3,2];

fn h_table(i: i32, j: i32) -> i32 {
    let j = j as usize;
    match i {
        0 => H0[j] as i32,
        1 => H1[j] as i32,
        2 => H2[j] as i32,
        _ => H3[j] as i32,
    }
}

pub struct PhaseTable {
    table: &'static [PhaseTableElement],
}

impl PhaseTable {
    pub fn new(mode: i16) -> Self {
        // Only Mode I is supported in this port (same as C++ eti-cmdline)
        let table = match mode {
            _ => MODE_I_TABLE,
        };
        PhaseTable { table }
    }

    pub fn get_phi(&self, k: i32) -> f32 {
        for entry in self.table {
            if entry.kmin <= k && k <= entry.kmax {
                let k_prime = entry.kmin;
                let i = entry.i;
                let n = entry.n;
                return PI / 2.0 * (h_table(i, k - k_prime) + n) as f32;
            }
        }
        0.0
    }
}
