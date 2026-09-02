use anyhow::{Context, Result};
use std::collections::VecDeque;
use std::io::Read;
use tracing::warn;

use crate::dablin::eti::{parse_frame, EtiFrame, FsyncState, ETI_FRAME_SIZE};

/// Outcome of one ETI frame read+parse+fsync step.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum EtiStepStatus {
    /// Parse error or bad frame - caller should continue.
    BadFrame,
    /// End of stream - caller should break.
    Eof,
    /// Successfully parsed frame.
    Frame,
}

/// Parsed step payload and status for one ETI frame read iteration.
pub(crate) struct EtiStep<'a> {
    status: EtiStepStatus,
    frame: Option<EtiFrame<'a>>,
}

enum ScanDecision {
    EmitAligned,
    BadFrame,
    NeedMore,
}

impl<'a> EtiStep<'a> {
    pub(crate) fn eof() -> Self {
        Self {
            status: EtiStepStatus::Eof,
            frame: None,
        }
    }

    pub(crate) fn bad_frame() -> Self {
        Self {
            status: EtiStepStatus::BadFrame,
            frame: None,
        }
    }

    pub(crate) fn frame(frame: EtiFrame<'a>) -> Self {
        Self {
            status: EtiStepStatus::Frame,
            frame: Some(frame),
        }
    }

    pub(crate) fn status(&self) -> EtiStepStatus {
        self.status
    }

    pub(crate) fn into_frame(self) -> Option<EtiFrame<'a>> {
        self.frame
    }
}

/// Stateful ETI frame reader keeping FSYNC and frame counters together.
pub(crate) struct EtiFrameReader {
    fsync_state: FsyncState,
    frame_count: u64,
    pending: VecDeque<u8>,
    eof: bool,
    synced: bool,
}

impl EtiFrameReader {
    pub(crate) fn new() -> Self {
        Self {
            fsync_state: FsyncState::new(),
            frame_count: 0,
            pending: VecDeque::new(),
            eof: false,
            synced: false,
        }
    }

    pub(crate) fn frame_count(&self) -> u64 {
        self.frame_count
    }

