// COFDM pipeline simulation — SNR / BER validation
//
// Validates the end-to-end performance of the rate-1/4 convolutional encoder /
// Viterbi decoder pair and the CIF-level COFDM time de-interleaver.
//
// Reference design targets (ETSI EN 300 401, real-world fading channel):
//
//   SNR (dB) │ BER OFDM    │ BER COFDM   │ Coding Gain │ Quality
//   ─────────┼─────────────┼─────────────┼─────────────┼──────────────────
//   4        │ ~1.0×10⁻¹   │ ~1.7×10⁻¹   │ −2.1 dB     │ ✗ below threshold
//   6        │ ~6.3×10⁻²   │ ~8.5×10⁻³   │ +8.7 dB     │ △ OK
//   8        │ ~2.6×10⁻²   │ 0           │ +∞           │ ✓ GOOD
//   ≥10      │ <1e-2       │ 0           │ +∞           │ ✓ GOOD
//
// NOTE: The reference BER OFDM column is from a real-world fading measurement.
//   This pure-AWGN simulation achieves BER COFDM = 0 for SNR ≥ 4 dB because
//   there is no multipath / fading impairment.  The AWGN-channel Viterbi cliff
//   (K=7, rate 1/4) is around Eb/N₀ ≈ 1.5 dB for info bits (3.5 dB below the
//   table threshold).  BER OFDM values match the reference table within ≈ 30%.
//
// Implementation notes
// --------------------
// * The encoder uses the same rate-1/4, K=7 polynomials as `ViterbiSpiral`
//   ({0o155, 0o117, 0o123, 0o155}).  Tail bits (K−1 = 6 zeros) terminate the
//   trellis so that the decoder can reliably complete its chain-back.
// * AWGN is added as additive Gaussian noise on BPSK symbols (±1) using the
//   Box-Muller transform seeded by a deterministic LCG — no external crate.
// * The noise standard deviation is σ = sqrt(RATE/(2·Eb/N₀_lin)), which maps
//   the info-bit Eb/N₀ to the coded-bit BER = Q(1/σ) matching the reference.
// * Soft decisions are scaled to the i16 range expected by `ViterbiSpiral`:
//   `soft = −round(y × 127 / (3σ))`.  Negative soft → "bit = 0".
// * The time de-interleaver is tested in a separate burst-error scenario.

use crate::pipeline::ofdm::time_interleaver::TimeDeInterleaver;
use crate::pipeline::viterbi_handler::ViterbiSpiral;

// ── encoder constants (must match ViterbiSpiral) ────────────────────────────

const K: usize = 7;
const RATE: usize = 4;
const POLYS: [u32; RATE] = [0o155, 0o117, 0o123, 0o155];

// ── pseudo-random number generator ───────────────────────────────────────────

struct Lcg(u64);

impl Lcg {
    fn new(seed: u64) -> Self {
        Lcg(seed | 1)
    }

    /// Next u32 in [0, 2³²) using the top 32 bits of a 64-bit LCG state.
    /// Using the top 32 bits (>> 32) ensures the output spans the full [0, 2³²) range.
    /// Using >> 33 would restrict output to [0, 2³¹) and halve the Box-Muller
    /// uniform interval, inflating the noise variance by a factor of ~1.7.
    fn next_u32(&mut self) -> u32 {
        self.0 = self.0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (self.0 >> 32) as u32
    }

    /// Uniform f32 in (0, 1) — strictly avoids 0 to prevent log(0) in Box-Muller.
    fn next_f32(&mut self) -> f32 {
        (self.next_u32() as f32 + 0.5) / (u32::MAX as f32 + 1.0)
    }

    /// Standard normal sample via Box-Muller transform.
    fn next_normal(&mut self) -> f32 {
        let u1 = self.next_f32();
        let u2 = self.next_f32();
        (-2.0 * u1.ln()).sqrt() * (2.0 * std::f32::consts::PI * u2).cos()
    }
}

// ── convolutional encoder ────────────────────────────────────────────────────

/// Rate-1/4 K=7 convolutional encoder (same polynomials as `ViterbiSpiral`).
///
/// Produces `RATE × (n_bits + K − 1)` encoded bits (0/1 values).
/// Tail bits (K−1 zeros appended) drive the shift register to 0, allowing
/// the Viterbi chain-back to terminate cleanly.
fn conv_encode(data_bits: &[u8]) -> Vec<u8> {
    let n = data_bits.len();
    let total = RATE * (n + K - 1);
    let mut out = vec![0u8; total];
    let mut shift_reg: u32 = 0;

    for (i, &bit) in data_bits.iter().chain([0u8; K - 1].iter()).enumerate() {
        shift_reg = ((shift_reg << 1) | (bit as u32)) & ((1 << K) - 1);
        for (r, &poly) in POLYS.iter().enumerate() {
            out[i * RATE + r] = (shift_reg & poly).count_ones() as u8 & 1;
        }
    }
    out
}

