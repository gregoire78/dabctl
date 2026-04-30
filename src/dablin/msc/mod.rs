//! MSC (Main Service Channel) sub-channel demuxer
//!
//! Extracts the raw bytes for a specific sub-channel (identified by SCID)
//! from parsed ETI frames and feeds them to the DAB+ pipeline.

use crate::dablin::eti::EtiFrame;

/// Extracts the raw data bytes for a given sub-channel ID from an ETI frame.
///
/// Returns `Some(&[u8])` when the sub-channel is present, `None` otherwise.
pub fn extract_subchannel(frame: &EtiFrame, target_scid: u8) -> Option<&[u8]> {
    for (i, entry) in frame.stc.iter().enumerate() {
        if entry.scid == target_scid {
            return frame.streams.get(i).map(|v| v.as_slice());
        }
    }
    None
}

/// Sub-channel stream assembler.
#[allow(dead_code)]
///
/// Buffers CIF-by-CIF data for a single sub-channel and emits
/// complete DAB+ super frames (5 CIFs = 120 ms) for decoding.
pub struct SubchannelBuffer {
    scid: u8,
    cif_bytes: usize,
    /// Accumulated raw bytes from ETI frames
    buffer: Vec<u8>,
}

#[allow(dead_code)]
impl SubchannelBuffer {
    /// Create a new buffer for a sub-channel.
    ///
    /// `stl_cus` is the sub-channel stream length in Capacity Units (STL field).
    /// One CU = 8 bytes in the ETI stream.
    pub fn new(scid: u8, stl_cus: u16) -> Self {
        Self {
            scid,
            cif_bytes: stl_cus as usize * 8,
            buffer: Vec::with_capacity(stl_cus as usize * 8 * 6),
        }
    }

    /// Feed one CIF of sub-channel data.
    pub fn push_cif(&mut self, data: &[u8]) {
        self.buffer.extend_from_slice(data);
    }

    /// Returns the sub-channel ID this buffer serves.
    pub fn scid(&self) -> u8 {
        self.scid
    }

    /// Returns how many bytes per CIF this sub-channel has.
    pub fn cif_bytes(&self) -> usize {
        self.cif_bytes
    }

    /// Super frame size = 5 CIFs of data.
    pub fn superframe_size(&self) -> usize {
        self.cif_bytes * 5
    }

    /// Attempt to peek/extract one super frame from the buffer (dablin sliding window).
    ///
    /// Returns `Some(Vec<u8>)` with the last 5 CIFs worth of bytes if available.
    /// The returned data is NOT consumed — call `advance_one_cif()` or `consume_superframe()`.
    pub fn try_peek_superframe(&self) -> Option<Vec<u8>> {
        let sf_size = self.superframe_size();
        if self.buffer.len() >= sf_size {
            Some(self.buffer[..sf_size].to_vec())
        } else {
            None
        }
    }

    /// Attempt to drain one super frame from the buffer.
    ///
    /// Returns `Some(Vec<u8>)` if at least one super frame worth of bytes
    /// is available, otherwise `None`.
    pub fn try_pop_superframe(&mut self) -> Option<Vec<u8>> {
        let sf_size = self.superframe_size();
        if self.buffer.len() >= sf_size {
            let sf: Vec<u8> = self.buffer.drain(..sf_size).collect();
            Some(sf)
        } else {
            None
        }
    }

    /// Advance one CIF (used in sliding-window sync to shift by one frame).
    pub fn advance_one_cif(&mut self) {
        let cif = self.cif_bytes;
        if self.buffer.len() >= cif {
            self.buffer.drain(..cif);
        }
    }

    /// Number of bytes currently buffered.
    pub fn len(&self) -> usize {
        self.buffer.len()
    }

    /// Number of bytes currently buffered (alias for clarity in runner).
    pub fn buffer_len(&self) -> usize {
        self.buffer.len()
    }

    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    /// Discard all buffered data (re-sync after errors).
    pub fn flush(&mut self) {
        self.buffer.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dablin::eti::{EtiFrame};
    use crate::dablin::eti::StcEntry;

    fn make_frame_with_stream(scid: u8, data: Vec<u8>) -> EtiFrame {
        EtiFrame {
            err: 0,
            fct: 0,
            ficf: true,
            nst: 1,
            fp: 0,
            mid: 1,
            fl: 40,
            stc: vec![StcEntry { scid, sad: 0, tpl: 0x22, stl: 33 }],
            mnsc: 0,
            fic: vec![],
            streams: vec![data],
        }
    }

    #[test]
    fn test_extract_subchannel_found() {
        let payload: Vec<u8> = (0..264u16).map(|i| (i & 0xff) as u8).collect();
        let frame = make_frame_with_stream(3, payload.clone());
        let extracted = extract_subchannel(&frame, 3).unwrap();
        assert_eq!(extracted, payload.as_slice());
    }

    #[test]
    fn test_extract_subchannel_not_found() {
        let frame = make_frame_with_stream(3, vec![0u8; 264]);
        assert!(extract_subchannel(&frame, 99).is_none());
    }

    #[test]
    fn test_buffer_accumulates_5_cifs() {
        let stl: u16 = 33;
        let cif_size = stl as usize * 8; // 264
        let mut buf = SubchannelBuffer::new(1, stl);

        for _i in 0..4 {
            buf.push_cif(&vec![0xAA; cif_size]);
            assert!(buf.try_pop_superframe().is_none());
        }
        buf.push_cif(&vec![0xBB; cif_size]);
        let sf = buf.try_pop_superframe().unwrap();
        assert_eq!(sf.len(), cif_size * 5);
        assert_eq!(&sf[0..cif_size], vec![0xAA; cif_size].as_slice());
        assert_eq!(&sf[cif_size * 4..], vec![0xBB; cif_size].as_slice());
    }

    #[test]
    fn test_buffer_flush_clears_data() {
        let mut buf = SubchannelBuffer::new(1, 33);
        buf.push_cif(&vec![0x00; 264]);
        assert_eq!(buf.len(), 264);
        buf.flush();
        assert_eq!(buf.len(), 0);
        assert!(buf.is_empty());
    }

    #[test]
    fn test_superframe_size_calculation() {
        let buf = SubchannelBuffer::new(1, 33);
        assert_eq!(buf.cif_bytes(), 264); // 33 * 8
        assert_eq!(buf.superframe_size(), 1320); // 264 * 5
    }
}
