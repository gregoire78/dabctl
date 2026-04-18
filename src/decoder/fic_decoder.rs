const FIB_BITS: usize = 256;
const FIB_BYTES: usize = 32;

// Literal FIC/FIB staging: accumulate hard-decision bits into CRC-checked FIB units.
#[derive(Default)]
pub struct FicDecoder {
    bit_buffer: Vec<i8>,
    valid_fibs: usize,
    total_fibs: usize,
}

impl FicDecoder {
    pub fn push_soft_bits(&mut self, soft_bits: &[i8]) -> Vec<[u8; FIB_BYTES]> {
        self.bit_buffer.extend_from_slice(soft_bits);
        let mut fibs = Vec::new();

        while self.bit_buffer.len() >= FIB_BITS {
            let chunk = self.bit_buffer.drain(..FIB_BITS).collect::<Vec<_>>();
            let fib = bits_to_fib(&chunk);
            self.total_fibs += 1;
            if crc16_ccitt(&fib) == 0 {
                self.valid_fibs += 1;
                fibs.push(fib);
            }
        }

        fibs
    }

    pub fn decode_ratio_percent(&self) -> usize {
        if self.total_fibs == 0 {
            0
        } else {
            100 * self.valid_fibs / self.total_fibs
        }
    }
}

fn bits_to_fib(bits: &[i8]) -> [u8; FIB_BYTES] {
    let mut fib = [0u8; FIB_BYTES];
    for (byte_idx, byte) in fib.iter_mut().enumerate() {
        let mut value = 0u8;
        for bit_idx in 0..8 {
            let bit = if bits[byte_idx * 8 + bit_idx] > 0 {
                1
            } else {
                0
            };
            value = (value << 1) | bit;
        }
        *byte = value;
    }
    fib
}

pub fn crc16_ccitt(data: &[u8]) -> u16 {
    let mut crc = 0xFFFFu16;
    for byte in data {
        crc ^= u16::from(*byte) << 8;
        for _ in 0..8 {
            if (crc & 0x8000) != 0 {
                crc = (crc << 1) ^ 0x1021;
            } else {
                crc <<= 1;
            }
        }
    }
    crc
}

#[cfg(test)]
mod tests {
    use super::{crc16_ccitt, FicDecoder};

    #[test]
    fn accepts_valid_crc_fib() {
        let mut fib = [0u8; 32];
        let crc = crc16_ccitt(&fib[..30]);
        fib[30] = (crc >> 8) as u8;
        fib[31] = (crc & 0xFF) as u8;

        let bits = fib
            .iter()
            .flat_map(|byte| {
                (0..8)
                    .rev()
                    .map(move |shift| if ((byte >> shift) & 1) != 0 { 1 } else { -1 })
            })
            .collect::<Vec<_>>();

        let mut decoder = FicDecoder::default();
        let out = decoder.push_soft_bits(&bits);
        assert_eq!(out.len(), 1);
        assert_eq!(decoder.decode_ratio_percent(), 100);
    }
}
