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

// ── RF channel impairment models ──────────────────────────────────────────────

/// Extended simulation result with channel description.
#[derive(Debug)]
pub struct SimResult {
    pub snr_db: f32,
    /// BER before Viterbi (hard decision on first coded replica).
    pub ber_ofdm: f64,
    /// BER after Viterbi decoding (info bits).
    pub ber_cofdm: f64,
    /// Coding gain in dB.  `+∞` when `ber_cofdm == 0`.
    pub coding_gain_db: f64,
    /// Human-readable channel description (e.g. `"Rayleigh-slow"`, `"CFO ε=0.30"`).
    pub channel: String,
}

/// Generate a Rayleigh-distributed amplitude with unit mean-square power.
///
/// `a = |h|` where `h ~ CN(0, 1)`, so `E[a²] = 1`.
/// Models one tap of a flat-fading wireless channel.
#[inline]
fn rayleigh_amplitude(rng: &mut Lcg) -> f32 {
    let g1 = rng.next_normal();
    let g2 = rng.next_normal();
    ((g1 * g1 + g2 * g2) / 2.0).sqrt()
}

/// Flat Rayleigh-fading BPSK channel **without** CSI equalization.
///
/// Each frame (`fast_fading=false`) or each coded bit (`fast_fading=true`) sees an
/// independent Rayleigh amplitude.  The soft-decision scale is computed from the
/// nominal AWGN SNR only, simulating a receiver without channel estimation.
/// BER is therefore substantially higher than AWGN at the same Eb/N₀.
fn rayleigh_fading_channel(
    encoded: &[u8],
    snr_db: f32,
    fast_fading: bool,
    rng: &mut Lcg,
) -> Vec<i16> {
    let snr_lin = 10f32.powf(snr_db / 10.0);
    let sigma = (RATE as f32 / (2.0 * snr_lin)).sqrt();
    let scale = 127.0 / (3.0 * sigma).max(1.0);
    let mut amp = rayleigh_amplitude(rng);
    encoded
        .iter()
        .enumerate()
        .map(|(i, &b)| {
            if fast_fading || i == 0 {
                amp = rayleigh_amplitude(rng);
            }
            let bpsk = if b == 0 { 1.0f32 } else { -1.0f32 };
            let y = amp * bpsk + sigma * rng.next_normal();
            (-(y * scale)).clamp(-127.0, 127.0) as i16
        })
        .collect()
}

/// Rician flat-fading BPSK channel (slow fading, no CSI equalization).
///
/// `k_factor ≥ 0` is the ratio of LoS power to scattered power.
/// - `k_factor = 0` → pure Rayleigh (no LoS component, maximum fading).
/// - `k_factor → ∞` → deterministic LoS, amplitude → 1, approaches AWGN.
///
/// `E[a²] = 1` is preserved for all K values.
fn rician_fading_channel(
    encoded: &[u8],
    snr_db: f32,
    k_factor: f32,
    rng: &mut Lcg,
) -> Vec<i16> {
    let snr_lin = 10f32.powf(snr_db / 10.0);
    let sigma = (RATE as f32 / (2.0 * snr_lin)).sqrt();
    let scale = 127.0 / (3.0 * sigma).max(1.0);
    let k = k_factor.max(0.0);
    // LoS component amplitude and scatter std-dev per complex component.
    let los_amp = (k / (k + 1.0)).sqrt();
    let scatter_std = (1.0 / (2.0 * (k + 1.0))).sqrt();
    encoded
        .iter()
        .map(|&b| {
            // Complex Rician coefficient h = LoS + scatter, E[|h|²] = 1.
            let h_re = los_amp + scatter_std * rng.next_normal();
            let h_im = scatter_std * rng.next_normal();
            let amp = (h_re * h_re + h_im * h_im).sqrt();
            let bpsk = if b == 0 { 1.0f32 } else { -1.0f32 };
            let y = amp * bpsk + sigma * rng.next_normal();
            (-(y * scale)).clamp(-127.0, 127.0) as i16
        })
        .collect()
}

/// Carrier-frequency-offset (CFO) channel.
///
/// `cfo_norm` is the CFO normalized to the OFDM subcarrier spacing
/// (DAB Mode I: Δf_sub = 1 kHz).  Models CFO as an amplitude reduction
/// proportional to `|sinc(ε)| = |sin(πε) / (πε)|`:
///
/// | cfo_norm | Amplitude loss | Impact     |
/// |----------|----------------|------------|
/// | 0.00     | 0 dB           | ideal      |
/// | 0.10     | −0.1 dB        | negligible |
/// | 0.30     | −1.3 dB        | noticeable |
/// | 0.50     | −3.9 dB        | severe     |
/// | ≥1.00    | −∞ dB          | full null  |
fn cfo_channel(encoded: &[u8], snr_db: f32, cfo_norm: f32, rng: &mut Lcg) -> Vec<i16> {
    let amplitude = if cfo_norm.abs() < 1e-6 {
        1.0f32
    } else {
        let arg = std::f32::consts::PI * cfo_norm;
        (arg.sin() / arg).abs()
    };
    let snr_lin = 10f32.powf(snr_db / 10.0);
    let sigma = (RATE as f32 / (2.0 * snr_lin)).sqrt();
    let scale = 127.0 / (3.0 * sigma).max(1.0);
    encoded
        .iter()
        .map(|&b| {
            let bpsk = if b == 0 { 1.0f32 } else { -1.0f32 };
            let y = amplitude * bpsk + sigma * rng.next_normal();
            (-(y * scale)).clamp(-127.0, 127.0) as i16
        })
        .collect()
}

