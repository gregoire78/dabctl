use eti_rtlsdr_rust::support::dab_params::DabParams;
use eti_rtlsdr_rust::ofdm::freq_interleaver::FreqInterleaver;

#[test]
fn freq_interleaver_mode1_size() {
    let params = DabParams::new(1);
    let fi = FreqInterleaver::new(&params);
    // Should be able to map all carrier indices 0..K-1
    for i in 0..params.k as usize {
        let _mapped = fi.map_in(i);
    }
}

#[test]
fn freq_interleaver_mode1_range() {
    let params = DabParams::new(1);
    let fi = FreqInterleaver::new(&params);
    let half_k = params.k / 2;
    for i in 0..params.k as usize {
        let m = fi.map_in(i);
        assert!(m >= -half_k && m <= half_k, "map_in({}) = {} out of range", i, m);
        assert_ne!(m, 0, "Carrier 0 (DC) should never appear");
    }
}

#[test]
fn freq_interleaver_mode1_no_duplicates() {
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
fn freq_interleaver_all_modes() {
    for mode in [1, 2, 3, 4] {
        let params = DabParams::new(mode);
        let fi = FreqInterleaver::new(&params);
        // Should produce exactly K unique mappings
        let mut set = std::collections::HashSet::new();
        for i in 0..params.k as usize {
            set.insert(fi.map_in(i));
        }
        assert_eq!(set.len(), params.k as usize, "Mode {} should have K unique mappings", mode);
    }
}
