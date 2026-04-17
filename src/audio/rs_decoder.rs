/// Reed-Solomon (120,110) decoder for DAB+ superframes.
/// Uses GF(2^8), generator polynomial 0x11D, FCR=0, prim=1, nroots=10, pad=135.
/// This is RS(255,245) shortened to (120,110).
/// ETSI TS 102 563 §6.
use std::sync::OnceLock;

const GF_POLY: u16 = 0x11D; // x^8 + x^4 + x^3 + x^2 + 1
const NROOTS: usize = 10;
const NN: usize = 255;
const PAD: usize = 135;
const BLOCK_SIZE: usize = 120;

fn superframe_column_count(sf_len: usize) -> usize {
    sf_len / BLOCK_SIZE
}

fn copy_interleaved_column(
    sf: &[u8],
    subch_index: usize,
    column_index: usize,
    packet: &mut [u8; BLOCK_SIZE],
) {
    for row in 0..BLOCK_SIZE {
        packet[row] = sf[row * subch_index + column_index];
    }
}

fn write_interleaved_column(
    sf: &mut [u8],
    subch_index: usize,
    column_index: usize,
    packet: &[u8; BLOCK_SIZE],
) {
    for row in 0..BLOCK_SIZE {
        sf[row * subch_index + column_index] = packet[row];
    }
}

/// GF(2^8) arithmetic tables
struct GfTables {
    alpha_to: [u8; 256],
    index_of: [u8; 256],
    #[allow(dead_code)]
    genpoly: [u8; NROOTS + 1],
}

fn build_gf_tables() -> GfTables {
    let mut alpha_to = [0u8; 256];
    let mut index_of = [0u8; 256];

    // Generate GF(2^8) using primitive polynomial 0x11D
    let mut sr: u16 = 1;
    for i in 0..255u16 {
        alpha_to[i as usize] = sr as u8;
        index_of[sr as usize] = i as u8;
        sr <<= 1;
        if sr & 0x100 != 0 {
            sr ^= GF_POLY;
        }
    }
    alpha_to[255] = 0;
    index_of[0] = 255; // sentinel for zero

    // Generate generator polynomial
    // g(x) = prod(x - alpha^i) for i = 0..NROOTS-1
    let mut genpoly = [0u8; NROOTS + 1];
    genpoly[0] = 1;
    let mut degree = 0usize;

    for i in 0..NROOTS {
        degree += 1;
        genpoly[degree] = 1;
        for j in (1..degree).rev() {
            if genpoly[j] != 0 {
                let idx = (index_of[genpoly[j] as usize] as u16 + i as u16) % 255;
                genpoly[j] = genpoly[j - 1] ^ alpha_to[idx as usize];
            } else {
                genpoly[j] = genpoly[j - 1];
            }
        }
        let idx = (index_of[genpoly[0] as usize] as u16 + i as u16) % 255;
        genpoly[0] = alpha_to[idx as usize];
    }

    // Convert to index form
    for coeff in genpoly.iter_mut() {
        *coeff = index_of[*coeff as usize];
    }

    GfTables {
        alpha_to,
        index_of,
        genpoly,
    }
}

/// RS decoder state
pub struct RsDecoder {
    gf: &'static GfTables,
}

/// Global GF(2^8) tables, computed once at first use.
static GF_TABLES: OnceLock<GfTables> = OnceLock::new();

impl Default for RsDecoder {
    fn default() -> Self {
        Self::new()
    }
}

impl RsDecoder {
    /// Create a new RS decoder.  GF tables are shared across all instances
    /// via a process-wide `OnceLock`; construction is O(1) after the first call.
    pub fn new() -> Self {
        RsDecoder {
            gf: GF_TABLES.get_or_init(build_gf_tables),
        }
    }

    /// Decode a DAB+ superframe in-place.
    /// `sf` is the full superframe buffer, `sf_len` must be a multiple of 120.
    /// Returns (total_corrections, has_uncorrectable_errors).
    pub fn decode_superframe(&self, sf: &mut [u8]) -> (usize, bool) {
        let subch_index = superframe_column_count(sf.len());
        let mut total_corr = 0;
        let mut uncorr = false;
        let mut rs_packet = [0u8; BLOCK_SIZE];

        for column_index in 0..subch_index {
            match self.decode_superframe_column(sf, subch_index, column_index, &mut rs_packet) {
                Some(corr_count) => total_corr += corr_count,
                None => uncorr = true,
            }
        }

        (total_corr, uncorr)
    }

    fn decode_superframe_column(
        &self,
        sf: &mut [u8],
        subch_index: usize,
        column_index: usize,
        rs_packet: &mut [u8; BLOCK_SIZE],
    ) -> Option<usize> {
        copy_interleaved_column(sf, subch_index, column_index, rs_packet);
        let corr_count = self.decode_rs(rs_packet)?;
        write_interleaved_column(sf, subch_index, column_index, rs_packet);
        Some(corr_count)
    }

