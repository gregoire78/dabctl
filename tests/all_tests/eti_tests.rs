use eti_rtlsdr_rust::eti_handling::cif_interleaver::CifInterleaver;

#[test]
fn cif_interleaver_new() {
    let _ci = CifInterleaver::new();
}

#[test]
fn cif_interleaver_returns_none_before_primed() {
    let mut ci = CifInterleaver::new();
    for _ in 0..15 {
        let result = ci.push_and_interleave(&[1, 2, 3, 4]);
        assert!(result.is_none(), "Should return None before 16 pushes");
    }
}

#[test]
fn cif_interleaver_returns_some_after_primed() {
    let mut ci = CifInterleaver::new();
    for i in 0..16 {
        let result = ci.push_and_interleave(&[i as u8; 8]);
        if i < 15 {
            assert!(result.is_none());
        } else {
            assert!(result.is_some(), "Should return Some after 16 pushes");
            let out = result.unwrap();
            assert_eq!(out.len(), 8);
        }
    }
}

#[test]
fn cif_interleaver_output_length_matches_input() {
    let mut ci = CifInterleaver::new();
    for i in 0..20 {
        let payload = vec![i as u8; 32];
        let result = ci.push_and_interleave(&payload);
        if let Some(out) = result {
            assert_eq!(out.len(), 32);
        }
    }
}
