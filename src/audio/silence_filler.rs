//! Silence fill helpers for DAB+ audio stream continuity.
//!
//! During radio fades, the superframe decoder emits `sync_fail` events instead
//! of decoded audio.  Rather than producing silence for every `sync_fail`
//! immediately (which would interleave silence with real audio on a brief
//! recovery), silence frames are buffered and only emitted once a configurable
//! hold-off period has elapsed without a `sync_ok`.
//!
//! ETSI TS 102 563 §5.1 — one DAB+ superframe = 5 CIF × ~24 ms = ~120 ms.

use std::time::{Duration, Instant};

// ── Timing helpers ────────────────────────────────────────────────────────────

/// Returns the silence deadline to set after a real PCM AU has been emitted.
///
/// Grants a 48 ms grace window (2 CIF frames, ETSI TS 102 563 §5.1) before
/// silence injection is allowed.  This filters out brief sync hiccups shorter
/// than 2 CIF periods without creating a large initial gap at the start of a
/// genuine fade.  48 ms (= holdoff × CIF period) was chosen because `flush()`
/// now guarantees chronological ordering anyway; the 120 ms window previously
/// needed to prevent interleaving is no longer required.
pub fn silence_deadline_after_good_au(now: Instant) -> Instant {
    now + Duration::from_millis(48)
}

/// Advances the silence deadline by exactly one superframe period (120 ms)
/// from `prev`, then clamps to `now` if the result has drifted behind real
/// time (e.g. after a pipeline stall).
///
/// Advancing from the previous deadline rather than from `now` keeps the
/// long-run fill rate at exactly 8.33 fills/s regardless of CIF-quantisation
/// jitter (~24 ms per frame).  Advancing from `now` instead would produce
/// ~144 ms intervals and a fill rate of ~0.89× real-time, causing ffmpeg to
/// report `speed < 1.0` during signal fades.
///
/// One DAB+ superframe = 5 CIF × 24 ms = 120 ms (ETSI TS 102 563 §5.1).
pub fn advance_silence_deadline(prev: Instant, now: Instant) -> Instant {
    let next = prev + Duration::from_millis(120);
    if next < now {
        now
    } else {
        next
    }
}

// ── SilenceBuffer ─────────────────────────────────────────────────────────────

/// Deferred silence buffer: accumulates synthetic silence frames and only emits
/// them once `holdoff` ticks have elapsed without a `cancel()`.
///
/// ## Rationale (ETSI TS 102 563 §5.1)
///
/// The `SuperframeFilter` advances by one CIF frame (~24 ms) on each
/// `sync_fail`, so it may find a valid superframe on the very next tick.
/// Writing silence immediately would interleave it with real audio decoded
/// 24 ms later.
///
/// By holding silence for `holdoff` ticks without a `cancel()` (e.g. a full
/// superframe = 5 ticks ≈ 120 ms), a recovery transition (`sync_fail`
/// followed immediately by `sync_ok`) never produces
/// `AU_OK → silence → AU_OK` in the output stream.
pub struct SilenceBuffer {
    /// Number of `tick()` calls to wait before flushing accumulated frames.
    holdoff: u32,
    /// Ticks elapsed since the last push/flush.
    ticks: u32,
    /// Pending silence frames (not yet written).
    frames: Vec<Vec<i16>>,
}

impl SilenceBuffer {
    /// Create a new buffer with the given hold-off tick count.
    pub fn new(holdoff: u32) -> Self {
        Self {
            holdoff,
            ticks: 0,
            frames: Vec::new(),
        }
    }

    /// Enqueue a silence frame and (re)start the hold-off counter.
    pub fn push(&mut self, frame: Vec<i16>) {
        self.frames.push(frame);
        self.ticks = 0;
    }

    /// Discard all pending silence.
    ///
    /// Call this when a good AU has been decoded so that silence queued during
    /// a brief sync hiccup is never emitted.
    pub fn cancel(&mut self) {
        self.frames.clear();
        self.ticks = 0;
    }

    /// Return ALL pending frames immediately, bypassing the hold-off counter,
    /// and reset the buffer.
    ///
    /// Use this on `sync_ok` to emit buffered silence **before** writing real
    /// audio, keeping the stream chronologically ordered *and* ensuring silence
    /// is never discarded (preserving speed = 1.0×).
    pub fn flush(&mut self) -> Vec<Vec<i16>> {
        self.ticks = 0;
        std::mem::take(&mut self.frames)
    }

    /// Advance one CIF tick (~24 ms).
    ///
    /// Returns the accumulated silence frames once `holdoff` ticks have
    /// elapsed; returns an empty vec otherwise.  After flushing, the buffer
    /// resets so a subsequent `push()` starts a new hold-off cycle.
    pub fn tick(&mut self) -> Vec<Vec<i16>> {
        if self.frames.is_empty() {
            return Vec::new();
        }
        self.ticks += 1;
        if self.ticks >= self.holdoff {
            self.ticks = 0;
            std::mem::take(&mut self.frames)
        } else {
            Vec::new()
        }
    }

