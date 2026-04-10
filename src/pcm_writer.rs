//! Asynchronous PCM output: decouples frame writing from the audio drain loop.
//!
//! [`spawn_pcm_writer`] starts a dedicated writer thread that owns stdout.
//! The drain loop pushes owned `Vec<i16>` frames via [`PcmWriter::push`], which
//! returns immediately.  A full or slow downstream pipe never blocks the
//! OFDM pipeline, eliminating the backpressure cascade that causes ring-buffer
//! overflow and OFDM sync loss.

use std::io::Write;
use std::sync::mpsc::{self, SyncSender, TrySendError};

/// Capacity of the PCM output queue in frames.
///
/// At HE-AAC v2 rates (~25 frames/s), 100 frames ≈ 4 seconds of buffer.
/// This absorbs transient downstream stalls without growing unboundedly.
pub const PCM_QUEUE_CAPACITY: usize = 100;

/// Non-blocking handle to the background PCM writer thread.
///
/// Constructed by [`spawn_pcm_writer`].  Dropping this value closes the
/// channel and causes the writer thread to exit after draining its queue.
pub struct PcmWriter {
    tx: SyncSender<Vec<i16>>,
}

impl PcmWriter {
    fn new(tx: SyncSender<Vec<i16>>) -> Self {
        Self { tx }
    }

    /// Push an owned PCM frame into the output queue without blocking.
    ///
    /// Returns `true` if the frame was queued.
    /// Returns `false` (logging a warning) if the queue is full, or silently
    /// `false` if the writer thread has already exited.
    pub fn push(&self, frame: Vec<i16>) -> bool {
        match self.tx.try_send(frame) {
            Ok(()) => true,
            Err(TrySendError::Full(_)) => {
                tracing::warn!("PCM output queue full, dropping frame");
                false
            }
            Err(TrySendError::Disconnected(_)) => false,
        }
    }

    /// Test helper: build a writer backed by an in-memory channel.
    #[cfg(test)]
    pub fn new_for_test(capacity: usize) -> (Self, mpsc::Receiver<Vec<i16>>) {
        let (tx, rx) = mpsc::sync_channel(capacity);
        (Self::new(tx), rx)
    }
}

/// Spawn a background thread that writes PCM frames from the queue to `out`
/// as raw S16LE bytes (ETSI TS 102 563 §5.2).
///
/// The thread exits when the returned [`PcmWriter`] is dropped.
pub fn spawn_pcm_writer(out: impl Write + Send + 'static) -> PcmWriter {
    let (tx, rx) = mpsc::sync_channel::<Vec<i16>>(PCM_QUEUE_CAPACITY);
    std::thread::spawn(move || {
        let mut out = out;
        while let Ok(frame) = rx.recv() {
            write_frame(&mut out, &frame);
        }
    });
    PcmWriter::new(tx)
}

/// Encode `samples` as S16LE and write to `out` (ETSI TS 102 563 §5.2).
fn write_frame(out: &mut impl Write, samples: &[i16]) {
    let bytes: Vec<u8> = samples.iter().flat_map(|s| s.to_le_bytes()).collect();
    if let Err(e) = out.write_all(&bytes) {
        tracing::warn!("PCM write failed: {e}");
    }
    if let Err(e) = out.flush() {
        tracing::warn!("PCM flush failed: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── PcmWriter::push ───────────────────────────────────────────────────────

    /// A pushed frame must be retrievable from the backing receiver.
    #[test]
    fn push_delivers_frame_to_receiver() {
        let (writer, rx) = PcmWriter::new_for_test(10);
        let frame = vec![1i16, 2, 3];
        assert!(writer.push(frame.clone()));
        assert_eq!(rx.recv().unwrap(), frame);
    }

    /// Multiple pushed frames must arrive in FIFO order.
    #[test]
    fn push_preserves_frame_order() {
        let (writer, rx) = PcmWriter::new_for_test(10);
        for i in 0..5i16 {
            assert!(writer.push(vec![i]));
        }
        for i in 0..5i16 {
            assert_eq!(rx.recv().unwrap(), vec![i]);
        }
    }

    /// When the queue is at capacity, push must return false and not block.
    #[test]
    fn push_returns_false_when_queue_full() {
        let (writer, _rx) = PcmWriter::new_for_test(2);
        assert!(writer.push(vec![0i16]));
        assert!(writer.push(vec![1i16]));
        // Queue is full; third push must be dropped.
        assert!(!writer.push(vec![2i16]));
    }

    /// When the receiver is dropped, push must return false without panicking.
    #[test]
    fn push_returns_false_when_disconnected() {
        let (writer, rx) = PcmWriter::new_for_test(10);
        drop(rx);
        assert!(!writer.push(vec![0i16]));
    }

    // ── write_frame encoding ──────────────────────────────────────────────────

    /// Samples must be serialised as little-endian i16 (PCM S16LE).
    #[test]
    fn write_frame_encodes_as_s16le() {
        let samples: &[i16] = &[0x0102i16, -1, i16::MIN, i16::MAX];
        let mut buf = Vec::new();
        write_frame(&mut buf, samples);
        let expected: Vec<u8> = samples.iter().flat_map(|s| s.to_le_bytes()).collect();
        assert_eq!(buf, expected);
    }

    /// An empty slice must produce no bytes.
    #[test]
    fn write_frame_empty_slice_produces_no_bytes() {
        let mut buf = Vec::new();
        write_frame(&mut buf, &[]);
        assert!(buf.is_empty());
    }
}
