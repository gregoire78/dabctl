/// Décodeur de Viterbi pour le canal FIC du DAB
///
/// Code convolutionnel DAB (ETSI EN 300 401 §11.1) :
///   - Taux mère : 1/4
///   - Longueur de contrainte : K = 7 (mémoire de 6 bits)
///   - Polynômes générateurs (octal) : G1=133, G2=171, G3=145, G4=133
///   - En binaire : G1=0x5B, G2=0x79, G3=0x65, G4=0x5B
const K: usize = 7;
const MEMORY: usize = K - 1; // = 6
const NUM_STATES: usize = 1 << MEMORY; // = 64

const G: [u8; 4] = [0x5B, 0x79, 0x65, 0x5B];

const P_CODE_8: [u8; 32] = [
    1, 1, 0, 0, 1, 1, 0, 0, 1, 1, 0, 0, 1, 1, 0, 0,
    1, 1, 0, 0, 1, 1, 0, 0, 1, 1, 0, 0, 1, 1, 0, 0,
];

const P_CODE_15: [u8; 32] = [
    1, 1, 1, 0, 1, 1, 1, 0, 1, 1, 1, 0, 1, 1, 1, 0,
    1, 1, 1, 0, 1, 1, 1, 0, 1, 1, 1, 0, 1, 1, 0, 0,
];

const P_CODE_16: [u8; 32] = [
    1, 1, 1, 0, 1, 1, 1, 0, 1, 1, 1, 0, 1, 1, 1, 0,
    1, 1, 1, 0, 1, 1, 1, 0, 1, 1, 1, 0, 1, 1, 1, 0,
];

fn build_fic_mode_i_puncture_mask() -> [bool; 3072 + 24] {
    let mut mask = [false; 3072 + 24];
    let mut index = 0usize;

    for _ in 0..21 {
        for k in 0..(32 * 4) {
            if P_CODE_16[k % 32] != 0 {
                mask[index] = true;
            }
            index += 1;
        }
    }

    for _ in 0..3 {
        for k in 0..(32 * 4) {
            if P_CODE_15[k % 32] != 0 {
                mask[index] = true;
            }
            index += 1;
        }
    }

    for &bit in P_CODE_8.iter().take(24) {
        if bit != 0 {
            mask[index] = true;
        }
        index += 1;
    }

    mask
}

/// Symbole effacé : distance de Hamming nulle sur cette position.
pub const ERASED: u8 = 2;

fn encode_bits(state: u8, input_bit: u8) -> [u8; 4] {
    let reg = (input_bit << MEMORY) | (state & 0x3F);
    [
        (reg & G[0]).count_ones() as u8 & 1,
        (reg & G[1]).count_ones() as u8 & 1,
        (reg & G[2]).count_ones() as u8 & 1,
        (reg & G[3]).count_ones() as u8 & 1,
    ]
}

struct Trellis {
    next_state: [[u8; 2]; NUM_STATES],
    output: [[[u8; 4]; 2]; NUM_STATES],
}

impl Trellis {
    fn build() -> Self {
        let mut t = Trellis {
            next_state: [[0u8; 2]; NUM_STATES],
            output: [[[0u8; 4]; 2]; NUM_STATES],
        };
        for state in 0..NUM_STATES {
            for input_bit in 0u8..2 {
                t.next_state[state][input_bit as usize] =
                    ((state >> 1) | ((input_bit as usize) << (MEMORY - 1))) as u8;
                t.output[state][input_bit as usize] = encode_bits(state as u8, input_bit);
            }
        }
        t
    }
}

#[inline]
fn branch_metric(received: &[u8], encoded: &[u8; 4]) -> u32 {
    let mut dist = 0u32;
    for (i, &enc) in encoded.iter().enumerate() {
        let rx = received[i];
        if rx != ERASED {
            dist += (rx ^ enc) as u32;
        }
    }
    dist
}

#[inline]
fn branch_metric_soft(received: &[i16], encoded: &[u8; 4]) -> i32 {
    let mut score = 0i32;
    for (index, &enc) in encoded.iter().enumerate() {
        let soft = received[index] as i32;
        score += if enc == 0 { soft } else { -soft };
    }
    score
}

