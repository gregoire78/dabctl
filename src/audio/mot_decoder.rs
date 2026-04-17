/// MOT DataGroup decoder for X-PAD transport.
///
/// Accumulates X-PAD data subfields (start + continuation) into a complete
/// MOT Data Group, validates CRC, and returns the raw Data Group.
///
/// Reference: ETSI EN 301 234 §5.1 (X-PAD Data Group transport)
use crate::audio::crc::crc16_ccitt;

const MOT_DG_SIZE_MAX: usize = 16384; // 2^14, max MOT Data Group size
const CRC_LEN: usize = 2;

/// MOT Data Group decoder: accumulates X-PAD subfields into a complete Data Group.
pub struct MotDecoder {
    buffer: Vec<u8>,
    size: usize,
    size_needed: usize,
    crc: crate::audio::crc::CrcCalculator,
}

impl Default for MotDecoder {
    fn default() -> Self {
        Self::new()
    }
}

impl MotDecoder {
    fn has_pending_length(&self) -> bool {
        self.size_needed >= CRC_LEN
    }

    fn begin_subfield(&mut self, start: bool) -> bool {
        if start {
            self.size = 0;
            if !self.has_pending_length() {
                tracing::trace!("Ignoring MOT start subfield without a valid pending DGLI");
                return false;
            }
            return true;
        }

        self.size != 0
    }

    fn append_subfield_data(&mut self, data: &[u8]) {
        // ETSI EN 301 234 §5.1: DGLI announces the exact Data Group size.
        // The final X-PAD subfield may still contain trailing pad bytes, which
        // must not be accumulated into the MOT group or the CRC check will be
        // evaluated on an overrun buffer.
        let remaining = if self.has_pending_length() {
            self.size_needed.saturating_sub(self.size)
        } else {
            MOT_DG_SIZE_MAX - self.size
        };
        let copy_len = remaining.min(MOT_DG_SIZE_MAX - self.size).min(data.len());
        self.buffer[self.size..self.size + copy_len].copy_from_slice(&data[..copy_len]);
        self.size += copy_len;

        if copy_len < data.len() {
            tracing::trace!(
                "MOT DG subfield trimmed: kept={} dropped={} target_size={}",
                copy_len,
                data.len() - copy_len,
                self.size_needed
            );
        }
    }

    fn is_complete_group_ready(&self) -> bool {
        self.has_pending_length() && self.size == self.size_needed && self.is_valid_crc()
    }

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
        // ETSI EN 301 234 §5.1: the X-PAD Data Group Length Indicator (DGLI)
        // announces the exact size of the following MOT Data Group.
        // Missing or malformed DGLI must not leave stale partial bytes in the
        // reassembler, otherwise later valid slideshow objects can be poisoned.
        if (CRC_LEN..=MOT_DG_SIZE_MAX).contains(&len) {
            self.size = 0;
            self.size_needed = len;
        } else {
            tracing::debug!("Ignoring MOT Data Group with invalid DGLI length: {}", len);
            self.reset();
        }
    }

    /// Process a data subfield. Returns true when a complete valid Data Group is available.
    pub fn process_subfield(&mut self, start: bool, data: &[u8]) -> bool {
        if !self.begin_subfield(start) {
            return false;
        }

        if self.size_needed > 0 && self.size >= self.size_needed {
            return false;
        }

        self.append_subfield_data(data);

        if self.size_needed == 0 || self.size < self.size_needed {
            if self.size_needed > 0 && self.size % 200 < data.len() {
                tracing::trace!(
                    "MOT DG accumulating: {}/{} bytes",
                    self.size,
                    self.size_needed
                );
            }
            return false;
        }

        if !self.is_complete_group_ready() {
            tracing::trace!("MOT DG CRC INVALID (size={})", self.size_needed);
            self.reset();
            return false;
        }

        tracing::trace!("MOT DG CRC OK (size={})", self.size_needed);
        true
    }

    /// Get the completed Data Group bytes (including CRC).
    ///
    /// Returns true when the CRC of the accumulated Data Group is valid.
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
        if !self.is_complete_group_ready() {
            return Vec::new();
        }
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

        // invalid CRC — corrupt last byte
        let mut dec2 = MotDecoder::new();
        let mut bad = dg.clone();
        let last = bad.len() - 1;
        bad[last] ^= 0xFF; // flip one CRC byte to corrupt it
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

    #[test]
    fn test_mot_decoder_zero_length_start_is_ignored_without_poisoning_state() {
        let mut dec = MotDecoder::new();

        dec.set_len(0);
        assert!(!dec.process_subfield(true, &[0x01, 0x02, 0x03]));
        assert_eq!(
            dec.size, 0,
            "missing DGLI must not leave partial stale MOT data behind"
        );

        let dg = build_dg_with_crc(&[0x10, 0x20, 0x30]);
        dec.set_len(dg.len());
        assert!(dec.process_subfield(true, &dg));
        assert_eq!(dec.get_data_group(), dg);
    }

    #[test]
    fn test_get_data_group_is_empty_until_crc_checked_group_is_complete() {
        let mut dec = MotDecoder::new();
        let dg = build_dg_with_crc(&[0x21, 0x22, 0x23, 0x24]);
        let mid = dg.len() / 2;

        dec.set_len(dg.len());
        assert!(!dec.process_subfield(true, &dg[..mid]));
        assert!(
            dec.get_data_group().is_empty(),
            "partial MOT groups must not be handed to the manager"
        );
    }

    #[test]
    fn test_mot_decoder_ignores_trailing_padding_after_announced_dgli_length() {
        let mut dec = MotDecoder::new();
        let dg = build_dg_with_crc(&[0x90, 0x91, 0x92, 0x93, 0x94]);

        dec.set_len(dg.len());

        assert!(!dec.process_subfield(true, &dg[..4]));
        assert!(dec.process_subfield(false, &[dg[4], dg[5], dg[6], 0xAA, 0xBB]));
        assert_eq!(dec.get_data_group(), dg);
    }
}