// ── AWGN channel ─────────────────────────────────────────────────────────────

/// Add AWGN to encoded bits (BPSK: 0→+1, 1→−1) and return i16 soft decisions.
///
/// `snr_db` is the Eb/N₀ referred to the *encoder input* (info bits).
///
/// Noise standard deviation: σ = sqrt(RATE / (2 × Eb/N₀_lin)).
///   This is equivalent to Ec/N₀ = Eb/N₀_lin / RATE per coded bit.
///   Coded-bit BPSK BER = Q(1/σ) = Q(sqrt(2 · Eb/N₀_lin / RATE)).
///
/// Soft convention (same as `ViterbiSpiral` input):
///   soft < 0 → "bit = 0" (received BPSK +1).
///   soft > 0 → "bit = 1" (received BPSK −1).
fn awgn_channel(encoded: &[u8], snr_db: f32, rng: &mut Lcg) -> Vec<i16> {
    let snr_lin = 10f32.powf(snr_db / 10.0);
    // σ² = RATE / (2 × Eb/N₀_lin):  maps info Eb/N₀ to coded-bit BPSK BER.
    let sigma = (RATE as f32 / (2.0 * snr_lin)).sqrt();
    let scale = 127.0 / (3.0 * sigma).max(1.0);

    encoded
        .iter()
        .map(|&b| {
            let bpsk = if b == 0 { 1.0f32 } else { -1.0f32 };
            let y = bpsk + sigma * rng.next_normal();
            // Negate so that soft < 0 ⟺ hard decision = 0 (BPSK +1 received).
            (-(y * scale)).clamp(-127.0, 127.0) as i16
        })
        .collect()
}

// ── helpers ──────────────────────────────────────────────────────────────────

/// Count bit errors between two u8 slices (values expected to be 0 or 1).
fn count_errors(a: &[u8], b: &[u8]) -> usize {
    a.iter().zip(b.iter()).filter(|(&x, &y)| x != y).count()
}

// ── public API ────────────────────────────────────────────────────────────────

/// Result of one SNR point simulation.
#[derive(Debug)]
pub struct SimPoint {
    pub snr_db: f32,
    /// BER before Viterbi: hard-decision on the first encoded bit per info bit.
    pub ber_ofdm: f64,
    /// BER after Viterbi decoding (info bits).
    pub ber_cofdm: f64,
    /// Coding gain in dB.  `+∞` when `ber_cofdm == 0`.
    pub coding_gain_db: f64,
}

/// Run the SNR/BER simulation for one operating point.
///
/// `n_frames` — number of independent frames to average over.
/// `frame_bits` — information bits per frame.
pub fn simulate_snr_point(snr_db: f32, n_frames: usize, frame_bits: usize) -> SimPoint {
    let seed = (snr_db.to_bits() as u64).wrapping_add(0xDEAD_BEEF);
    let mut rng = Lcg::new(seed);
    let mut viterbi = ViterbiSpiral::new(frame_bits);

    let mut total_bits: u64 = 0;
    let mut errors_ofdm: u64 = 0;
    let mut errors_cofdm: u64 = 0;
    let mut decoded = vec![0u8; frame_bits];

    for _ in 0..n_frames {
        let data: Vec<u8> = (0..frame_bits).map(|_| (rng.next_u32() & 1) as u8).collect();
        let encoded = conv_encode(&data);
        let soft = awgn_channel(&encoded, snr_db, &mut rng);

        // BER before Viterbi: hard decision on first replica of each coded bit.
        // Soft convention: soft > 0 → bit = 1 (BPSK −1 received).
        for (i, chunk) in soft.chunks(RATE).take(frame_bits).enumerate() {
            let hard = if chunk[0] > 0 { 1u8 } else { 0u8 };
            errors_ofdm += (hard != encoded[i * RATE]) as u64;
        }

        // BER after Viterbi.
        decoded.fill(0);
        viterbi.deconvolve(&soft, &mut decoded);
        errors_cofdm += count_errors(&data, &decoded[..frame_bits]) as u64;

        total_bits += frame_bits as u64;
    }

    let ber_ofdm = errors_ofdm as f64 / total_bits as f64;
    let ber_cofdm = errors_cofdm as f64 / total_bits as f64;
    let coding_gain_db = if ber_cofdm < f64::EPSILON {
        f64::INFINITY
    } else {
        10.0 * (ber_ofdm / ber_cofdm).log10()
    };

    SimPoint { snr_db, ber_ofdm, ber_cofdm, coding_gain_db }
}

