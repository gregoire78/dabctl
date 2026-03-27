use eti_rtlsdr_rust::ofdm::phase_table::PhaseTable;
use std::f32::consts::PI;

#[test]
fn phase_table_mode1_creation() {
    let _pt = PhaseTable::new(1);
}

#[test]
fn phase_table_get_phi_returns_multiples_of_pi_over_2() {
    let pt = PhaseTable::new(1);
    // Phase values should be multiples of PI/2
    for k in [-768, -500, -100, 1, 100, 500, 768] {
        let phi = pt.get_phi(k);
        // phi should be n * PI/2 for some integer n
        let ratio = phi / (PI / 2.0);
        let rounded = ratio.round();
        assert!((ratio - rounded).abs() < 1e-5,
            "get_phi({}) = {} is not a multiple of PI/2", k, phi);
    }
}

#[test]
fn phase_table_dc_carrier_returns_zero() {
    let pt = PhaseTable::new(1);
    // k=0 (DC) is not in the table, should return 0
    assert_eq!(pt.get_phi(0), 0.0);
}

#[test]
fn phase_table_out_of_range() {
    let pt = PhaseTable::new(1);
    // Carrier far out of range → 0.0
    assert_eq!(pt.get_phi(5000), 0.0);
    assert_eq!(pt.get_phi(-5000), 0.0);
}