    /// Returns `true` if at least one silence frame is queued.
    pub fn has_pending(&self) -> bool {
        !self.frames.is_empty()
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── silence_deadline_after_good_au ────────────────────────────────────────

    /// After a good AU, the deadline must be exactly 48 ms in the future (2 CIF
    /// ticks) so that sync_fail cannot push silence for very brief hiccups while
    /// still minimising the initial gap at the start of a genuine fade.
    /// With flush() guaranteeing ordering, a 120 ms window is no longer needed.
    #[test]
    fn silence_deadline_after_good_au_is_48ms_ahead() {
        let now = Instant::now();
        let deadline = silence_deadline_after_good_au(now);
        assert_eq!(deadline.duration_since(now), Duration::from_millis(48));
    }

    // ── advance_silence_deadline ──────────────────────────────────────────────

    /// When the previous deadline is in the future, `advance_silence_deadline`
    /// must step forward exactly 120 ms from it — not from `now`.
    /// This keeps the long-run fill rate at exactly 8.33 fills/s.
    #[test]
    fn advance_silence_deadline_steps_from_prev_when_in_future() {
        let base = Instant::now();
        let prev = base + Duration::from_millis(10); // still in the future
        let now = base;
        let next = advance_silence_deadline(prev, now);
        assert_eq!(next.duration_since(prev), Duration::from_millis(120));
    }

    /// When the previous deadline has already drifted behind `now` (pipeline
    /// stall), `advance_silence_deadline` must clamp to `now` so the next
    /// iteration does not emit a burst of silence frames.
    #[test]
    fn advance_silence_deadline_clamps_to_now_on_stall() {
        let now = Instant::now();
        let prev = now - Duration::from_millis(500);
        let next = advance_silence_deadline(prev, now);
        assert!(next >= now);
    }

    /// Consecutive calls must keep the deadline advancing at exactly 120 ms per
    /// step, producing exactly 8.33 fills/s with no drift.
    #[test]
    fn advance_silence_deadline_rate_is_8_33_per_second() {
        let start = Instant::now();
        let mut deadline = start;
        for i in 1..=10u32 {
            let now = deadline; // deadline is exactly met — worst-case jitter
            deadline = advance_silence_deadline(deadline, now);
            let expected = start + Duration::from_millis(120 * u64::from(i));
            assert_eq!(deadline, expected, "step {i} deadline drifted");
        }
    }

    // ── SilenceBuffer ─────────────────────────────────────────────────────────

    /// A freshly created buffer has no pending silence.
    #[test]
    fn silence_buffer_new_has_no_pending() {
        let buf = SilenceBuffer::new(5);
        assert!(!buf.has_pending());
    }

    /// push() accumulates pending silence; tick() does not flush until the
    /// hold-off count is reached.
    #[test]
    fn silence_buffer_tick_does_not_flush_before_holdoff() {
        let silence = vec![0i16; 4];
        let mut buf = SilenceBuffer::new(3);
        buf.push(silence.clone());
        assert!(buf.tick().is_empty());
        assert!(buf.tick().is_empty());
        assert!(buf.has_pending());
    }

    /// tick() returns accumulated frames once the hold-off is reached.
    #[test]
    fn silence_buffer_tick_flushes_after_holdoff() {
        let silence = vec![0i16; 4];
        let mut buf = SilenceBuffer::new(3);
        buf.push(silence.clone());
        buf.push(silence.clone());
        let _ = buf.tick();
        let _ = buf.tick();
        let flushed = buf.tick();
        assert_eq!(flushed.len(), 2);
        assert!(!buf.has_pending());
    }

    /// cancel() discards all pending silence; subsequent tick() returns nothing.
    #[test]
    fn silence_buffer_cancel_discards_pending() {
        let silence = vec![0i16; 4];
        let mut buf = SilenceBuffer::new(3);
        buf.push(silence.clone());
        buf.push(silence.clone());
        buf.cancel();
        assert!(!buf.has_pending());
        assert!(buf.tick().is_empty());
        assert!(buf.tick().is_empty());
        assert!(buf.tick().is_empty());
    }

    /// flush() returns ALL pending frames immediately (no hold-off) and resets
    /// the buffer.
    #[test]
    fn silence_buffer_flush_returns_all_pending_immediately() {
        let silence = vec![0i16; 4];
        let mut buf = SilenceBuffer::new(5);
        buf.push(silence.clone());
        buf.push(silence.clone());
        buf.push(silence.clone());
        let flushed = buf.flush();
        assert_eq!(flushed.len(), 3);
        assert!(!buf.has_pending());
    }

    /// After flush(), tick() returns nothing.
    #[test]
    fn silence_buffer_flush_leaves_buffer_empty() {
        let silence = vec![0i16; 4];
        let mut buf = SilenceBuffer::new(3);
        buf.push(silence.clone());
        let _ = buf.flush();
        assert!(buf.tick().is_empty());
        assert!(buf.tick().is_empty());
        assert!(buf.tick().is_empty());
    }

    /// flush() on an already-empty buffer returns an empty vec.
    #[test]
    fn silence_buffer_flush_on_empty_returns_empty() {
        let mut buf = SilenceBuffer::new(3);
        assert!(buf.flush().is_empty());
    }

    /// After a flush, the hold-off counter resets; a new push must wait another
    /// full hold-off before flushing.
    #[test]
    fn silence_buffer_resets_after_flush() {
        let silence = vec![0i16; 4];
        let mut buf = SilenceBuffer::new(2);
        buf.push(silence.clone());
        let _ = buf.tick();
        let flushed = buf.tick();
        assert_eq!(flushed.len(), 1);

        buf.push(silence.clone());
        assert!(buf.tick().is_empty());
        let flushed2 = buf.tick();
        assert_eq!(flushed2.len(), 1);
    }

    /// Multiple consecutive push() calls accumulate; they are all returned on flush.
    #[test]
    fn silence_buffer_accumulates_multiple_pushes() {
        let silence = vec![0i16; 4];
        let mut buf = SilenceBuffer::new(1);
        for _ in 0..5 {
            buf.push(silence.clone());
        }
        let flushed = buf.tick();
        assert_eq!(flushed.len(), 5);
    }
}