    pub(crate) fn read_step<'buf>(
        &mut self,
        reader: &mut impl Read,
        frame_buf: &'buf mut [u8],
    ) -> Result<EtiStep<'buf>> {
        loop {
            let required = if self.synced {
                ETI_FRAME_SIZE
            } else {
                ETI_FRAME_SIZE * 2
            };

            while self.pending.len() < required && !self.eof {
                self.fill_pending(reader)?;
            }

            if self.pending.len() < ETI_FRAME_SIZE {
                return Ok(EtiStep::eof());
            }

            let decision = if self.synced {
                self.scan_synced_step()
            } else {
                self.scan_resync_step()
            }?;

            match decision {
                ScanDecision::EmitAligned => {
                    self.copy_front_frame(frame_buf);
                    let frame = match parse_frame(frame_buf) {
                        Ok(frame) => frame,
                        Err(e) => {
                            warn!("ETI parse error frame {}: {}", self.frame_count, e);
                            self.fsync_state.reset();
                            self.synced = false;
                            self.pending.pop_front();
                            self.frame_count += 1;
                            return Ok(EtiStep::bad_frame());
                        }
                    };
                    self.pending.drain(..ETI_FRAME_SIZE);
                    self.frame_count += 1;
                    return Ok(EtiStep::frame(frame));
                }
                ScanDecision::BadFrame => return Ok(EtiStep::bad_frame()),
                ScanDecision::NeedMore => continue,
            }
        }
    }

    fn scan_synced_step(&mut self) -> Result<ScanDecision> {
        let pending = self.pending.make_contiguous();
        let fsync = [pending[1], pending[2], pending[3]];

        if parse_frame(&pending[..ETI_FRAME_SIZE]).is_err() {
            warn!(
                "ETI parse error frame {}: malformed aligned frame",
                self.frame_count
            );
            self.fsync_state.reset();
            self.synced = false;
            self.pending.pop_front();
            self.frame_count += 1;
            return Ok(ScanDecision::BadFrame);
        }

        if !self.fsync_state.check(fsync) {
            warn!("FSYNC mismatch at frame {}, re-syncing", self.frame_count);
            self.fsync_state.reset();
            self.synced = false;
            self.pending.pop_front();
            self.frame_count += 1;
            return Ok(ScanDecision::BadFrame);
        }

        Ok(ScanDecision::EmitAligned)
    }

    fn scan_resync_step(&mut self) -> Result<ScanDecision> {
        enum ResyncResult {
            NeedMore,
            EmitAligned([u8; 3]),
        }

        let result = {
            let pending = self.pending.make_contiguous();
            match parse_frame(&pending[..ETI_FRAME_SIZE]) {
                Ok(candidate) => {
                    let fsync = [pending[1], pending[2], pending[3]];

                    if self.eof && pending.len() < ETI_FRAME_SIZE * 2 {
                        ResyncResult::EmitAligned(fsync)
                    } else if pending.len() < ETI_FRAME_SIZE * 2 {
                        ResyncResult::NeedMore
                    } else {
                        let next_fsync = [
                            pending[ETI_FRAME_SIZE + 1],
                            pending[ETI_FRAME_SIZE + 2],
                            pending[ETI_FRAME_SIZE + 3],
                        ];
                        let second_fct =
                            match parse_frame(&pending[ETI_FRAME_SIZE..ETI_FRAME_SIZE * 2]) {
                                Ok(frame) => frame.fct,
                                Err(_) => return Ok(ScanDecision::NeedMore),
                            };
                        let fsync_b = [!fsync[0], !fsync[1], !fsync[2]];
                        if next_fsync == fsync_b && second_fct == candidate.fct.wrapping_add(1) {
                            ResyncResult::EmitAligned(fsync)
                        } else {
                            ResyncResult::NeedMore
                        }
                    }
                }
                Err(_) => ResyncResult::NeedMore,
            }
        };

        match result {
            ResyncResult::EmitAligned(fsync) => {
                self.fsync_state.reset();
                self.fsync_state.check(fsync);
                self.synced = true;
                Ok(ScanDecision::EmitAligned)
            }
            ResyncResult::NeedMore => {
                self.pending.pop_front();
                Ok(ScanDecision::NeedMore)
            }
        }
    }

    fn fill_pending(&mut self, reader: &mut impl Read) -> Result<()> {
        let mut chunk = [0u8; ETI_FRAME_SIZE];
        let n = reader.read(&mut chunk).context("ETI read error")?;
        if n == 0 {
            self.eof = true;
            return Ok(());
        }
        self.pending.extend(&chunk[..n]);
        Ok(())
    }

    fn copy_front_frame(&mut self, frame_buf: &mut [u8]) {
        let pending = self.pending.make_contiguous();
        frame_buf[..ETI_FRAME_SIZE].copy_from_slice(&pending[..ETI_FRAME_SIZE]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dablin::eti::EtiError;
    use std::io::Cursor;

    fn load_fixture_frame(index: usize) -> Vec<u8> {
        let eti = std::fs::read("test-local/multiplex.eti").unwrap();
        let start = index * ETI_FRAME_SIZE;
        eti[start..start + ETI_FRAME_SIZE].to_vec()
    }

    fn make_cursor_from_frames(prefix_len: usize, frame_indexes: &[usize]) -> Cursor<Vec<u8>> {
        let mut input = vec![0x00; prefix_len];
        for &index in frame_indexes {
            input.extend(load_fixture_frame(index));
        }
        Cursor::new(input)
    }

    #[test]
    fn resync_skips_prefix_and_emits_stable_frames() {
        let mut reader = EtiFrameReader::new();
        let mut cursor = make_cursor_from_frames(137, &[0, 1, 2]);

        let mut first_buf = vec![0u8; ETI_FRAME_SIZE];
        let first = reader.read_step(&mut cursor, &mut first_buf).unwrap();
        assert_eq!(first.status(), EtiStepStatus::Frame);
        let _ = first.into_frame().unwrap();

        let mut second_buf = vec![0u8; ETI_FRAME_SIZE];
        let second = reader.read_step(&mut cursor, &mut second_buf).unwrap();
        assert_eq!(second.status(), EtiStepStatus::Frame);
        let _ = second.into_frame().unwrap();

        let mut third_buf = vec![0u8; ETI_FRAME_SIZE];
        let third = reader.read_step(&mut cursor, &mut third_buf).unwrap();
        assert_eq!(third.status(), EtiStepStatus::Frame);
        let _ = third.into_frame().unwrap();
    }

    #[test]
    fn parse_frame_rejects_corrupted_header_crc() {
        let mut frame = load_fixture_frame(0);
        let header_crc_offset = {
            let parsed = parse_frame(&frame).unwrap();
            8 + parsed.stc.len() * 4 + 2
        };
        frame[header_crc_offset] ^= 0x01;

        let result = parse_frame(&frame);
        assert!(matches!(result, Err(EtiError::HeaderCrcMismatch { .. })));
    }

    #[test]
    fn frame_count_advances_after_emitted_frames() {
        let mut reader = EtiFrameReader::new();
        let mut cursor = make_cursor_from_frames(0, &[0, 1]);
        let mut frame_buf = vec![0u8; ETI_FRAME_SIZE];

        let first = reader.read_step(&mut cursor, &mut frame_buf).unwrap();
        assert_eq!(first.status(), EtiStepStatus::Frame);
        let _ = first.into_frame().unwrap();
        assert_eq!(reader.frame_count(), 1);

        let second = reader.read_step(&mut cursor, &mut frame_buf).unwrap();
        assert_eq!(second.status(), EtiStepStatus::Frame);
        let _ = second.into_frame().unwrap();
        assert_eq!(reader.frame_count(), 2);
    }

    #[test]
    fn eof_after_valid_frame_returns_eof() {
        let mut reader = EtiFrameReader::new();
        let mut cursor = Cursor::new(load_fixture_frame(0));
        let mut frame_buf = vec![0u8; ETI_FRAME_SIZE];

        let first = reader.read_step(&mut cursor, &mut frame_buf).unwrap();
        assert_eq!(first.status(), EtiStepStatus::Frame);
        let _ = first.into_frame().unwrap();

        let second = reader.read_step(&mut cursor, &mut frame_buf).unwrap();
        assert_eq!(second.status(), EtiStepStatus::Eof);
    }

    #[test]
    fn short_input_at_eof_returns_eof() {
        let mut reader = EtiFrameReader::new();
        let mut frame_buf = vec![0u8; ETI_FRAME_SIZE];
        let mut cursor = Cursor::new(vec![0x01; 100]);

        let step = reader.read_step(&mut cursor, &mut frame_buf).unwrap();
        assert_eq!(step.status(), EtiStepStatus::Eof);
    }
}