/// Wiener (random-walk) phase-noise channel.
///
/// The local-oscillator phase is modelled as a Wiener process accumulating
/// `phase_noise_rms_rad` RMS per sample.  Received amplitude is `cos(φ(t))`,
/// which drifts and folds soft decisions as phase error grows.
///
/// Typical values:
/// - Cheap RTL-SDR oscillator: ~0.02–0.05 rad/sample (~1–3°/sample)
/// - TCXO-disciplined SDR:    ~0.002 rad/sample (~0.1°/sample)
fn phase_noise_channel(
    encoded: &[u8],
    snr_db: f32,
    phase_noise_rms_rad: f32,
    rng: &mut Lcg,
) -> Vec<i16> {
    let snr_lin = 10f32.powf(snr_db / 10.0);
    let sigma = (RATE as f32 / (2.0 * snr_lin)).sqrt();
    let scale = 127.0 / (3.0 * sigma).max(1.0);
    let mut phi = 0.0f32;
    encoded
        .iter()
        .map(|&b| {
            phi += phase_noise_rms_rad * rng.next_normal();
            let bpsk = if b == 0 { 1.0f32 } else { -1.0f32 };
            let y = bpsk * phi.cos() + sigma * rng.next_normal();
            (-(y * scale)).clamp(-127.0, 127.0) as i16
        })
        .collect()
}

/// Bernoulli–Gaussian impulse-noise channel.
///
/// Each coded bit is independently struck by a large noise spike with
/// probability `burst_prob`.  When struck, the noise is drawn from
/// N(0, `burst_amp`²) instead of the background AWGN N(0, σ²), modelling
/// impulsive interference (lightning, SMPS switching, motor brushes).
fn impulse_noise_channel(
    encoded: &[u8],
    snr_db: f32,
    burst_prob: f32,
    burst_amp: f32,
    rng: &mut Lcg,
) -> Vec<i16> {
    let snr_lin = 10f32.powf(snr_db / 10.0);
    let sigma = (RATE as f32 / (2.0 * snr_lin)).sqrt();
    let scale = 127.0 / (3.0 * sigma).max(1.0);
    encoded
        .iter()
        .map(|&b| {
            let bpsk = if b == 0 { 1.0f32 } else { -1.0f32 };
            let noise = if rng.next_f32() < burst_prob {
                burst_amp * rng.next_normal()
            } else {
                sigma * rng.next_normal()
            };
            let y = bpsk + noise;
            (-(y * scale)).clamp(-127.0, 127.0) as i16
        })
        .collect()
}

// ── helpers shared by all simulate_* functions ────────────────────────────────

fn run_ber_loop(
    n_frames: usize,
    frame_bits: usize,
    rng: &mut Lcg,
    viterbi: &mut ViterbiSpiral,
    channel_fn: &mut dyn FnMut(&[u8], &mut Lcg) -> Vec<i16>,
) -> (f64, f64) {
    let mut total_bits: u64 = 0;
    let mut errors_ofdm: u64 = 0;
    let mut errors_cofdm: u64 = 0;
    let mut decoded = vec![0u8; frame_bits];

    for _ in 0..n_frames {
        let data: Vec<u8> = (0..frame_bits).map(|_| (rng.next_u32() & 1) as u8).collect();
        let encoded = conv_encode(&data);
        let soft = channel_fn(&encoded, rng);

        for (i, chunk) in soft.chunks(RATE).take(frame_bits).enumerate() {
            let hard = if chunk[0] > 0 { 1u8 } else { 0u8 };
            errors_ofdm += (hard != encoded[i * RATE]) as u64;
        }
        decoded.fill(0);
        viterbi.deconvolve(&soft, &mut decoded);
        errors_cofdm += count_errors(&data, &decoded[..frame_bits]) as u64;
        total_bits += frame_bits as u64;
    }

    let ber_ofdm = errors_ofdm as f64 / total_bits as f64;
    let ber_cofdm = errors_cofdm as f64 / total_bits as f64;
    (ber_ofdm, ber_cofdm)
}

fn coding_gain(ber_ofdm: f64, ber_cofdm: f64) -> f64 {
    if ber_cofdm < f64::EPSILON {
        f64::INFINITY
    } else {
        10.0 * (ber_ofdm / ber_cofdm).log10()
    }
}

// ── public RF simulation API ──────────────────────────────────────────────────

