use anyhow::{Context, Result};
use std::io::{self, Read};
use tracing::warn;

use crate::dablin::eti::{parse_frame, EtiFrame, FsyncState};

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
}

impl EtiFrameReader {
    pub(crate) fn new() -> Self {
        Self {
            fsync_state: FsyncState::new(),
            frame_count: 0,
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
        match reader.read_exact(frame_buf) {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => return Ok(EtiStep::eof()),
            Err(e) => return Err(e).context("ETI read error"),
        }
        let frame = match parse_frame(frame_buf) {
            Ok(f) => f,
            Err(e) => {
                warn!("ETI parse error frame {}: {}", self.frame_count, e);
                self.fsync_state.reset();
                self.frame_count += 1;
                return Ok(EtiStep::bad_frame());
            }
        };

        let fsync = [frame_buf[1], frame_buf[2], frame_buf[3]];
        if !self.fsync_state.check(fsync) {
            warn!("FSYNC mismatch at frame {}, re-syncing", self.frame_count);
            self.fsync_state.reset();
            self.fsync_state.check(fsync);
        }

        self.frame_count += 1;
        Ok(EtiStep::frame(frame))
    }
}
