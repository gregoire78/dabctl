//! FireCode CRC check for DAB+ super frames
//! Reference: ETSI TS 102 563, Annex A
//!
//! The FireCode is a 16-bit CRC computed over the first 9 bytes of the
//! super frame (bytes 2-10), computed via the Fire code polynomial:
//!   G(x) = (x^11 + 1)(x^5 + x^3 + x^2 + x + 1)
//! which when expanded = x^16 + x^14 + x^13 + x^12 + x^11 + x^5 + x^3 + x^2 + x + 1

/// Precomputed FireCode CRC table (over GF(2) using the Fire polynomial).
/// We use a shift-register approach matching the reference implementation.
fn firecode_crc(data: &[u8]) -> u16 {
    // Fire code polynomial: x^16 + x^14 + x^13 + x^12 + x^11 + x^5 + x^3 + x^2 + x + 1
    // Feedback taps: bits 16, 14, 13, 12, 11, 5, 3, 2, 1, 0 (0-indexed from LSB)
    // In 16-bit register: taps at 14, 13, 12, 11, 5, 3, 2, 1, 0 (after shifting out bit 15)
    // Taps at positions: 14,13,12,11,5,3,2,1,0 (bit 0 = x^0 implicit via feedback)
    const POLY: u16 = 0b0111_1000_0010_1111;

    let mut crc: u16 = 0;
    for &byte in data {
        for bit in (0..8).rev() {
            let input_bit = ((byte >> bit) & 1) as u16;
            let feedback = (crc >> 15) ^ input_bit;
            crc <<= 1; // u16 wraps automatically at 16 bits
            if feedback != 0 {
                crc ^= POLY; // POLY already has bit 0 set (0x2F has bit 0 = 1)
            }
        }
    }
    crc
}

/// Check the FireCode CRC of a DAB+ super frame.
///
/// The super frame begins with 2 bytes of FireCode, followed by the payload.
/// FireCode is computed over bytes [2..11] (9 bytes) of the super frame.
///
/// Returns `true` if the FireCode matches (valid super frame start).
pub fn check_firecode(superframe: &[u8]) -> bool {
    if superframe.len() < 11 {
        return false;
    }
    let stored_fc = ((superframe[0] as u16) << 8) | superframe[1] as u16;
    let computed = firecode_crc(&superframe[2..11]);
    stored_fc == computed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_firecode_wrong_data_fails() {
        // All-zeros super frame cannot have a valid firecode that matches zeros
        // (unless the polynomial maps all-zeros to 0x0000, which it does not for non-trivial data)
        let mut sf = vec![0u8; 20];
        let fc = firecode_crc(&sf[2..11]);
        // With all-zeros input the CRC should be 0 (linear code)
        assert_eq!(fc, 0);
        // Stored firecode = 0x0000, computed = 0 → check passes for this degenerate case
        assert!(check_firecode(&sf));

        // Now corrupt a data byte
        sf[3] = 0x01;
        assert!(!check_firecode(&sf));
    }

    #[test]
    fn test_firecode_check_known_good() {
        // Construct a super frame with correct FireCode
        let mut sf = vec![0u8; 20];
        // Set some non-zero data bytes 2..11
        for i in 2..11 {
            sf[i] = (i * 3) as u8;
        }
        let fc = firecode_crc(&sf[2..11]);
        sf[0] = (fc >> 8) as u8;
        sf[1] = (fc & 0xff) as u8;
        assert!(check_firecode(&sf));
    }

    #[test]
    fn test_firecode_too_short() {
        let sf = vec![0u8; 5];
        assert!(!check_firecode(&sf));
    }
}
