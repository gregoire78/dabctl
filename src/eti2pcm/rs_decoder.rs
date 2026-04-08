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
        let subch_index = sf.len() / BLOCK_SIZE;
        let mut total_corr = 0;
        let mut uncorr = false;

        let mut rs_packet = [0u8; BLOCK_SIZE];

        for i in 0..subch_index {
            // De-interleave: collect column i
            for pos in 0..BLOCK_SIZE {
                rs_packet[pos] = sf[pos * subch_index + i];
            }

            // Decode
            match self.decode_rs(&mut rs_packet) {
                Some(corr_count) => {
                    total_corr += corr_count;
                    // Write corrections back
                    for pos in 0..BLOCK_SIZE {
                        sf[pos * subch_index + i] = rs_packet[pos];
                    }
                }
                None => {
                    uncorr = true;
                }
            }
        }

        (total_corr, uncorr)
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
                for (t_val, (&lam_val, &b_val)) in t
                    .iter_mut()
                    .zip(lambda.iter().zip(b.iter()))
                {
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

        // Find error locations by Chien search
        let mut error_locs = Vec::new();
        for i in 0..NN {
            let mut sum: u8 = 0;
            for (j, &lam_j) in lambda.iter().enumerate().take(NROOTS + 1) {
                if lam_j != 0 {
                    let idx = (gf.index_of[lam_j as usize] as u16
                        + (j as u16 * i as u16) % 255)
                        % 255;
                    sum ^= gf.alpha_to[idx as usize];
                }
            }
            if sum == 0 {
                // Error at position NN - 1 - i
                let pos = NN - 1 - i;
                if pos < PAD {
                    // Error in padding (shouldn't happen in valid shortened code, but it's correctable)
                    continue;
                }
                error_locs.push(pos);
            }
        }

        if error_locs.len() != l {
            return None; // Uncorrectable
        }

        // Forney algorithm to compute error magnitudes
        // First, compute omega (error evaluator polynomial)
        let mut omega = [0u8; NROOTS + 1];
        for (i, (om, &syn_i)) in omega
            .iter_mut()
            .zip(syndromes.iter())
            .enumerate()
            .take(NROOTS)
        {
            *om = syn_i;
            for j in 1..=i.min(l) {
                if lambda[j] != 0 && syndromes[i - j] != 0 {
                    let idx = (gf.index_of[lambda[j] as usize] as u16
                        + gf.index_of[syndromes[i - j] as usize] as u16)
                        % 255;
                    *om ^= gf.alpha_to[idx as usize];
                }
            }
        }

        // Compute error values
        for &loc in &error_locs {
            let xi_inv = (255 - loc as u16) % 255; // alpha^(-loc)

            // Evaluate omega at xi_inv
            let mut omega_val: u8 = 0;
            for (i, &om_i) in omega.iter().enumerate().take(NROOTS) {
                if om_i != 0 {
                    let idx =
                        (gf.index_of[om_i as usize] as u16 + xi_inv * i as u16 % 255) % 255;
                    omega_val ^= gf.alpha_to[idx as usize];
                }
            }

            // Evaluate lambda' (formal derivative) at xi_inv
            let mut lambda_prime: u8 = 0;
            for i in (1..=l).step_by(2) {
                if lambda[i] != 0 {
                    let idx = (gf.index_of[lambda[i] as usize] as u16
                        + xi_inv * (i - 1) as u16 % 255)
                        % 255;
                    lambda_prime ^= gf.alpha_to[idx as usize];
                }
            }

            if lambda_prime == 0 {
                return None;
            }

            // Error magnitude = omega / lambda'
            let err_val_idx = (gf.index_of[omega_val as usize] as u16 + 255
                - gf.index_of[lambda_prime as usize] as u16)
                % 255;
            let err_val = gf.alpha_to[err_val_idx as usize];

            if (PAD..PAD + BLOCK_SIZE).contains(&loc) {
                block[loc] ^= err_val;
            }
        }

        // Copy corrected data back
        data.copy_from_slice(&block[PAD..PAD + BLOCK_SIZE]);
        Some(error_locs.len())
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
}