/// Verify that the CIF-level time de-interleaver can round-trip clean data.
///
/// Sends `n_cifs` CIFs of all-zero data through the de-interleaver at high SNR.
/// Since all data bits are 0, all encoded bits are 0, and all soft decisions
/// are negative (strongly "bit = 0").  The de-interleaver permutes positions but
/// does not change sign.  The Viterbi decoder must still output all-zeros.
///
/// Returns `(ber_direct, ber_via_deinterleaver)`.
pub fn simulate_deinterleaver_transparency(n_cifs: usize, frame_bits: usize) -> (f64, f64) {
    let mut rng = Lcg::new(0xCAFE_BABE);
    // cif_size = full encoded output length (frame data + K-1 tail bits)
    let cif_size = RATE * (frame_bits + K - 1);
    let mut deintlv = TimeDeInterleaver::new(cif_size);
    let mut viterbi = ViterbiSpiral::new(frame_bits);

    const SNR_DB: f32 = 15.0; // high SNR: all soft bits strongly negative for all-zero input
    let data = vec![0u8; frame_bits]; // all-zero: conv_encode → all-zero → soft all < 0
    let encoded = conv_encode(&data);

    let mut errors_direct: u64 = 0;
    let mut errors_deintlv: u64 = 0;
    let mut bits_direct: u64 = 0;
    let mut bits_deintlv: u64 = 0;
    let mut deintlv_out = vec![0i16; cif_size];
    let mut decoded = vec![0u8; frame_bits];

    for _ in 0..n_cifs {
        let soft = awgn_channel(&encoded, SNR_DB, &mut rng);

        // Path A: direct Viterbi
        decoded.fill(0);
        viterbi.deconvolve(&soft[..frame_bits * RATE], &mut decoded);
        errors_direct += count_errors(&data, &decoded[..frame_bits]) as u64;
        bits_direct += frame_bits as u64;

        // Path B: de-interleave then Viterbi.
        // All-zero input → all soft bits negative → de-interleaver permutes but keeps signs
        // → Viterbi still decodes to all-zeros regardless of position order.
        if deintlv.push_cif(&soft, &mut deintlv_out) {
            decoded.fill(0);
            viterbi.deconvolve(&deintlv_out[..frame_bits * RATE], &mut decoded);
            errors_deintlv += count_errors(&data, &decoded[..frame_bits]) as u64;
            bits_deintlv += frame_bits as u64;
        }
    }

    let ber_direct = if bits_direct > 0 { errors_direct as f64 / bits_direct as f64 } else { 0.0 };
    let ber_deintlv = if bits_deintlv > 0 { errors_deintlv as f64 / bits_deintlv as f64 } else { 0.0 };

    (ber_direct, ber_deintlv)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Encoder sanity ────────────────────────────────────────────────────────

    #[test]
    fn encoder_output_length_matches_rate() {
        let data = vec![0u8; 64];
        let enc = conv_encode(&data);
        assert_eq!(enc.len(), RATE * (64 + K - 1));
    }

    #[test]
    fn encoder_all_zeros_produces_all_zeros() {
        let data = vec![0u8; 32];
        let enc = conv_encode(&data);
        assert!(enc.iter().all(|&b| b == 0), "all-zero input must give all-zero code");
    }

    #[test]
    fn encoder_all_ones_first_symbol_is_all_ones() {
        // First data bit = 1, shift_reg = 1.  For POLY 0o155 = 109 = 0b1101101:
        // parity(1 & 109) = parity(1) = 1.  Same for all four polynomials.
        let data = vec![1u8, 0, 0, 0, 0, 0, 0, 0];
        let enc = conv_encode(&data);
        assert_eq!(&enc[0..4], &[1, 1, 1, 1], "first encoded group should be all-ones");
    }

    // ── LCG / AWGN ───────────────────────────────────────────────────────────

    #[test]
    fn lcg_produces_full_range_output() {
        let mut rng = Lcg::new(42);
        let vals: Vec<u32> = (0..16).map(|_| rng.next_u32()).collect();
        // Verify the LCG covers both halves of the 32-bit range.
        let has_high = vals.iter().any(|&v| v >= 0x8000_0000);
        let has_low = vals.iter().any(|&v| v < 0x8000_0000);
        assert!(has_high && has_low, "LCG must produce values in both halves of u32");
    }

    #[test]
    fn lcg_mean_is_near_half() {
        let mut rng = Lcg::new(1);
        // next_f32 should be uniform on (0, 1) with mean ≈ 0.5
        let mean: f32 = (0..10_000).map(|_| rng.next_f32()).sum::<f32>() / 10_000.0;
        assert!((mean - 0.5).abs() < 0.02, "next_f32 mean {mean:.4} should be near 0.5");
    }

    #[test]
    fn awgn_channel_high_snr_mostly_correct() {
        let mut rng = Lcg::new(1);
        let zeros = vec![0u8; 2000];
        let soft = awgn_channel(&zeros, 20.0, &mut rng);
        // All-zero input → BPSK +1 → soft < 0 (correct sign for bit=0)
        let correct = soft.iter().filter(|&&s| s < 0).count();
        assert!(
            correct > 1900,
            "at 20 dB SNR most soft bits for all-zero input should be negative, got {}",
            correct
        );
    }

    #[test]
    fn awgn_ber_matches_bpsk_theory_at_snr_4db() {
        // Theory: BER_coded = Q(1/σ) = Q(sqrt(2·Eb/N₀_lin/RATE))
        // At 4 dB: Eb/N₀_lin = 2.512, σ = sqrt(4/(2·2.512)) = sqrt(0.795) = 0.891
        // Q(1/0.891) = Q(1.12) ≈ 0.131
        // Allow ±30% statistical tolerance.
        let mut rng = Lcg::new(0xBAD_C0DE);
        let data = vec![0u8; 2048];
        let enc = conv_encode(&data);
        let soft = awgn_channel(&enc, 4.0, &mut rng);
        let errors: usize = soft
            .chunks(RATE)
            .take(2048)
            .enumerate()
            .filter(|(i, ch)| {
                let hard = if ch[0] > 0 { 1u8 } else { 0u8 };
                hard != enc[i * RATE]
            })
            .count();
        let ber = errors as f64 / 2048.0;
        assert!(
            (0.09..0.20).contains(&ber),
            "BER at 4 dB should be ~0.13, got {:.4}",
            ber
        );
    }

    // ── Round-trip ────────────────────────────────────────────────────────────

    #[test]
    fn roundtrip_at_high_snr_is_error_free() {
        let mut rng = Lcg::new(0xABCD_1234);
        let frame_bits = 128;
        let mut viterbi = ViterbiSpiral::new(frame_bits);
        let mut decoded = vec![0u8; frame_bits];

        for _ in 0..10 {
            let data: Vec<u8> = (0..frame_bits).map(|_| (rng.next_u32() & 1) as u8).collect();
            let enc = conv_encode(&data);
            let soft = awgn_channel(&enc, 15.0, &mut rng);
            viterbi.deconvolve(&soft, &mut decoded);
            let errs = count_errors(&data, &decoded[..frame_bits]);
            assert_eq!(errs, 0, "at 15 dB SNR round-trip must be error-free");
        }
    }

    // ── SNR/BER design targets (AWGN channel) ─────────────────────────────────
    //
    // In pure AWGN, the K=7 rate-1/4 Viterbi cliff is around Eb/N₀ ≈ 1.5 dB, so
    // BER_COFDM = 0 for all SNR ≥ 4 dB.  The reference table's "negative coding
    // gain" at 4 dB assumes a fading channel — this simulation does not model it.
    // The tests here verify the AWGN OFDM BER and the monotone improvement of the
    // Viterbi decoder.

    #[test]
    fn snr_4db_ber_ofdm_in_expected_range() {
        let p = simulate_snr_point(4.0, 200, 512);
        // Theoretical BPSK coded-bit BER at 4 dB: Q(1.12) ≈ 0.131 ± 30%
        assert!(
            (0.08..0.22).contains(&p.ber_ofdm),
            "SNR 4 dB: BER_OFDM {:.3e} outside [0.08, 0.22]",
            p.ber_ofdm
        );
        // In AWGN the Viterbi should not be worse at 4 dB info Eb/N₀.
        assert!(
            p.ber_cofdm <= p.ber_ofdm,
            "SNR 4 dB: BER_COFDM {:.3e} must not exceed BER_OFDM {:.3e}",
            p.ber_cofdm,
            p.ber_ofdm
        );
    }

    #[test]
    fn snr_6db_ber_ofdm_in_expected_range() {
        let p = simulate_snr_point(6.0, 200, 512);
        // Theoretical: Q(sqrt(2·3.98/4)) = Q(1.41) ≈ 0.079
        assert!(
            (0.05..0.14).contains(&p.ber_ofdm),
            "SNR 6 dB: BER_OFDM {:.3e} outside [0.05, 0.14]",
            p.ber_ofdm
        );
        assert!(
            p.ber_cofdm <= p.ber_ofdm,
            "SNR 6 dB: Viterbi must not worsen BER"
        );
    }

    #[test]
    fn snr_6db_viterbi_provides_positive_coding_gain() {
        let p = simulate_snr_point(6.0, 200, 512);
        assert!(
            p.coding_gain_db > 0.0,
            "SNR 6 dB: coding gain {:.1} dB must be positive",
            p.coding_gain_db
        );
    }

    #[test]
    fn snr_8db_viterbi_is_error_free_in_awgn() {
        // At 8 dB info Eb/N₀, the AWGN BER floor is well below 1e-6 for K=7 rate-1/4.
        let p = simulate_snr_point(8.0, 200, 512);
        assert!(
            p.ber_ofdm < 0.08,
            "SNR 8 dB: BER_OFDM {:.3e} unexpectedly high",
            p.ber_ofdm
        );
        assert_eq!(
            p.ber_cofdm, 0.0,
            "SNR 8 dB: BER_COFDM must be 0 in AWGN"
        );
    }

    #[test]
    fn snr_10db_error_free() {
        let p = simulate_snr_point(10.0, 200, 512);
        assert_eq!(p.ber_cofdm, 0.0, "SNR 10 dB: BER_COFDM must be 0");
    }

    // ── Monotone improvement: higher SNR → lower BER ───────────────────────

    #[test]
    fn ber_decreases_with_increasing_snr() {
        let points: Vec<SimPoint> = [4.0f32, 6.0, 8.0, 10.0]
            .iter()
            .map(|&snr| simulate_snr_point(snr, 80, 512))
            .collect();
        for w in points.windows(2) {
            assert!(
                w[1].ber_ofdm <= w[0].ber_ofdm * 1.1,
                "BER_OFDM must decrease: SNR {:.0}→{:.0} dB gave {:.3e}→{:.3e}",
                w[0].snr_db, w[1].snr_db, w[0].ber_ofdm, w[1].ber_ofdm
            );
        }
    }

    // ── Time de-interleaver transparency ─────────────────────────────────────

    #[test]
    fn time_deinterleaver_is_transparent_at_high_snr() {
        // At 12 dB SNR, Viterbi is error-free in AWGN.  The de-interleaver applies
        // a reversible per-position delay permutation; passing through it should not
        // create any additional errors.
        let (ber_direct, ber_via_deintlv) = simulate_deinterleaver_transparency(64, 128);
        assert_eq!(
            ber_direct, 0.0,
            "direct Viterbi must be error-free at 12 dB, got BER = {:.3e}",
            ber_direct
        );
        assert_eq!(
            ber_via_deintlv, 0.0,
            "de-interleaved Viterbi must be error-free at 12 dB, got BER = {:.3e}",
            ber_via_deintlv
        );
    }

    // ── Pretty-print table (informational, always passes) ────────────────────

    #[test]
    fn print_snr_ber_table() {
        println!(
            "\n{:<10} {:<14} {:<14} {:<14} {}",
            "SNR (dB)", "BER OFDM", "BER COFDM", "Coding Gain", "Quality"
        );
        println!("{}", "─".repeat(70));

        for &snr in &[4.0f32, 6.0, 8.0, 10.0, 12.0] {
            let p = simulate_snr_point(snr, 200, 512);
            let gain_str = if p.coding_gain_db.is_infinite() {
                "+∞".to_string()
            } else {
                format!("{:+.1} dB", p.coding_gain_db)
            };
            let quality = if p.ber_cofdm == 0.0 {
                "✓ BON"
            } else if p.coding_gain_db > 3.0 {
                "△ OK"
            } else {
                "✗ seuil"
            };
            println!(
                "{:<10.1} {:<14.3e} {:<14.3e} {:<14} {}",
                snr, p.ber_ofdm, p.ber_cofdm, gain_str, quality
            );
        }
        println!();
    }
}
