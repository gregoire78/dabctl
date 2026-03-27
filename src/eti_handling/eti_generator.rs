use crate::pipeline::{bits_to_bytes, qpsk_hard_demapp, DabNormalizedFrame};

/// Build a best-effort MSC payload from the latest normalized DAB frame.
///
/// This is not yet a full subchannel protection decode chain like eti-cmdline,
/// but it provides deterministic non-zero MSC bytes for ETI MST filling.
pub fn extract_msc_payload_from_normalized_frame(frame: &DabNormalizedFrame) -> Vec<u8> {
    let mut bits = Vec::with_capacity(frame.msc_symbols.len() * frame.phase_reference.carriers.len() * 2);

    for symbol in &frame.msc_symbols {
        for carrier in &symbol.carriers {
            let (bit_i, bit_q) = qpsk_hard_demapp(*carrier);
            bits.push(bit_i);
            bits.push(bit_q);
        }
    }

    bits_to_bytes(&bits)
}
