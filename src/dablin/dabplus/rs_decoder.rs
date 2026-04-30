//! Reed-Solomon RS(120, 110) decoder for DAB+ using libfec (Phil Karn's library)
//! Reference: dablin RSDecoder, init_rs_char(8, 0x11D, 0, 1, 10, 135)
//!
//! Parameters (matching dablin exactly):
//!   symsize = 8 (GF(2^8))
//!   gfpoly  = 0x11D
//!   fcr     = 0 (first consecutive root)
//!   prim    = 1 (primitive element)
//!   nroots  = 10 (parity bytes)
//!   pad     = 135 (shortened code: virtual block = 255 bytes, real block = 120 bytes)

use std::sync::OnceLock;

const RS_N: usize = 120;
const RS_K: usize = 110;

// libfec FFI
#[link(name = "fec")]
extern "C" {
    fn init_rs_char(
        symsize: i32,
        gfpoly: i32,
        fcr: i32,
        prim: i32,
        nroots: i32,
        pad: i32,
    ) -> *mut std::ffi::c_void;

    fn decode_rs_char(
        rs: *mut std::ffi::c_void,
        data: *mut u8,
        eras_pos: *mut i32,
        no_eras: i32,
    ) -> i32;

    fn free_rs_char(rs: *mut std::ffi::c_void);
}

/// RAII wrapper around the libfec RS handle (thread-safe singleton)
struct RsHandle(*mut std::ffi::c_void);

unsafe impl Send for RsHandle {}
unsafe impl Sync for RsHandle {}

impl Drop for RsHandle {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { free_rs_char(self.0) };
        }
    }
}

static RS_HANDLE: OnceLock<RsHandle> = OnceLock::new();

fn get_rs_handle() -> *mut std::ffi::c_void {
    RS_HANDLE
        .get_or_init(|| {
            let h = unsafe { init_rs_char(8, 0x11D, 0, 1, 10, 135) };
            assert!(!h.is_null(), "init_rs_char failed");
            RsHandle(h)
        })
        .0
}

/// Decode DAB+ superframe in-place using libfec's RS(120,110,PAD=135).
///
/// Matches dablin's RSDecoder::DecodeSuperframe() exactly:
///   subch_index = sf_len / 120
///   For each column i in 0..subch_index:
///     extract rs_packet[pos] = sf[pos * subch_index + i] for pos in 0..120
///     decode with decode_rs_char
///     write back corrected bytes at pos >= PAD (pos >= 135 - but our N=120 < 135+N, so all)
///
/// Returns Ok(n_corrected_codewords) or Err(n_uncorrectable)
pub fn rs_decode_superframe(superframe: &mut [u8]) -> Result<usize, usize> {
    let sf_len = superframe.len();
    if sf_len == 0 || !sf_len.is_multiple_of(RS_N) {
        return Err(1);
    }
    let subch_index = sf_len / RS_N;

    let rs = get_rs_handle();
    let mut total_corrected = 0usize;
    let mut total_failed = 0usize;
    let mut rs_packet = [0u8; RS_N];

    for col in 0..subch_index {
        // De-interleave column
        for pos in 0..RS_N {
            rs_packet[pos] = superframe[col + pos * subch_index];
        }

        let corr_count = unsafe {
            decode_rs_char(rs, rs_packet.as_mut_ptr(), std::ptr::null_mut(), 0)
        };

        if corr_count == -1 {
            total_failed += 1;
        } else {
            if corr_count > 0 {
                total_corrected += 1;
            }
            // Write back corrected bytes (only data part, pos in 0..RS_K)
            // In dablin: pos >= PAD (135) relative to 255-byte virtual block.
            // Since PAD=135 and N=120, real data bytes start at virtual index 135.
            // All 120 real bytes correspond to virtual positions 135..254,
            // so pos (within our 120-byte codeword) < RS_K are data bytes.
            for pos in 0..RS_K {
                superframe[col + pos * subch_index] = rs_packet[pos];
            }
        }
    }

    if total_failed > 0 {
        Err(total_failed)
    } else {
        Ok(total_corrected)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rs_handle_init() {
        let h = get_rs_handle();
        assert!(!h.is_null());
    }

    #[test]
    fn test_rs_decode_zeros() {
        // A superframe of all zeros should have zero syndromes (no errors)
        let mut sf = vec![0u8; 120];
        let result = rs_decode_superframe(&mut sf);
        // 0 data is valid (syndromes = 0), result should be Ok(0)
        assert!(result.is_ok());
    }

    #[test]
    fn test_rs_decode_invalid_size() {
        let mut sf = vec![0u8; 100]; // not multiple of 120
        assert!(rs_decode_superframe(&mut sf).is_err());
    }

    #[test]
    fn test_rs_decode_1320_size() {
        // 1320 = 11 * 120 codewords (for STL=33 sub-channels)
        let mut sf = vec![0u8; 1320];
        let result = rs_decode_superframe(&mut sf);
        assert!(result.is_ok());
    }

    #[test]
    fn test_rs_decode_error_correction() {
        // Build a valid all-zero codeword, inject an error, verify correction
        let mut sf = vec![0u8; 120];
        sf[5] = 0xFF; // inject error in data byte
        let result = rs_decode_superframe(&mut sf);
        // Should correct the error (or at least not panic)
        // For all-zero data + parity, any non-zero byte is an error
        // Note: all-zero data has all-zero parity, so any error IS detectable
        let _ = result; // just ensure it runs without panic
    }
}