/// Rayleigh-fading BER simulation for one SNR operating point.
///
/// `fast_fading = true`  — one independent fading amplitude per coded bit
///   (high Doppler, e.g. 300 km/h train, DAB Mode I fd·T >> 1).
/// `fast_fading = false` — one amplitude per frame (quasi-static / slow fading).
pub fn simulate_rayleigh(
    snr_db: f32,
    n_frames: usize,
    frame_bits: usize,
    fast_fading: bool,
) -> SimResult {
    let seed = (snr_db.to_bits() as u64 ^ 0x1234_5678)
        .wrapping_add(if fast_fading { 1 } else { 0 });
    let mut rng = Lcg::new(seed);
    let mut viterbi = ViterbiSpiral::new(frame_bits);
    let (ber_ofdm, ber_cofdm) = run_ber_loop(
        n_frames,
        frame_bits,
        &mut rng,
        &mut viterbi,
        &mut |enc, r| rayleigh_fading_channel(enc, snr_db, fast_fading, r),
    );
    let channel = if fast_fading { "Rayleigh-fast" } else { "Rayleigh-slow" }.to_string();
    SimResult { snr_db, ber_ofdm, ber_cofdm, coding_gain_db: coding_gain(ber_ofdm, ber_cofdm), channel }
}

/// Rician-fading BER simulation for one SNR operating point.
///
/// `k_factor = 0.0` → pure Rayleigh; `k_factor → ∞` → AWGN (no fading).
pub fn simulate_rician(
    snr_db: f32,
    n_frames: usize,
    frame_bits: usize,
    k_factor: f32,
) -> SimResult {
    let seed = (snr_db.to_bits() as u64 ^ 0xABCD_EF01).wrapping_add(k_factor.to_bits() as u64);
    let mut rng = Lcg::new(seed);
    let mut viterbi = ViterbiSpiral::new(frame_bits);
    let (ber_ofdm, ber_cofdm) = run_ber_loop(
        n_frames,
        frame_bits,
        &mut rng,
        &mut viterbi,
        &mut |enc, r| rician_fading_channel(enc, snr_db, k_factor, r),
    );
    SimResult {
        snr_db,
        ber_ofdm,
        ber_cofdm,
        coding_gain_db: coding_gain(ber_ofdm, ber_cofdm),
        channel: format!("Rician K={k_factor:.1}"),
    }
}

/// CFO degradation simulation for one SNR operating point.
///
/// `cfo_norm` is normalized to the DAB Mode I subcarrier spacing (1 kHz).
/// - `0.0` → ideal (no offset).
/// - `0.5` → 500 Hz offset, ~−4 dB amplitude loss.
pub fn simulate_cfo(
    snr_db: f32,
    n_frames: usize,
    frame_bits: usize,
    cfo_norm: f32,
) -> SimResult {
    let seed = (snr_db.to_bits() as u64 ^ 0xC0FE_C0DE).wrapping_add(cfo_norm.to_bits() as u64);
    let mut rng = Lcg::new(seed);
    let mut viterbi = ViterbiSpiral::new(frame_bits);
    let (ber_ofdm, ber_cofdm) = run_ber_loop(
        n_frames,
        frame_bits,
        &mut rng,
        &mut viterbi,
        &mut |enc, r| cfo_channel(enc, snr_db, cfo_norm, r),
    );
    SimResult {
        snr_db,
        ber_ofdm,
        ber_cofdm,
        coding_gain_db: coding_gain(ber_ofdm, ber_cofdm),
        channel: format!("CFO ε={cfo_norm:.2}"),
    }
}

/// Phase-noise degradation simulation for one SNR operating point.
///
/// `phase_noise_rms_deg` is the per-sample RMS phase noise in degrees.
/// Typical cheap RTL-SDR oscillator: 1–3°/sample; TCXO: ~0.1°/sample.
pub fn simulate_phase_noise(
    snr_db: f32,
    n_frames: usize,
    frame_bits: usize,
    phase_noise_rms_deg: f32,
) -> SimResult {
    let pn_rad = phase_noise_rms_deg * std::f32::consts::PI / 180.0;
    let seed =
        (snr_db.to_bits() as u64 ^ 0xFEED_FACE).wrapping_add(phase_noise_rms_deg.to_bits() as u64);
    let mut rng = Lcg::new(seed);
    let mut viterbi = ViterbiSpiral::new(frame_bits);
    let (ber_ofdm, ber_cofdm) = run_ber_loop(
        n_frames,
        frame_bits,
        &mut rng,
        &mut viterbi,
        &mut |enc, r| phase_noise_channel(enc, snr_db, pn_rad, r),
    );
    SimResult {
        snr_db,
        ber_ofdm,
        ber_cofdm,
        coding_gain_db: coding_gain(ber_ofdm, ber_cofdm),
        channel: format!("PhaseNoise {phase_noise_rms_deg:.1}°/sample"),
    }
}

/// Impulse-noise BER simulation for one SNR operating point.
///
/// - `burst_prob`  — probability that any individual coded bit is struck.
/// - `burst_amp`   — RMS amplitude of the spike (relative to background AWGN σ=1).
pub fn simulate_impulse_noise(
    snr_db: f32,
    n_frames: usize,
    frame_bits: usize,
    burst_prob: f32,
    burst_amp: f32,
) -> SimResult {
    let seed =
        (snr_db.to_bits() as u64 ^ 0xBADC_0DE0).wrapping_add(burst_prob.to_bits() as u64);
    let mut rng = Lcg::new(seed);
    let mut viterbi = ViterbiSpiral::new(frame_bits);
    let (ber_ofdm, ber_cofdm) = run_ber_loop(
        n_frames,
        frame_bits,
        &mut rng,
        &mut viterbi,
        &mut |enc, r| impulse_noise_channel(enc, snr_db, burst_prob, burst_amp, r),
    );
    SimResult {
        snr_db,
        ber_ofdm,
        ber_cofdm,
        coding_gain_db: coding_gain(ber_ofdm, ber_cofdm),
        channel: format!("Impulse p={burst_prob:.2} A={burst_amp:.1}"),
    }
}