    /// Decode a single RS(120,110) block. Returns number of corrections or None if uncorrectable.
    fn decode_rs(&self, data: &mut [u8; BLOCK_SIZE]) -> Option<usize> {
        let gf = &self.gf;

        // Pad the shortened codeword to full length
        let mut block = [0u8; NN];
        // data[0..110] = information, data[110..120] = parity
        // In shortened RS: PAD zeros at beginning, then data, then parity
        block[PAD..PAD + BLOCK_SIZE].copy_from_slice(data);

        // Compute syndromes
        let mut syndromes = [0u8; NROOTS];
        let mut has_errors = false;

        for (i, syn_out) in syndromes.iter_mut().enumerate() {
            let mut syn: u8 = 0;
            for &bv in block.iter() {
                if syn == 0 {
                    syn = bv;
                } else {
                    let idx = (gf.index_of[syn as usize] as u16 + i as u16) % 255;
                    syn = bv ^ gf.alpha_to[idx as usize];
                }
            }
            *syn_out = syn;
            if syn != 0 {
                has_errors = true;
            }
        }

        if !has_errors {
            return Some(0);
        }

        // Berlekamp-Massey algorithm
        let mut lambda = [0u8; NROOTS + 1]; // Error locator polynomial
        lambda[0] = 1;

        let mut b = [0u8; NROOTS + 1];
        b[0] = 1;

        let mut l = 0usize;

        for n in 0..NROOTS {
            // Compute discrepancy
            let mut delta: u8 = syndromes[n];
            for i in 1..=l {
                if lambda[i] != 0 && syndromes[n - i] != 0 {
                    let idx = (gf.index_of[lambda[i] as usize] as u16
                        + gf.index_of[syndromes[n - i] as usize] as u16)
                        % 255;
                    delta ^= gf.alpha_to[idx as usize];
                }
            }

            // Shift b
            for i in (1..=NROOTS).rev() {
                b[i] = b[i - 1];
            }
            b[0] = 0;

            if delta != 0 {
                let delta_idx = gf.index_of[delta as usize];

                // t(x) = lambda(x) - delta * x * b(x)
                let mut t = [0u8; NROOTS + 1];
                for (t_val, (&lam_val, &b_val)) in t.iter_mut().zip(lambda.iter().zip(b.iter())) {
                    *t_val = lam_val;
                    if b_val != 0 {
                        let idx = (gf.index_of[b_val as usize] as u16 + delta_idx as u16) % 255;
                        *t_val ^= gf.alpha_to[idx as usize];
                    }
                }

                if 2 * l <= n {
                    l = n + 1 - l;
                    // b = lambda / delta
                    let inv_delta_idx = (255 - delta_idx as u16) % 255;
                    for i in 0..=NROOTS {
                        if lambda[i] != 0 {
                            let idx =
                                (gf.index_of[lambda[i] as usize] as u16 + inv_delta_idx) % 255;
                            b[i] = gf.alpha_to[idx as usize];
                        } else {
                            b[i] = 0;
                        }
                    }
                }

                lambda = t;
            }
        }

        // Find error locations by Chien search.
        // At most NROOTS errors can be corrected, so a stack array suffices.
        let mut error_locs = [0usize; NROOTS];
        let mut n_errors = 0usize;
        for i in 0..NN {
            let mut sum: u8 = 0;
            for (j, &lam_j) in lambda.iter().enumerate().take(NROOTS + 1) {
                if lam_j != 0 {
                    let idx =
                        (gf.index_of[lam_j as usize] as u16 + (j as u16 * i as u16) % 255) % 255;
                    sum ^= gf.alpha_to[idx as usize];
                }
            }
            if sum == 0 {
                // Chien root at α^i → block position p where α^{p+1 mod 255} = α^i,
                // so p = (i - 1) mod 255 = (i + 254) % 255.
                let pos = (i + NN - 1) % NN;
                if pos < PAD {
                    // Error in padding (shortened zeros) — correctable but outside data
                    continue;
                }
                if n_errors < NROOTS {
                    error_locs[n_errors] = pos;
                    n_errors += 1;
                }
            }
        }

        if n_errors != l {
            return None; // Uncorrectable
        }

        // Forney algorithm — compute error magnitudes via explicit Ω.
        // Ω(x) = S(x)·Λ(x) mod x^NROOTS   (error evaluator polynomial)
        let mut omega = [0u8; NROOTS + 1];
        for i in 0..NROOTS {
            omega[i] = syndromes[i];
            for j in 1..=i.min(l) {
                if lambda[j] != 0 && syndromes[i - j] != 0 {
                    let idx = (gf.index_of[lambda[j] as usize] as u16
                        + gf.index_of[syndromes[i - j] as usize] as u16)
                        % 255;
                    omega[i] ^= gf.alpha_to[idx as usize];
                }
            }
        }

        // For each error location, compute the error magnitude.
        // Forney formula for FCR=0: e = X_j · Ω(X_j^{-1}) / Λ'(X_j^{-1})
        // where X_j = α^{254-p} for block position p.  ETSI TS 102 563 §6.
        for &loc in &error_locs[..n_errors] {
            // Evaluation point: X_j^{-1} = α^{root} where root = (p+1) % 255
            let root = (loc as u16 + 1) % 255;

            // Evaluate Ω at α^root
            let mut omega_val: u8 = 0;
            for (i, &om_i) in omega.iter().enumerate().take(NROOTS) {
                if om_i != 0 {
                    let idx = (gf.index_of[om_i as usize] as u16 + (i as u16 * root) % 255) % 255;
                    omega_val ^= gf.alpha_to[idx as usize];
                }
            }

            // Evaluate Λ'(α^root) — formal derivative: only odd-power terms in GF(2)
            let mut lambda_prime: u8 = 0;
            for i in (1..=l).step_by(2) {
                if lambda[i] != 0 {
                    let idx = (gf.index_of[lambda[i] as usize] as u16
                        + ((i - 1) as u16 * root) % 255)
                        % 255;
                    lambda_prime ^= gf.alpha_to[idx as usize];
                }
            }

            if lambda_prime == 0 || omega_val == 0 {
                return None;
            }

            // err = X_j · Ω / Λ' = α^{254-loc} · Ω_val / Λ'_val
            let x_j_log = (254 - loc as u16 + 255) % 255; // log(X_j) = 254-loc (mod 255)
            let err_val_idx = (gf.index_of[omega_val as usize] as u16 + 255
                - gf.index_of[lambda_prime as usize] as u16
                + x_j_log)
                % 255;
            let err_val = gf.alpha_to[err_val_idx as usize];

            if (PAD..PAD + BLOCK_SIZE).contains(&loc) {
                block[loc] ^= err_val;
            }
        }

        // Copy corrected data back
        data.copy_from_slice(&block[PAD..PAD + BLOCK_SIZE]);
        Some(n_errors)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gf_tables_consistency() {
        let gf = build_gf_tables();
        // alpha_to[0] should be 1 (alpha^0 = 1)
        assert_eq!(gf.alpha_to[0], 1);
        // index_of[1] should be 0
        assert_eq!(gf.index_of[1], 0);
        // alpha_to[index_of[x]] == x for all non-zero x
        for x in 1..=255u16 {
            let idx = gf.index_of[x as usize];
            if idx != 255 {
                assert_eq!(gf.alpha_to[idx as usize], x as u8);
            }
        }
    }

    #[test]
    fn test_no_errors() {
        let rs = RsDecoder::new();
        // An all-zero block has no errors and valid zero syndromes
        let mut block = [0u8; BLOCK_SIZE];
        let result = rs.decode_rs(&mut block);
        assert_eq!(result, Some(0));
    }

    /// Encode a RS(120,110) block: compute 10 parity bytes for 110 data bytes.
    /// Uses systematic encoding with the generator polynomial.
    fn rs_encode(data: &[u8; 110]) -> [u8; BLOCK_SIZE] {
        let gf = GF_TABLES.get_or_init(build_gf_tables);
        let mut codeword = [0u8; BLOCK_SIZE];
        codeword[..110].copy_from_slice(data);

        // Work on the full 255-byte frame (shortened code: PAD leading zeros)
        let mut full = [0u8; NN];
        full[PAD..PAD + 110].copy_from_slice(data);

        // Systematic encoding: compute remainder of message polynomial
        // divided by generator polynomial (both in index form).
        let mut feedback;
        let mut parity = [0u8; NROOTS];

        for i in PAD..PAD + 110 {
            feedback = gf.index_of[full[i] as usize ^ parity[0] as usize];
            if feedback != 255 {
                for j in 1..NROOTS {
                    if gf.genpoly[NROOTS - j] != 255 {
                        let idx = (feedback as u16 + gf.genpoly[NROOTS - j] as u16) % 255;
                        parity[j] ^= gf.alpha_to[idx as usize];
                    }
                }
            }
            // Shift parity register
            parity.copy_within(1.., 0);
            if feedback != 255 {
                let idx = (feedback as u16 + gf.genpoly[0] as u16) % 255;
                parity[NROOTS - 1] = gf.alpha_to[idx as usize];
            } else {
                parity[NROOTS - 1] = 0;
            }
        }

        codeword[110..120].copy_from_slice(&parity);
        codeword
    }

    #[test]
    fn test_encode_then_decode_no_errors() {
        let rs = RsDecoder::new();
        let mut data = [0u8; 110];
        data[0] = 0x42;
        data[50] = 0xFF;
        data[109] = 0x01;
        let mut block = rs_encode(&data);
        let result = rs.decode_rs(&mut block);
        assert_eq!(result, Some(0));
        assert_eq!(&block[..110], &data[..]);
    }

    #[test]
    fn test_correct_1_error() {
        let rs = RsDecoder::new();
        let mut data = [0u8; 110];
        data[0] = 0xAB;
        data[42] = 0xCD;
        let original = data;
        let mut block = rs_encode(&data);

        // Introduce 1 error
        block[20] ^= 0x55;

        let result = rs.decode_rs(&mut block);
        assert_eq!(result, Some(1));
        assert_eq!(&block[..110], &original[..]);
    }

    #[test]
    fn test_correct_2_errors() {
        let rs = RsDecoder::new();
        let mut data = [0u8; 110];
        data[10] = 0x12;
        data[99] = 0x34;
        let original = data;
        let mut block = rs_encode(&data);

        // Introduce 2 errors
        block[0] ^= 0xFF;
        block[109] ^= 0x01;

        let result = rs.decode_rs(&mut block);
        assert_eq!(result, Some(2));
        assert_eq!(&block[..110], &original[..]);
    }

    #[test]
    fn test_correct_3_errors() {
        let rs = RsDecoder::new();
        let mut data = [0u8; 110];
        for i in 0..110 {
            data[i] = (i * 7 + 3) as u8;
        }
        let original = data;
        let mut block = rs_encode(&data);

        // Introduce 3 errors
        block[5] ^= 0xAA;
        block[55] ^= 0x55;
        block[105] ^= 0xFF;

        let result = rs.decode_rs(&mut block);
        assert_eq!(result, Some(3));
        assert_eq!(&block[..110], &original[..]);
    }

    #[test]
    fn test_correct_5_errors_maximum() {
        // RS(120,110) can correct up to NROOTS/2 = 5 byte errors
        let rs = RsDecoder::new();
        let mut data = [0u8; 110];
        for i in 0..110 {
            data[i] = (i as u8).wrapping_mul(13).wrapping_add(7);
        }
        let original = data;
        let mut block = rs_encode(&data);

        // Introduce 5 errors (maximum correctable)
        block[0] ^= 0x01;
        block[30] ^= 0x80;
        block[60] ^= 0x42;
        block[90] ^= 0xFE;
        block[110] ^= 0x33; // error in parity byte

        let result = rs.decode_rs(&mut block);
        assert_eq!(result, Some(5));
        assert_eq!(&block[..110], &original[..]);
    }

    #[test]
    fn test_6_errors_uncorrectable() {
        // 6 errors exceeds the correction capacity
        let rs = RsDecoder::new();
        let mut data = [0u8; 110];
        data[0] = 0x42;
        let mut block = rs_encode(&data);

        // Introduce 6 errors
        block[0] ^= 0x01;
        block[20] ^= 0x02;
        block[40] ^= 0x03;
        block[60] ^= 0x04;
        block[80] ^= 0x05;
        block[100] ^= 0x06;

        let result = rs.decode_rs(&mut block);
        assert!(result.is_none(), "6 errors should be uncorrectable");
    }

    #[test]
    fn test_superframe_single_column() {
        // A superframe with subch_index=1 is a single 120-byte RS block
        let rs = RsDecoder::new();
        let mut data = [0u8; 110];
        data[0] = 0x99;
        let original = data;
        let mut sf = rs_encode(&data).to_vec();

        // Introduce 2 errors
        sf[10] ^= 0xAA;
        sf[50] ^= 0xBB;

        let (corr, uncorr) = rs.decode_superframe(&mut sf);
        assert_eq!(corr, 2);
        assert!(!uncorr);
        assert_eq!(&sf[..110], &original[..]);
    }

    #[test]
    fn superframe_column_count_matches_interleaved_width() {
        assert_eq!(superframe_column_count(BLOCK_SIZE), 1);
        assert_eq!(superframe_column_count(BLOCK_SIZE * 3), 3);
    }

    #[test]
    fn interleaved_column_round_trip_preserves_bytes() {
        let mut sf = vec![0u8; BLOCK_SIZE * 2];
        for row in 0..BLOCK_SIZE {
            sf[row * 2] = row as u8;
            sf[row * 2 + 1] = (200 + row) as u8;
        }

        let mut packet = [0u8; BLOCK_SIZE];
        copy_interleaved_column(&sf, 2, 1, &mut packet);
        assert_eq!(packet[0], 200);
        assert_eq!(packet[10], 210);

        packet[0] = 7;
        write_interleaved_column(&mut sf, 2, 1, &packet);
        assert_eq!(sf[1], 7);
    }
}
