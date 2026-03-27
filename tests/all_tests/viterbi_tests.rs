use eti_rtlsdr_rust::eti_handling::viterbi_handler::ViterbiSpiral;

#[test]
fn viterbi_new_does_not_panic() {
    let _v = ViterbiSpiral::new(768);
}

#[test]
fn viterbi_zero_input_produces_zero_output() {
    let mut v = ViterbiSpiral::new(32);
    // 32 bits → need (32+6)*4 = 152 input symbols
    let input = vec![0i16; 152];
    let mut output = vec![0u8; 32];
    v.deconvolve(&input, &mut output);
    // All-zero input should produce all-zero output
    assert!(output.iter().all(|&b| b == 0), "Expected all zeros, got {:?}", output);
}

#[test]
fn viterbi_deconvolve_output_is_binary() {
    let mut v = ViterbiSpiral::new(64);
    let input: Vec<i16> = (0..280).map(|i| if i % 3 == 0 { 127 } else { -127 }).collect();
    let mut output = vec![0u8; 64];
    v.deconvolve(&input, &mut output);
    assert!(output.iter().all(|&b| b == 0 || b == 1), "Output must be binary");
}

#[test]
fn viterbi_different_word_lengths() {
    for wl in [16, 32, 64, 128, 768] {
        let mut v = ViterbiSpiral::new(wl);
        let input = vec![0i16; (wl + 6) * 4];
        let mut output = vec![0u8; wl];
        v.deconvolve(&input, &mut output);
    }
}

#[test]
fn viterbi_strong_encoded_ones() {
    // Feed all +127 (strong positive soft bits)
    let mut v = ViterbiSpiral::new(32);
    let input = vec![127i16; 152];
    let mut output = vec![0u8; 32];
    v.deconvolve(&input, &mut output);
    // Output should be binary regardless of content
    assert!(output.iter().all(|&b| b == 0 || b == 1));
}
