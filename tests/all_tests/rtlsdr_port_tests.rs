use eti_rtlsdr_rust::iq::rtlsdr_port::eti_cmdline_gain_selection;

#[test]
fn rtlsdr_port_gain_selection_matches_edges() {
    let gains = [0, 14, 20, 29, 37, 43, 49];
    let low = eti_cmdline_gain_selection(&gains, 0).expect("low");
    let high = eti_cmdline_gain_selection(&gains, 100).expect("high");
    assert_eq!(low.set_gain_tenth_db, 0);
    assert_eq!(high.set_gain_tenth_db, 49);
}

#[test]
fn rtlsdr_port_gain_selection_midpoint_is_stable() {
    let gains = [0, 14, 20, 29, 37, 43, 49];
    let s = eti_cmdline_gain_selection(&gains, 50).expect("selection");
    assert_eq!(s.set_index, 3);
    assert_eq!(s.set_gain_tenth_db, 29);
}

#[test]
fn rtlsdr_port_gain_selection_clamps_large_percent() {
    let gains = [0, 10, 20, 30];
    let s = eti_cmdline_gain_selection(&gains, 999).expect("selection");
    assert_eq!(s.set_index, 3);
    assert_eq!(s.effective_index, 3);
}

#[test]
fn rtlsdr_port_gain_selection_empty_is_none() {
    assert!(eti_cmdline_gain_selection(&[], 40).is_none());
}

#[test]
fn rtlsdr_port_gain_selection_monotonic() {
    let gains = [0, 10, 20, 30, 40, 50];
    let mut last = i32::MIN;
    for p in 0..=100 {
        let s = eti_cmdline_gain_selection(&gains, p).expect("selection");
        assert!(s.set_gain_tenth_db >= last);
        last = s.set_gain_tenth_db;
    }
}
