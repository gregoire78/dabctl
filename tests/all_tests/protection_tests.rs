use eti_rtlsdr_rust::eti_handling::protection::{EepProtection, Protection};

#[test]
fn eep_protection_creation_a1() {
    let _p = EepProtection::new(64, 0); // 64 kbps, prot level A1
}

#[test]
fn eep_protection_creation_a2() {
    let _p = EepProtection::new(128, 1); // A2
}

#[test]
fn eep_protection_creation_a3() {
    let _p = EepProtection::new(64, 2); // A3
}

#[test]
fn eep_protection_creation_a4() {
    let _p = EepProtection::new(64, 3); // A4
}

#[test]
fn eep_protection_creation_b_profiles() {
    // B profiles have bit 2 set
    for level in 4..8 {
        let _p = EepProtection::new(64, level);
    }
}

#[test]
fn eep_protection_deconvolve_zero_input() {
    let mut p = EepProtection::new(64, 0);
    let in_size = 24 * 64; // out_size for 64kbps
    let input = vec![0i16; in_size * 4 + 24]; // approximate input size
    let mut output = vec![0u8; in_size];
    p.deconvolve(&input, &mut output);
    // Should not panic, output should be binary
    assert!(output.iter().all(|&b| b == 0 || b == 1));
}

#[test]
fn protection_enum_eep() {
    let mut prot = Protection::Eep(EepProtection::new(64, 0));
    let in_size = 24 * 64;
    let input = vec![0i16; in_size * 4 + 24];
    let mut output = vec![0u8; in_size];
    prot.deconvolve(&input, &mut output);
}
