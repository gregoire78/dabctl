/// ETI frame reader: synchronizes on FSYNC and delivers 6144-byte frames.
use std::io::Read;

pub const ETI_FRAME_SIZE: usize = 6144;
const FSYNC0: [u8; 3] = [0x07, 0x3A, 0xB6];
const FSYNC1: [u8; 3] = [0xF8, 0xC5, 0x49];

pub struct EtiReader<R: Read> {
    reader: R,
    buf: Vec<u8>,
    pos: usize,
}

impl<R: Read> EtiReader<R> {
    pub fn new(reader: R) -> Self {
        EtiReader {
            reader,
            buf: vec![0u8; ETI_FRAME_SIZE * 3],
            pos: 0,
        }
    }

    /// Read the next valid ETI frame. Returns None on EOF.
    pub fn next_frame(&mut self) -> std::io::Result<Option<[u8; ETI_FRAME_SIZE]>> {
        loop {
            // Ensure we have enough data in buffer
            while self.pos < ETI_FRAME_SIZE {
                let n = self.reader.read(&mut self.buf[self.pos..])?;
                if n == 0 {
                    return Ok(None);
                }
                self.pos += n;
            }

            // Scan for FSYNC at bytes [1..4]
            if is_fsync(&self.buf[1..4]) {
                let mut frame = [0u8; ETI_FRAME_SIZE];
                frame.copy_from_slice(&self.buf[..ETI_FRAME_SIZE]);
                // Shift remaining data
                let remaining = self.pos - ETI_FRAME_SIZE;
                self.buf.copy_within(ETI_FRAME_SIZE..self.pos, 0);
                self.pos = remaining;
                return Ok(Some(frame));
            }

            // No sync found: scan forward for next FSYNC
            if let Some(offset) = find_fsync(&self.buf[..self.pos]) {
                self.buf.copy_within(offset..self.pos, 0);
                self.pos -= offset;
            } else {
                // Keep last 3 bytes in case FSYNC straddles boundary
                let keep = 3.min(self.pos);
                self.buf.copy_within((self.pos - keep)..self.pos, 0);
                self.pos = keep;
            }
        }
    }
}

fn is_fsync(data: &[u8]) -> bool {
    data == FSYNC0 || data == FSYNC1
}

/// Find the start of an ETI frame by scanning for FSYNC at offset 1
fn find_fsync(data: &[u8]) -> Option<usize> {
    for i in 0..data.len().saturating_sub(3) {
        if is_fsync(&data[i + 1..i + 4]) {
            return Some(i);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_fsync() {
        assert!(is_fsync(&FSYNC0));
        assert!(is_fsync(&FSYNC1));
        assert!(!is_fsync(&[0x00, 0x00, 0x00]));
    }

    #[test]
    fn test_read_single_frame() {
        let mut frame_data = vec![0u8; ETI_FRAME_SIZE];
        frame_data[0] = 0xFF; // ERR
        frame_data[1] = 0x07; // FSYNC0
        frame_data[2] = 0x3A;
        frame_data[3] = 0xB6;

        let cursor = std::io::Cursor::new(frame_data.clone());
        let mut reader = EtiReader::new(cursor);
        let frame = reader.next_frame().unwrap().unwrap();
        assert_eq!(&frame[0..4], &[0xFF, 0x07, 0x3A, 0xB6]);
    }

    #[test]
    fn test_read_with_garbage_prefix() {
        let garbage = vec![0xDE, 0xAD, 0xBE, 0xEF, 0x01, 0x02];
        let mut frame_data = vec![0u8; ETI_FRAME_SIZE];
        frame_data[0] = 0xFF;
        frame_data[1] = 0xF8; // FSYNC1
        frame_data[2] = 0xC5;
        frame_data[3] = 0x49;

        let mut input = garbage;
        input.extend_from_slice(&frame_data);

        let cursor = std::io::Cursor::new(input);
        let mut reader = EtiReader::new(cursor);
        let frame = reader.next_frame().unwrap().unwrap();
        assert_eq!(&frame[1..4], &[0xF8, 0xC5, 0x49]);
    }

    #[test]
    fn test_eof_returns_none() {
        let cursor = std::io::Cursor::new(vec![0u8; 100]);
        let mut reader = EtiReader::new(cursor);
        assert!(reader.next_frame().unwrap().is_none());
    }
}
