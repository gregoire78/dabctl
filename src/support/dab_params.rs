// DAB parameters - converted from dab-params.cpp (eti-cmdline)

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode1_defaults() {
        let p = DabParams::new(1);
        assert_eq!(p.dab_mode, 1);
        assert_eq!(p.l, 76);
        assert_eq!(p.k, 1536);
        assert_eq!(p.t_u, 2048);
        assert_eq!(p.t_s, 2552);
        assert_eq!(p.t_g, 504);
        assert_eq!(p.t_null, 2656);
        assert_eq!(p.t_f, 196608);
        assert_eq!(p.carrier_diff, 1000);
    }

    #[test]
    fn mode2() {
        let p = DabParams::new(2);
        assert_eq!(p.dab_mode, 2);
        assert_eq!(p.l, 76);
        assert_eq!(p.k, 384);
        assert_eq!(p.t_u, 512);
    }

    #[test]
    fn mode3() {
        let p = DabParams::new(3);
        assert_eq!(p.dab_mode, 3);
        assert_eq!(p.l, 153);
        assert_eq!(p.k, 192);
        assert_eq!(p.t_u, 256);
    }

    #[test]
    fn mode4() {
        let p = DabParams::new(4);
        assert_eq!(p.dab_mode, 4);
        assert_eq!(p.l, 76);
        assert_eq!(p.k, 768);
        assert_eq!(p.t_u, 1024);
    }

    #[test]
    fn invalid_defaults_to_mode1() {
        let p = DabParams::new(99);
        assert_eq!(p.dab_mode, 1);
        assert_eq!(p.k, 1536);
    }

    #[test]
    fn get_carriers() {
        assert_eq!(DabParams::new(1).get_carriers(), 1536);
        assert_eq!(DabParams::new(2).get_carriers(), 384);
    }

    #[test]
    fn get_l() {
        assert_eq!(DabParams::new(1).get_l(), 76);
        assert_eq!(DabParams::new(3).get_l(), 153);
    }

    #[test]
    fn symbol_length_is_tu_plus_tg() {
        for mode in [1, 2, 3, 4] {
            let p = DabParams::new(mode);
            assert_eq!(p.t_s, p.t_u + p.t_g, "t_s != t_u + t_g for mode {}", mode);
        }
    }
}