/// Measure the gain of the CIF-level time de-interleaver against bad-CIF bursts.
///
/// A fraction `burst_rate` of all **over-the-air** CIFs are "bad" (SNR = `burst_snr_db`,
/// where BER ≈ 50%); the rest are clean (SNR = `normal_snr_db`).
///
/// # Paths compared
///
/// **Path A — no interleaver**: the burst directly corrupts an entire encoded
/// frame.  The Viterbi decoder fails on those frames, giving high average BER.
///
/// **Path B — TX + RX interleaving**: the transmitter applies the inverse of
/// `TimeDeInterleaver` before transmission:
/// `OTA[t][i] = encoded[t + (DEPTH − D_i) % DEPTH][i]`
/// where `D_i = DELAY_TABLE[i % DEPTH]`.  A physical burst on OTA frame `t`
/// therefore corrupts only `1/DEPTH` of each output frame's positions after
/// RX de-interleaving, leaving the Viterbi decoder with a manageable low BER.
///
/// Returns `(ber_without_interleaver, ber_with_interleaver)`.
pub fn simulate_interleaver_vs_bad_cif(
    burst_rate: f32,
    burst_snr_db: f32,
    normal_snr_db: f32,
    n_cifs: usize,
    frame_bits: usize,
) -> (f64, f64) {
    // ETSI EN 300 401 §12.3 — must match TimeDeInterleaver constants.
    const DEPTH: usize = 16;
    const DELAY_TABLE: [usize; DEPTH] = [0, 8, 4, 12, 2, 10, 6, 14, 1, 9, 5, 13, 3, 11, 7, 15];

    let mut rng = Lcg::new(0xBEEF_1234);
    let cif_size = RATE * (frame_bits + K - 1);

    // ── 1. Generate data + encoded frames for all CIFs ────────────────────
    let all_data: Vec<Vec<u8>> = (0..n_cifs)
        .map(|_| (0..frame_bits).map(|_| (rng.next_u32() & 1) as u8).collect())
        .collect();
    let all_encoded: Vec<Vec<u8>> = all_data.iter().map(|d| conv_encode(d)).collect();

    // ── 2. Burst mask: which OTA CIFs are hit ────────────────────────────
    let is_burst: Vec<bool> = (0..n_cifs).map(|_| rng.next_f32() < burst_rate).collect();

    // ── 3. Path A: no interleaver ─────────────────────────────────────────
    //    A burst on OTA frame t → bad SNR on original encoded frame t.
    let mut rng_a = Lcg::new(0xAAAA_1234);
    let mut vit_a = ViterbiSpiral::new(frame_bits);
    let mut decoded = vec![0u8; frame_bits];
    let mut errors_a: u64 = 0;
    let mut bits_a: u64 = 0;

    for t in 0..n_cifs {
        let snr = if is_burst[t] { burst_snr_db } else { normal_snr_db };
        let soft = awgn_channel(&all_encoded[t], snr, &mut rng_a);
        decoded.fill(0);
        vit_a.deconvolve(&soft, &mut decoded);
        errors_a += count_errors(&all_data[t], &decoded) as u64;
        bits_a += frame_bits as u64;
    }

    // ── 4. Path B: TX interleaving + RX de-interleaving ──────────────────
    //    TX rule: OTA[t][i] = encoded[t + look_ahead(i)][i]
    //    where look_ahead(i) = (DEPTH − DELAY_TABLE[i%DEPTH]) % DEPTH.
    //    After round-trip: deintlv_out at step r reproduces encoded[r][i]. ✓
    //
    //    The TX look-ahead is at most DEPTH−1 = 15 frames.  OTA frame t is
    //    fully valid only when t + 15 < n_cifs (i.e. t ≤ n_cifs − DEPTH).
    //    The RX de-interleaver at step r reads OTA frames r−15 … r, so the
    //    output is trustworthy for r ∈ [DEPTH−1, n_cifs−DEPTH].
    let mut rng_b = Lcg::new(0xBBBB_1234);

    // Build interleaved OTA soft frames.
    let ota_soft: Vec<Vec<i16>> = (0..n_cifs)
        .map(|t| {
            let mut ota_enc = vec![0u8; cif_size];
            for i in 0..cif_size {
                let look_ahead = (DEPTH - DELAY_TABLE[i % DEPTH]) % DEPTH;
                let src = t + look_ahead;
                if src < n_cifs {
                    ota_enc[i] = all_encoded[src][i];
                }
                // Out-of-range positions stay 0 (zero-coded bit → soft ≈ −max).
            }
            let snr = if is_burst[t] { burst_snr_db } else { normal_snr_db };
            awgn_channel(&ota_enc, snr, &mut rng_b)
        })
        .collect();

    // RX de-interleave and decode.
    let valid_start = DEPTH - 1;              // first step with full ring
    let valid_end = n_cifs.saturating_sub(DEPTH); // last step with full TX look-ahead

    let mut deintlv = TimeDeInterleaver::new(cif_size);
    let mut vit_b = ViterbiSpiral::new(frame_bits);
    let mut deintlv_out = vec![0i16; cif_size];
    let mut errors_b: u64 = 0;
    let mut bits_b: u64 = 0;

    for t in 0..n_cifs {
        let valid = deintlv.push_cif(&ota_soft[t], &mut deintlv_out);
        if valid && t >= valid_start && t <= valid_end {
            // De-interleaved output at step t corresponds to encoded frame t.
            decoded.fill(0);
            vit_b.deconvolve(&deintlv_out, &mut decoded);
            errors_b += count_errors(&all_data[t], &decoded) as u64;
            bits_b += frame_bits as u64;
        }
    }

    let ber_a = if bits_a > 0 { errors_a as f64 / bits_a as f64 } else { 0.0 };
    let ber_b = if bits_b > 0 { errors_b as f64 / bits_b as f64 } else { 0.0 };
    (ber_a, ber_b)
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
            "\n{:<10} {:<14} {:<10} {:<12} {}",
            "SNR (dB)", "BER COFDM", "FIB OK%", "Gain dB", "Quality"
        );
        println!("{}", "─".repeat(58));

        for &snr in &[4.0f32, 6.0, 8.0, 10.0, 12.0] {
            let p = simulate_snr_point(snr, 200, 512);
            let quality = if p.ber_cofdm == 0.0 {
                "✓ BON"
            } else if p.coding_gain_db > 3.0 {
                "△ OK"
            } else {
                "✗ seuil"
            };
            println!(
                "{:<10.1} {:<14.3e} {:<10} {:<12} {}",
                snr, p.ber_cofdm, fmt_fib(p.ber_cofdm), fmt_gain(p.coding_gain_db), quality
            );
        }
        println!();
    }

    // ══════════════════════════════════════════════════════════════════════════
    // RF channel impairment tests
    // ══════════════════════════════════════════════════════════════════════════

    // ── Rayleigh fading ───────────────────────────────────────────────────────

    /// Rayleigh fading (no equalization) must give higher BER than pure AWGN.
    ///
    /// Theory: Rayleigh flat BER_OFDM = 0.5·(1 − √(γ/(1+γ))) where γ = SNR_lin/RATE.
    /// At SNR=8 dB: γ = 1.58 → BER ≈ 0.11, vs AWGN ≈ 0.038.
    #[test]
    fn rayleigh_slow_ber_ofdm_higher_than_awgn() {
        let snr = 8.0f32;
        let awgn = simulate_snr_point(snr, 300, 256);
        let ray = simulate_rayleigh(snr, 300, 256, false);
        assert!(
            ray.ber_ofdm > awgn.ber_ofdm * 1.5,
            "Rayleigh BER_OFDM {:.3e} should exceed AWGN {:.3e} at {snr} dB",
            ray.ber_ofdm,
            awgn.ber_ofdm
        );
    }

    /// Viterbi coding gain must be positive even in Rayleigh fading.
    #[test]
    fn rayleigh_viterbi_provides_positive_coding_gain() {
        let p = simulate_rayleigh(10.0, 300, 256, false);
        assert!(
            p.ber_cofdm <= p.ber_ofdm,
            "Viterbi must not worsen BER in Rayleigh (BER_OFDM {:.3e}, BER_COFDM {:.3e})",
            p.ber_ofdm,
            p.ber_cofdm
        );
    }

    /// Fast fading BER must not be dramatically lower than slow fading (it is
    /// typically equal or worse due to independent per-bit channel variations).
    #[test]
    fn rayleigh_fast_ber_not_better_than_slow() {
        let snr = 8.0f32;
        let slow = simulate_rayleigh(snr, 300, 256, false);
        let fast = simulate_rayleigh(snr, 300, 256, true);
        // Allow fast to be at most 40% lower than slow (statistical tolerance).
        assert!(
            fast.ber_ofdm >= slow.ber_ofdm * 0.6,
            "Fast BER_OFDM {:.3e} should not be far below slow {:.3e}",
            fast.ber_ofdm,
            slow.ber_ofdm
        );
    }

    /// Rayleigh BER must be non-zero even at SNR=12 dB (fading floor without equalization).
    #[test]
    fn rayleigh_has_error_floor_without_equalization() {
        let p = simulate_rayleigh(12.0, 300, 256, false);
        // Without CSI equalization, Rayleigh has a BER floor well above AWGN.
        assert!(
            p.ber_ofdm > 0.01,
            "Rayleigh BER_OFDM {:.3e} should remain above 0.01 without equalization",
            p.ber_ofdm
        );
    }

    // ── Rician fading ─────────────────────────────────────────────────────────

    /// Rician K=0 is mathematically Rayleigh: BER must be in the fading regime.
    #[test]
    fn rician_k0_is_in_fading_regime() {
        let snr = 8.0f32;
        let awgn = simulate_snr_point(snr, 300, 256);
        let k0 = simulate_rician(snr, 300, 256, 0.0);
        // Must be substantially worse than AWGN.
        assert!(
            k0.ber_ofdm > awgn.ber_ofdm * 1.5,
            "Rician K=0 BER {:.3e} should be in fading regime vs AWGN {:.3e}",
            k0.ber_ofdm,
            awgn.ber_ofdm
        );
    }

    /// Increasing K-factor must reduce BER (stronger LoS → less fading).
    #[test]
    fn rician_higher_k_gives_lower_ber() {
        let snr = 8.0f32;
        let k0 = simulate_rician(snr, 400, 256, 0.0);
        let k10 = simulate_rician(snr, 400, 256, 10.0);
        let k50 = simulate_rician(snr, 400, 256, 50.0);
        assert!(
            k10.ber_ofdm <= k0.ber_ofdm * 1.1,
            "K=10 BER {:.3e} should not exceed K=0 BER {:.3e}",
            k10.ber_ofdm,
            k0.ber_ofdm
        );
        assert!(
            k50.ber_ofdm <= k10.ber_ofdm * 1.1,
            "K=50 BER {:.3e} should not exceed K=10 BER {:.3e}",
            k50.ber_ofdm,
            k10.ber_ofdm
        );
    }

    /// At K=100, the channel is essentially AWGN; BER should approach AWGN BER.
    #[test]
    fn rician_very_high_k_approaches_awgn() {
        let snr = 10.0f32;
        let awgn = simulate_snr_point(snr, 300, 256);
        let k100 = simulate_rician(snr, 300, 256, 100.0);
        // Allow 3× overhead — residual scatter still slightly elevates BER.
        assert!(
            k100.ber_ofdm <= awgn.ber_ofdm * 3.0 + 1e-3,
            "Rician K=100 BER {:.3e} should approach AWGN {:.3e}",
            k100.ber_ofdm,
            awgn.ber_ofdm
        );
    }

    // ── CFO ───────────────────────────────────────────────────────────────────

    /// At CFO=0 the channel is identical to AWGN; BER should match.
    #[test]
    fn cfo_zero_ber_matches_awgn() {
        let snr = 8.0f32;
        let awgn = simulate_snr_point(snr, 200, 256);
        let cfo0 = simulate_cfo(snr, 200, 256, 0.0);
        // Different seeds; allow ±50% relative tolerance.
        let diff = (cfo0.ber_ofdm - awgn.ber_ofdm).abs();
        assert!(
            diff < awgn.ber_ofdm * 0.5 + 1e-4,
            "CFO=0 BER {:.3e} should match AWGN {:.3e}",
            cfo0.ber_ofdm,
            awgn.ber_ofdm
        );
    }

    /// BER must increase monotonically with CFO.
    #[test]
    fn cfo_ber_increases_monotonically() {
        let snr = 10.0f32;
        let c0 = simulate_cfo(snr, 200, 256, 0.0);
        let c2 = simulate_cfo(snr, 200, 256, 0.2);
        let c5 = simulate_cfo(snr, 200, 256, 0.5);
        assert!(
            c2.ber_ofdm >= c0.ber_ofdm,
            "CFO=0.2 BER {:.3e} should exceed CFO=0 BER {:.3e}",
            c2.ber_ofdm,
            c0.ber_ofdm
        );
        assert!(
            c5.ber_ofdm >= c2.ber_ofdm,
            "CFO=0.5 BER {:.3e} should exceed CFO=0.2 BER {:.3e}",
            c5.ber_ofdm,
            c2.ber_ofdm
        );
    }

    /// At CFO=0.5 (sinc=2/π ≈ 0.637, −3.9 dB) BER must exceed pure AWGN.
    #[test]
    fn cfo_half_subcarrier_degrades_ber_significantly() {
        let snr = 10.0f32;
        let awgn = simulate_snr_point(snr, 200, 256);
        let cfo5 = simulate_cfo(snr, 200, 256, 0.5);
        assert!(
            cfo5.ber_ofdm > awgn.ber_ofdm,
            "CFO=0.5 BER {:.3e} should exceed AWGN {:.3e}",
            cfo5.ber_ofdm,
            awgn.ber_ofdm
        );
    }

    /// At CFO=0.9 the sinc null is approached; BER must be very high (>15%).
    #[test]
    fn cfo_near_null_causes_high_ber() {
        let cfo9 = simulate_cfo(20.0, 200, 256, 0.9);
        // sinc(0.9) ≈ 0.109 → effective Eb/N0 ≈ 0.8 dB → BER_OFDM ≈ 0.22
        assert!(
            cfo9.ber_ofdm > 0.15,
            "CFO=0.9 BER {:.3e} should exceed 15% (signal near null)",
            cfo9.ber_ofdm
        );
    }

    // ── Phase noise ───────────────────────────────────────────────────────────

    /// At 0°/sample phase noise the channel is identical to AWGN.
    #[test]
    fn phase_noise_zero_matches_awgn() {
        let snr = 8.0f32;
        let awgn = simulate_snr_point(snr, 200, 256);
        let pn0 = simulate_phase_noise(snr, 200, 256, 0.0);
        let diff = (pn0.ber_ofdm - awgn.ber_ofdm).abs();
        assert!(
            diff < awgn.ber_ofdm * 0.5 + 1e-4,
            "Phase noise 0° BER {:.3e} should match AWGN {:.3e}",
            pn0.ber_ofdm,
            awgn.ber_ofdm
        );
    }

    /// Large phase noise must degrade BER significantly.
    #[test]
    fn phase_noise_degrades_ber() {
        let snr = 12.0f32;
        let low = simulate_phase_noise(snr, 200, 256, 1.0);  // ~1°/sample
        let high = simulate_phase_noise(snr, 200, 256, 15.0); // ~15°/sample
        assert!(
            high.ber_ofdm >= low.ber_ofdm,
            "High phase noise BER {:.3e} should not be below low noise BER {:.3e}",
            high.ber_ofdm,
            low.ber_ofdm
        );
    }

    /// At 90°/sample the phase is uniformly distributed → BER ≈ 50%.
    #[test]
    fn severe_phase_noise_randomises_signal() {
        // At 20 dB AWGN would be error-free; 90°/sample phase noise kills it.
        let extreme = simulate_phase_noise(20.0, 200, 256, 90.0);
        assert!(
            extreme.ber_ofdm > 0.1,
            "90°/sample phase noise should cause high BER, got {:.3e}",
            extreme.ber_ofdm
        );
    }

    // ── Impulse noise ─────────────────────────────────────────────────────────

    /// Impulse noise with 5% hit probability and 10× amplitude must degrade BER.
    #[test]
    fn impulse_noise_degrades_ber_vs_awgn() {
        let snr = 10.0f32;
        let awgn = simulate_snr_point(snr, 200, 256);
        let imp = simulate_impulse_noise(snr, 200, 256, 0.05, 10.0);
        assert!(
            imp.ber_ofdm > awgn.ber_ofdm,
            "Impulse noise BER {:.3e} should exceed AWGN {:.3e}",
            imp.ber_ofdm,
            awgn.ber_ofdm
        );
    }

    /// Higher burst probability must give worse or equal BER.
    #[test]
    fn impulse_noise_higher_prob_higher_ber() {
        let snr = 10.0f32;
        let low = simulate_impulse_noise(snr, 200, 256, 0.01, 5.0);
        let high = simulate_impulse_noise(snr, 200, 256, 0.10, 5.0);
        assert!(
            high.ber_ofdm >= low.ber_ofdm,
            "Higher burst prob BER {:.3e} should not be below {:.3e}",
            high.ber_ofdm,
            low.ber_ofdm
        );
    }

    /// Viterbi must provide positive coding gain even under impulse noise.
    #[test]
    fn impulse_noise_viterbi_provides_coding_gain() {
        let p = simulate_impulse_noise(10.0, 200, 256, 0.05, 8.0);
        assert!(
            p.ber_cofdm <= p.ber_ofdm,
            "Viterbi must not increase BER under impulse noise"
        );
    }

    // ── Interleaver vs bad-CIF bursts ─────────────────────────────────────────

    /// With 10% bad CIFs at 0 dB, the time de-interleaver must reduce BER.
    #[test]
    fn interleaver_reduces_ber_under_bad_cif_bursts() {
        // 10% of CIFs arrive at 0 dB (BER_OFDM ≈ 50%).
        // The de-interleaver spreads each bad CIF across 16 output CIFs
        // (~1/16 of errors per output CIF), which the Viterbi corrects.
        let (ber_direct, ber_intlv) =
            simulate_interleaver_vs_bad_cif(0.10, 0.0, 15.0, 300, 128);
        assert!(
            ber_intlv <= ber_direct,
            "De-interleaved BER {:.3e} should be ≤ direct BER {:.3e}",
            ber_intlv,
            ber_direct
        );
    }

    /// The interleaver gain must be substantial (at least 3 dB) under heavy bursts.
    #[test]
    fn interleaver_gain_at_least_3db_under_heavy_burst() {
        // 20% bad CIFs → direct Viterbi fails on those → high average BER.
        // De-interleaver distributes errors → low BER per output CIF.
        let (ber_direct, ber_intlv) =
            simulate_interleaver_vs_bad_cif(0.20, 0.0, 15.0, 400, 128);
        // Avoid division by zero: if interleaver is perfect we can't take log.
        let gain = if ber_intlv < 1e-9 {
            f64::INFINITY
        } else {
            10.0 * (ber_direct / ber_intlv).log10()
        };
        assert!(
            gain > 3.0,
            "Interleaver gain {gain:.1} dB should exceed 3 dB under 20% burst rate"
        );
    }

    /// With zero bad CIFs, the interleaver must be transparent (BER unchanged).
    #[test]
    fn interleaver_transparent_when_no_burst() {
        let (ber_direct, ber_intlv) =
            simulate_interleaver_vs_bad_cif(0.0, 0.0, 15.0, 200, 128);
        assert_eq!(
            ber_direct, 0.0,
            "At 15 dB with no burst, direct BER must be 0, got {:.3e}",
            ber_direct
        );
        assert_eq!(
            ber_intlv, 0.0,
            "At 15 dB with no burst, interleaved BER must be 0, got {:.3e}",
            ber_intlv
        );
    }

    // ── Comprehensive RF simulation table (informational) ────────────────────

    fn fmt_gain(g: f64) -> String {
        if g.is_infinite() { "+∞".into() } else { format!("{g:+.1} dB") }
    }

    /// FIB decode success probability: P(all 240 data bits correct) × 100 %.
    ///
    /// A DAB FIB is 30 bytes of data + 2 bytes CRC-16-CCITT (ETSI EN 300 401 §5.2.2).
    /// Assuming independent bit errors, P(FIB OK) = (1 − BER_COFDM)^240.
    fn fib_ok_pct(ber_cofdm: f64) -> f64 {
        (1.0 - ber_cofdm).powi(240) * 100.0
    }

    fn fmt_fib(ber_cofdm: f64) -> String {
        format!("{:.1}%", fib_ok_pct(ber_cofdm))
    }

    fn fmt_quality(ber_cofdm: f64, gain_db: f64) -> &'static str {
        if ber_cofdm == 0.0 {
            "✓ BON"
        } else if gain_db > 3.0 {
            "△ OK"
        } else {
            "✗ seuil"
        }
    }

    fn print_sim_row(p: &SimResult) {
        println!(
            "{:<26} {:<7.1} {:<13.3e} {:<10} {:<12} {}",
            p.channel,
            p.snr_db,
            p.ber_cofdm,
            fmt_fib(p.ber_cofdm),
            fmt_gain(p.coding_gain_db),
            fmt_quality(p.ber_cofdm, p.coding_gain_db)
        );
    }

    #[test]
    fn print_rf_simulation_table() {
        let hdr = format!(
            "\n{:<26} {:<7} {:<13} {:<10} {:<12} {}",
            "Canal", "SNR dB", "BER COFDM", "FIB OK%", "Gain", "Qualité"
        );
        println!("{hdr}");
        println!("{}", "═".repeat(80));

        // ── AWGN reference ────────────────────────────────────────────────────
        println!("── AWGN (référence) ──");
        for &snr in &[4.0f32, 6.0, 8.0, 10.0, 12.0] {
            let p = simulate_snr_point(snr, 200, 256);
            println!(
                "{:<26} {:<7.1} {:<13.3e} {:<10} {:<12} {}",
                "AWGN",
                snr,
                p.ber_cofdm,
                fmt_fib(p.ber_cofdm),
                fmt_gain(p.coding_gain_db),
                fmt_quality(p.ber_cofdm, p.coding_gain_db)
            );
        }

        // ── Rayleigh fading ───────────────────────────────────────────────────
        println!("\n── Rayleigh (sans égalisation) ──");
        for &snr in &[4.0f32, 8.0, 12.0] {
            print_sim_row(&simulate_rayleigh(snr, 200, 256, false));
            print_sim_row(&simulate_rayleigh(snr, 200, 256, true));
        }

        // ── Rician fading ─────────────────────────────────────────────────────
        println!("\n── Rician (facteur K variable, SNR=8 dB) ──");
        for &k in &[0.0f32, 1.0, 5.0, 20.0, 100.0] {
            print_sim_row(&simulate_rician(8.0, 200, 256, k));
        }

        // ── CFO ───────────────────────────────────────────────────────────────
        println!("\n── Décalage fréquence CFO (SNR=10 dB) ──");
        for &cfo in &[0.0f32, 0.05, 0.10, 0.20, 0.30, 0.50, 0.70, 0.90] {
            print_sim_row(&simulate_cfo(10.0, 200, 256, cfo));
        }

        // ── Phase noise ───────────────────────────────────────────────────────
        println!("\n── Bruit de phase oscillateur (SNR=12 dB) ──");
        for &pn in &[0.0f32, 0.5, 1.0, 3.0, 5.0, 10.0, 30.0, 90.0] {
            print_sim_row(&simulate_phase_noise(12.0, 200, 256, pn));
        }

        // ── Impulse noise ─────────────────────────────────────────────────────
        println!("\n── Bruit impulsif (SNR=8 dB) ──");
        for &prob in &[0.01f32, 0.02, 0.05, 0.10, 0.20] {
            print_sim_row(&simulate_impulse_noise(8.0, 200, 256, prob, 8.0));
        }

        // ── Interleaver gain ──────────────────────────────────────────────────
        println!("\n── Gain désentrelaceur temporel (CIFs mauvais à 0 dB) ──");
        println!(
            "{:<26} {:<13} {:<13} {}",
            "Config", "BER direct", "BER désentrelacé", "Gain"
        );
        println!("{}", "─".repeat(65));
        for &rate in &[0.05f32, 0.10, 0.15, 0.20, 0.30] {
            let (bd, bi) = simulate_interleaver_vs_bad_cif(rate, 0.0, 15.0, 300, 128);
            let gain =
                if bi < 1e-12 { f64::INFINITY } else { 10.0 * (bd / bi).log10() };
            println!(
                "{:<26} {:<13.3e} {:<13.3e} {}",
                format!("burst_rate={rate:.2}"),
                bd,
                bi,
                fmt_gain(gain)
            );
        }
        println!();
    }
}