/// Décode une séquence de bits groupés par 4 (G1 G2 G3 G4) vers les bits informationnels.
///
/// Les positions contenant `ERASED` sont traitées comme des effacements.
/// Longueur de `received` doit être un multiple de 4.
pub fn viterbi_decode(received: &[u8]) -> Vec<u8> {
    assert!(received.len().is_multiple_of(4), "longueur doit être multiple de 4");
    let num_steps = received.len() / 4;

    const INF: u32 = u32::MAX / 2;
    let mut path_metric = [INF; NUM_STATES];
    path_metric[0] = 0;

    // traceback[pas][état_suivant] = état_précédent survivant.
    // On stocke l'état précédent pour lever l'ambiguïté : deux états précédents
    // distincts peuvent mener au même état suivant avec le même bit d'entrée.
    let mut traceback: Vec<[u8; NUM_STATES]> = vec![[0u8; NUM_STATES]; num_steps];

    let trellis = Trellis::build();

    for step in 0..num_steps {
        let rx = &received[step * 4..step * 4 + 4];
        let mut new_metric = [INF; NUM_STATES];
        for state in 0..NUM_STATES {
            if path_metric[state] == INF { continue; }
            for input_bit in 0u8..2 {
                let next = trellis.next_state[state][input_bit as usize] as usize;
                let enc = &trellis.output[state][input_bit as usize];
                let metric = path_metric[state] + branch_metric(rx, enc);
                if metric < new_metric[next] {
                    new_metric[next] = metric;
                    traceback[step][next] = state as u8;
                }
            }
        }
        path_metric = new_metric;
    }

    let best_state = path_metric
        .iter()
        .enumerate()
        .min_by_key(|&(_, &m)| m)
        .map(|(s, _)| s)
        .unwrap_or(0);

    // Le bit d'entrée est le MSB de l'état atteint :
    //   next = (prev >> 1) | (bit << (MEMORY-1))  =>  bit = (next >> (MEMORY-1)) & 1
    let mut decoded = vec![0u8; num_steps];
    let mut state = best_state;
    for step in (0..num_steps).rev() {
        decoded[step] = ((state >> (MEMORY - 1)) & 1) as u8;
        state = traceback[step][state] as usize;
    }
    decoded
}

/// Décodeur Viterbi soft-decision.
///
/// `received` contient des métriques signées groupées par 4 (G1 G2 G3 G4) :
/// - valeur > 0 : bit 0 plus probable
/// - valeur < 0 : bit 1 plus probable
/// - amplitude : confiance
pub fn viterbi_decode_soft(received: &[i16]) -> Vec<u8> {
    assert!(received.len().is_multiple_of(4), "longueur doit être multiple de 4");
    let num_steps = received.len() / 4;

    const NEG_INF: i32 = i32::MIN / 4;
    let mut path_metric = [NEG_INF; NUM_STATES];
    path_metric[0] = 0;
    let mut traceback: Vec<[u8; NUM_STATES]> = vec![[0u8; NUM_STATES]; num_steps];
    let trellis = Trellis::build();

    for step in 0..num_steps {
        let rx = &received[step * 4..step * 4 + 4];
        let mut new_metric = [NEG_INF; NUM_STATES];

        for state in 0..NUM_STATES {
            if path_metric[state] == NEG_INF {
                continue;
            }
            for input_bit in 0u8..2 {
                let next = trellis.next_state[state][input_bit as usize] as usize;
                let enc = &trellis.output[state][input_bit as usize];
                let metric = path_metric[state] + branch_metric_soft(rx, enc);
                if metric > new_metric[next] {
                    new_metric[next] = metric;
                    traceback[step][next] = state as u8;
                }
            }
        }

        path_metric = new_metric;
    }

    let best_state = path_metric
        .iter()
        .enumerate()
        .max_by_key(|&(_, &m)| m)
        .map(|(state, _)| state)
        .unwrap_or(0);

    let mut decoded = vec![0u8; num_steps];
    let mut state = best_state;
    for step in (0..num_steps).rev() {
        decoded[step] = ((state >> (MEMORY - 1)) & 1) as u8;
        state = traceback[step][state] as usize;
    }

    decoded
}

