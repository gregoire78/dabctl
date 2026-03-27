use crate::pipeline::{
    bits_to_bytes,
    FicBlockCandidate,
    FibCandidate,
    FibExtractor,
    FigCandidate,
    FigDetails,
    FigType0Details,
    SignallingDecoder,
    SignallingSnapshot,
    Type0ExtensionSummary,
    DAB_FIB_BITS,
    DAB_FIB_BYTES,
};
use std::collections::BTreeMap;

impl Default for FibExtractor {
    fn default() -> Self {
        Self::new()
    }
}

impl FibExtractor {
    pub fn new() -> Self {
        Self
    }

    pub fn extract_fibs(&self, blocks: &[FicBlockCandidate]) -> Vec<FibCandidate> {
        blocks
            .iter()
            .filter(|block| block.crc_ok && block.bit_count == DAB_FIB_BITS)
            .map(|block| FibCandidate {
                frame_start_sample: block.frame_start_sample,
                block_index: block.block_index,
                crc_ok: block.crc_ok,
                bytes: bits_to_bytes(&block.bits),
            })
            .filter(|fib| fib.bytes.len() == DAB_FIB_BYTES)
            .collect()
    }

    pub fn extract_figs(&self, fibs: &[FibCandidate]) -> Vec<FigCandidate> {
        let mut figs = Vec::new();

        for fib in fibs {
            let data_len = fib.bytes.len().saturating_sub(2);
            let mut offset = 0usize;

            while offset < data_len {
                let header = fib.bytes[offset];
                if header == 0xFF {
                    break;
                }

                let fig_type = header >> 5;
                let payload_len = (header & 0x1F) as usize;
                if payload_len == 0 {
                    offset += 1;
                    continue;
                }

                let payload_start = offset + 1;
                let payload_end = payload_start + payload_len;
                if payload_end > data_len {
                    break;
                }

                let extension = fib.bytes.get(payload_start).copied();
                let payload = fib.bytes[payload_start..payload_end].to_vec();
                let details = if fig_type == 0 {
                    let control = payload[0];
                    let extension = control & 0x1F;
                    FigDetails::Type0(FigType0Details {
                        cn: (control & 0x80) != 0,
                        oe: (control & 0x40) != 0,
                        pd: (control & 0x20) != 0,
                        extension,
                        body: payload[1..].to_vec(),
                    })
                } else {
                    FigDetails::Raw
                };
                figs.push(FigCandidate {
                    frame_start_sample: fib.frame_start_sample,
                    block_index: fib.block_index,
                    fig_type,
                    extension,
                    payload_len,
                    payload,
                    details,
                });

                offset = payload_end;
            }
        }

        figs
    }
}

impl Default for SignallingDecoder {
    fn default() -> Self {
        Self::new()
    }
}

impl SignallingDecoder {
    pub fn new() -> Self {
        Self
    }

    pub fn decode(&self, figs: &[FigCandidate]) -> Option<SignallingSnapshot> {
        if figs.is_empty() {
            return None;
        }

        let mut type0_extensions = BTreeMap::<u8, Type0ExtensionSummary>::new();
        let mut type1_count = 0usize;

        for fig in figs {
            match &fig.details {
                FigDetails::Type0(details) => {
                    let entry = type0_extensions
                        .entry(details.extension)
                        .or_insert_with(|| Type0ExtensionSummary {
                            extension: details.extension,
                            count: 0,
                            cn: false,
                            oe: false,
                            pd: false,
                            last_body: Vec::new(),
                        });
                    entry.count += 1;
                    entry.cn |= details.cn;
                    entry.oe |= details.oe;
                    entry.pd |= details.pd;
                    entry.last_body = details.body.clone();
                }
                FigDetails::Raw => {
                    if fig.fig_type == 1 {
                        type1_count += 1;
                    }
                }
            }
        }

        Some(SignallingSnapshot {
            fig_count: figs.len(),
            type1_count,
            type0_extensions: type0_extensions.into_values().collect(),
        })
    }
}
