// DAB parameters - converted from dab-params.cpp (eti-cmdline)
// Copyright (C) 2013-2017 Jan van Katwijk - Lazy Chair Computing

#[derive(Clone, Debug)]
pub struct DabParams {
    pub dab_mode: i16,
    pub l: i16,         // blocks per frame
    pub k: i16,         // carriers
    pub t_null: i16,    // null symbol length
    pub t_f: i32,       // frame length in samples
    pub t_s: i16,       // symbol length (guard + useful)
    pub t_u: i16,       // useful symbol length (FFT size)
    pub t_g: i16,       // guard interval
    pub carrier_diff: i32,
}

impl DabParams {
    pub fn new(mode: u8) -> Self {
        match mode {
            2 => DabParams {
                dab_mode: 2, l: 76, k: 384, t_null: 664,
                t_f: 49152, t_s: 638, t_u: 512, t_g: 126, carrier_diff: 4000,
            },
            4 => DabParams {
                dab_mode: 4, l: 76, k: 768, t_null: 1328,
                t_f: 98304, t_s: 1276, t_u: 1024, t_g: 252, carrier_diff: 2000,
            },
            3 => DabParams {
                dab_mode: 3, l: 153, k: 192, t_null: 345,
                t_f: 49152, t_s: 319, t_u: 256, t_g: 63, carrier_diff: 2000,
            },
            _ => DabParams { // Mode 1 (default)
                dab_mode: 1, l: 76, k: 1536, t_null: 2656,
                t_f: 196608, t_s: 2552, t_u: 2048, t_g: 504, carrier_diff: 1000,
            },
        }
    }

    pub fn get_carriers(&self) -> usize { self.k as usize }
    pub fn get_l(&self) -> usize { self.l as usize }
}
