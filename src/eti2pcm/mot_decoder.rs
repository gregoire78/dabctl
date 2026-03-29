/// MOT DataGroup decoder for X-PAD transport.
///
/// Accumulates X-PAD data subfields (start + continuation) into a complete
/// MOT Data Group, validates CRC, and returns the raw Data Group.
///
/// Reference: ETSI EN 301 234 §5.1 (X-PAD Data Group transport)
use crate::eti2pcm::crc::crc16_ccitt;

const MOT_DG_SIZE_MAX: usize = 16384; // 2^14, max MOT Data Group size
const CRC_LEN: usize = 2;

/// MOT Data Group decoder: accumulates X-PAD subfields into a complete Data Group.
pub struct MotDecoder {
    buffer: Vec<u8>,
    size: usize,
    size_needed: usize,
    crc: crate::eti2pcm::crc::CrcCalculator,
}

impl MotDecoder {
    pub fn new() -> Self {
        MotDecoder {
            buffer: vec![0u8; MOT_DG_SIZE_MAX],
            size: 0,
            size_needed: 0,
            crc: crc16_ccitt(),
        }
    }

    pub fn reset(&mut self) {
        self.size = 0;
        self.size_needed = 0;
    }

    /// Set the expected Data Group length (from DGLI).
    /// Must be called before the start subfield of a new Data Group.
    pub fn set_len(&mut self, len: usize) {
        self.size_needed = len;
    }

    /// Process a data subfield. Returns true when a complete valid Data Group is available.
    pub fn process_subfield(&mut self, start: bool, data: &[u8]) -> bool {
        if start {
            self.size = 0;
        } else if self.size == 0 {
            // Ignore continuation without a start
            return false;
        }

        // Abort if we've already reached needed size
        if self.size_needed > 0 && self.size >= self.size_needed {
            return false;
        }

        // Append data
        let copy_len = (MOT_DG_SIZE_MAX - self.size).min(data.len());
        self.buffer[self.size..self.size + copy_len].copy_from_slice(&data[..copy_len]);
        self.size += copy_len;

        // Check if we have enough data
        if self.size_needed == 0 || self.size < self.size_needed {
            return false;
        }
        // Validation CRC extraite
        if !self.is_valid_crc() {
            self.reset();
            return false;
        }
        true
    }

    /// Get the completed Data Group bytes (including CRC).

    /// Validation CRC du Data Group courant
    fn is_valid_crc(&self) -> bool {
        if self.size_needed < CRC_LEN {
            return false;
        }
        let data_len = self.size_needed - CRC_LEN;
        let crc_stored = (self.buffer[data_len] as u16) << 8 | self.buffer[data_len + 1] as u16;
        let crc_calced = self.crc.calc(&self.buffer[..data_len]);
        crc_stored == crc_calced
    }
    pub fn get_data_group(&self) -> Vec<u8> {
        self.buffer[..self.size_needed].to_vec()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_dg_with_crc(payload: &[u8]) -> Vec<u8> {
        let crc_calc = crc16_ccitt();
        let crc = crc_calc.calc(payload);
        let mut dg = payload.to_vec();
        dg.push((crc >> 8) as u8);
        dg.push((crc & 0xFF) as u8);
        dg
    }

    #[test]
    fn test_mot_decoder_single_subfield_given_valid_crc_then_returns_true() {
        // Given
        let mut dec = MotDecoder::new();
        let payload = vec![0x01, 0x02, 0x03, 0x04];
        let dg = build_dg_with_crc(&payload);
        dec.set_len(dg.len());
        // When
        let result = dec.process_subfield(true, &dg);
        // Then
        assert!(result);
        assert_eq!(dec.get_data_group(), dg);
    }

    #[test]
    fn test_mot_decoder_single_subfield_given_invalid_crc_then_returns_false() {
        // Given
        let mut dec = MotDecoder::new();
        let mut payload = vec![0x01, 0x02, 0x03, 0x04];
        // CRC faux
        payload.push(0x00);
        payload.push(0x00);
        dec.set_len(payload.len());
        // When
        let result = dec.process_subfield(true, &payload);
        // Then
        assert!(!result);
    }

    #[test]
    fn test_is_valid_crc_given_valid_and_invalid_cases() {
        // Given
        let mut dec = MotDecoder::new();
        let payload = vec![0x10, 0x20, 0x30, 0x40];
        let dg = build_dg_with_crc(&payload);
        dec.set_len(dg.len());
        dec.process_subfield(true, &dg);
        // When/Then
        assert!(dec.is_valid_crc());

        // Cas CRC faux
        let mut dec2 = MotDecoder::new();
        let mut bad = dg.clone();
        let last = bad.len() - 1;
        bad[last] ^= 0xFF; // corrompre le CRC
        dec2.set_len(bad.len());
        dec2.process_subfield(true, &bad);
        assert!(!dec2.is_valid_crc());
    }

    #[test]
    fn test_mot_decoder_multi_subfield() {
        let mut dec = MotDecoder::new();

        let payload = vec![0x10, 0x20, 0x30, 0x40, 0x50, 0x60];
        let dg = build_dg_with_crc(&payload);

        dec.set_len(dg.len());

        // Send in two chunks
        let mid = dg.len() / 2;
        assert!(!dec.process_subfield(true, &dg[..mid]));
        assert!(dec.process_subfield(false, &dg[mid..]));
        assert_eq!(dec.get_data_group(), dg);
    }

    #[test]
    fn test_mot_decoder_bad_crc() {
        let mut dec = MotDecoder::new();

        let mut dg = build_dg_with_crc(&[0x01, 0x02]);
        let last = dg.len() - 1;
        dg[last] ^= 0xFF; // corrupt CRC

        dec.set_len(dg.len());
        let result = dec.process_subfield(true, &dg);
        assert!(!result);
    }

    #[test]
    fn test_mot_decoder_continuation_without_start() {
        let mut dec = MotDecoder::new();
        let result = dec.process_subfield(false, &[0x01, 0x02]);
        assert!(!result);
    }

    #[test]
    fn test_mot_decoder_reset() {
        let mut dec = MotDecoder::new();

        let dg = build_dg_with_crc(&[0x01]);
        dec.set_len(dg.len());
        dec.process_subfield(true, &dg);

        dec.reset();
        assert_eq!(dec.size, 0);
        assert_eq!(dec.size_needed, 0);
    }

    #[test]
    fn test_mot_decoder_start_resets_previous() {
        let mut dec = MotDecoder::new();

        let dg1 = build_dg_with_crc(&[0x01, 0x02, 0x03]);
        let dg2 = build_dg_with_crc(&[0xAA, 0xBB]);

        dec.set_len(dg1.len());
        // Start first DG but don't complete it
        dec.process_subfield(true, &dg1[..2]);

        // Start new DG
        dec.set_len(dg2.len());
        assert!(dec.process_subfield(true, &dg2));
        assert_eq!(dec.get_data_group(), dg2);
    }
}
