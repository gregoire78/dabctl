use eti_rtlsdr_rust::viterbi::{
    ERASED,
    fic_deinterleave_mode_i,
    fic_depuncture_mode_i,
    fic_depuncture_mode_i_soft,
    viterbi_decode,
    viterbi_decode_soft,
};

const TEST_MEMORY: u8 = 6;
const TEST_G: [u8; 4] = [0x5B, 0x79, 0x65, 0x5B];

fn encode_bits_test(state: u8, input_bit: u8) -> [u8; 4] {
    let reg = (input_bit << TEST_MEMORY) | (state & 0x3F);
    [
        (reg & TEST_G[0]).count_ones() as u8 & 1,
        (reg & TEST_G[1]).count_ones() as u8 & 1,
        (reg & TEST_G[2]).count_ones() as u8 & 1,
        (reg & TEST_G[3]).count_ones() as u8 & 1,
    ]
}

#[test]
fn viterbi_decodes_zero_sequence() {
    let encoded = vec![0u8; 32];
    let decoded = viterbi_decode(&encoded);
    assert_eq!(decoded.len(), 8);
    assert_eq!(decoded, vec![0u8; 8]);
}

#[test]
fn viterbi_encode_decode_roundtrip_short() {
    let input = vec![1u8, 0, 1, 1, 0, 0, 1, 0];
    let mut encoded = Vec::new();
    let mut state = 0u8;
    for &bit in &input {
        let enc = encode_bits_test(state, bit);
        encoded.extend_from_slice(&enc);
        state = ((state >> 1) | (bit << (TEST_MEMORY - 1))) & 0x3F;
    }
    assert_eq!(encoded.len(), 32);
    let decoded = viterbi_decode(&encoded);
    assert_eq!(decoded, input);
}

#[test]
fn viterbi_corrects_single_bit_error() {
    let input = vec![1u8, 0, 1, 1, 0, 0, 1, 0];
    let mut encoded = Vec::new();
    let mut state = 0u8;
    for &bit in &input {
        let enc = encode_bits_test(state, bit);
        encoded.extend_from_slice(&enc);
        state = ((state >> 1) | (bit << (TEST_MEMORY - 1))) & 0x3F;
    }
    encoded[5] ^= 1;
    let decoded = viterbi_decode(&encoded);
    assert_eq!(decoded, input);
}

#[test]
fn viterbi_handles_erased_bits() {
    let mut encoded = vec![0u8; 32];
    for i in (1..32).step_by(4) {
        encoded[i] = ERASED;
    }
    let decoded = viterbi_decode(&encoded);
    assert_eq!(decoded, vec![0u8; 8]);
}

#[test]
fn viterbi_soft_roundtrip_short() {
    let input = vec![1u8, 0, 1, 1, 0, 0, 1, 0];
    let mut soft = Vec::new();
    let mut state = 0u8;
    for &bit in &input {
        let enc = encode_bits_test(state, bit);
        for output_bit in enc {
            soft.push(if output_bit == 0 { 64 } else { -64 });
        }
        state = ((state >> 1) | (bit << (TEST_MEMORY - 1))) & 0x3F;
    }

    let decoded = viterbi_decode_soft(&soft);
    assert_eq!(decoded, input);
}

#[test]
fn depuncture_mode_i_soft_passthrough() {
    let bits = vec![12i16, -7, 3, -2, 44, -55, 9, -1];
    let out = fic_depuncture_mode_i_soft(&bits);
    assert_eq!(out, bits);
}

#[test]
fn deinterleave_mode_i_produces_correct_length() {
    let bits = vec![0u8; 3 * 1536 * 2];
    let out = fic_deinterleave_mode_i(&bits);
    assert_eq!(out.len(), 3 * 1536 * 2);
}

#[test]
fn deinterleave_mode_i_is_permutation() {
    let mut bits = vec![0u8; 3 * 1536 * 2];
    for (i, b) in bits.iter_mut().enumerate() {
        *b = (i % 2) as u8;
    }
    let out = fic_deinterleave_mode_i(&bits);
    let sum_in: u32 = bits.iter().map(|&b| b as u32).sum();
    let sum_out: u32 = out.iter().map(|&b| b as u32).sum();
    assert_eq!(sum_in, sum_out);
}

#[test]
fn depuncture_mode_i_passthrough() {
    let bits = vec![0u8, 1, 0, 1, 1, 0, 1, 0];
    let out = fic_depuncture_mode_i(&bits);
    assert_eq!(out, bits);
}