/// Dépuncturing FIC Mode I (ETSI EN 300 401 §11.2).
///
/// En Mode I, le FIC est transmis au taux plein 1/4 sans perforation.
/// Cette fonction vérifie l'alignement modulo-4 et passe les bits tels quels.
pub fn fic_depuncture_mode_i(bits: &[u8]) -> Vec<u8> {
    if bits.len() != 2304 {
        if !bits.len().is_multiple_of(4) {
            return bits[..bits.len() & !3].to_vec();
        }
        return bits.to_vec();
    }

    let mask = build_fic_mode_i_puncture_mask();
    let mut out = vec![ERASED; 3072 + 24];
    let mut in_index = 0usize;
    for (i, used) in mask.iter().enumerate() {
        if *used {
            out[i] = bits[in_index];
            in_index += 1;
        }
    }
    out
}

pub fn fic_depuncture_mode_i_soft(bits: &[i16]) -> Vec<i16> {
    if bits.len() != 2304 {
        if !bits.len().is_multiple_of(4) {
            return bits[..bits.len() & !3].to_vec();
        }
        return bits.to_vec();
    }

    let mask = build_fic_mode_i_puncture_mask();
    let mut out = vec![0i16; 3072 + 24];
    let mut in_index = 0usize;
    for (i, used) in mask.iter().enumerate() {
        if *used {
            out[i] = bits[in_index];
            in_index += 1;
        }
    }
    out
}

/// Désentrelacement fréquentiel FIC selon ETSI EN 300 401 §14.6.1 Mode I.
///
/// Permutation des 1536 porteuses par symbole :
///   Π(0) = 0,  Π(k+1) = (13 × Π(k) + 511) mod 1536
///
/// Les bits QPSK (2 par porteuse) suivent la permutation de leur porteuse.
pub fn fic_deinterleave_mode_i(bits: &[u8]) -> Vec<u8> {
    const N_CARRIERS: usize = 1536;
    const N_SYMBOLS: usize = 3;
    const N_BITS_PER_CARRIER: usize = 2;

    let mut perm = [0u16; N_CARRIERS];
    perm[0] = 0;
    for k in 0..N_CARRIERS - 1 {
        perm[k + 1] = ((13 * perm[k] as u32 + 511) % N_CARRIERS as u32) as u16;
    }

    let expected = N_SYMBOLS * N_CARRIERS * N_BITS_PER_CARRIER;
    if bits.len() < expected {
        return bits.to_vec();
    }

    let mut out = vec![0u8; expected];
    for s in 0..N_SYMBOLS {
        for c in 0..N_CARRIERS {
            // Π décrit l'interleaving côté émission.
            // Pour le désentrelacement réception, il faut appliquer Π^{-1} :
            // out[c] = in[Π(c)].
            let src_c = perm[c] as usize;
            let dest_base = s * N_CARRIERS * N_BITS_PER_CARRIER + c * N_BITS_PER_CARRIER;
            let src_base = s * N_CARRIERS * N_BITS_PER_CARRIER + src_c * N_BITS_PER_CARRIER;
            out[dest_base] = bits[src_base];
            out[dest_base + 1] = bits[src_base + 1];
        }
    }
    out
}

pub fn fic_deinterleave_mode_i_soft(bits: &[i16]) -> Vec<i16> {
    const N_CARRIERS: usize = 1536;
    const N_SYMBOLS: usize = 3;
    const N_BITS_PER_CARRIER: usize = 2;

    let mut perm = [0u16; N_CARRIERS];
    perm[0] = 0;
    for k in 0..N_CARRIERS - 1 {
        perm[k + 1] = ((13 * perm[k] as u32 + 511) % N_CARRIERS as u32) as u16;
    }

    let expected = N_SYMBOLS * N_CARRIERS * N_BITS_PER_CARRIER;
    if bits.len() < expected {
        return bits.to_vec();
    }

    let mut out = vec![0i16; expected];
    for s in 0..N_SYMBOLS {
        for c in 0..N_CARRIERS {
            let src_c = perm[c] as usize;
            let dest_base = s * N_CARRIERS * N_BITS_PER_CARRIER + c * N_BITS_PER_CARRIER;
            let src_base = s * N_CARRIERS * N_BITS_PER_CARRIER + src_c * N_BITS_PER_CARRIER;
            out[dest_base] = bits[src_base];
            out[dest_base + 1] = bits[src_base + 1];
        }
    }
    out
}

