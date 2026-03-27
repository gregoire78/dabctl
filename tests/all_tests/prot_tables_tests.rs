use eti_rtlsdr_rust::eti_handling::prot_tables::get_pcodes;

#[test]
fn prot_tables_all_24_valid() {
    for i in 0..24 {
        let pcode = get_pcodes(i);
        assert_eq!(pcode.len(), 32);
        // All values should be 0 or 1
        for &v in pcode.iter() {
            assert!(v == 0 || v == 1, "P_Code[{}] has invalid value {}", i, v);
        }
    }
}

#[test]
fn prot_tables_first_is_least_punctured() {
    // P_Code 0 should have the fewest 1s (least punctured = most protection)
    let first_ones: i32 = get_pcodes(0).iter().map(|&v| v as i32).sum();
    let last_ones: i32 = get_pcodes(23).iter().map(|&v| v as i32).sum();
    assert!(first_ones <= last_ones, "First code should have <= ones than last");
}

#[test]
fn prot_tables_last_is_all_ones() {
    // P_Code 24 (index 23) should be all 1s
    let pcode = get_pcodes(23);
    assert!(pcode.iter().all(|&v| v == 1), "Last P_Code should be all 1s");
}

#[test]
fn prot_tables_monotonic_ones_count() {
    // The number of 1s should be non-decreasing across indices
    let mut prev_count = 0i32;
    for i in 0..24 {
        let count: i32 = get_pcodes(i).iter().map(|&v| v as i32).sum();
        assert!(count >= prev_count,
            "P_Code[{}] has {} ones, but P_Code[{}] had {} (should be non-decreasing)",
            i, count, i.wrapping_sub(1), prev_count);
        prev_count = count;
    }
}
