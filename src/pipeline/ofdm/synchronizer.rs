// ETSI EN 300 401 §8.4 — synchronisation, null symbol detection

/// Circular-buffer size for the sliding amplitude window.
const SYNC_BUFFER_SIZE: usize = 32768;
/// Mask for fast modulo arithmetic on `SYNC_BUFFER_SIZE`.
const SYNC_BUFFER_MASK: usize = SYNC_BUFFER_SIZE - 1;
/// Number of samples in the sliding amplitude window.
const SLIDING_WINDOW: usize = 50;

/// Result of processing one sample through the sync state machine.
#[derive(Debug, PartialEq, Eq)]
pub enum NullState {
    /// Still searching; no transition has occurred.
    Searching,
    /// Amplitude dropped below 0.50 × s_level — entered null symbol.
    NullFound,
    /// Amplitude rose above 0.75 × s_level — null symbol has ended.
    EndOfNull,
    /// Timed out waiting for the null or end-of-null.
    ///
    /// After a `Timeout`, call [`SyncState::false_sync_pending`] to check
    /// whether 5 consecutive timeouts occurred (caller should then emit a
    /// sync-lost signal).
    Timeout,
}

#[derive(Debug, PartialEq, Eq)]
enum SyncPhase {
    SeekingNull,
    SeekingEndOfNull,
}

/// Frame synchroniser state machine.
///
/// Maintains a 50-sample sliding amplitude window. Call [`reset`] at the
/// start of each acquisition attempt, [`prefill`] 50 times to initialise
/// the window, then [`detect_null`] once per sample.
///
/// [`reset`]: SyncState::reset
/// [`prefill`]: SyncState::prefill
/// [`detect_null`]: SyncState::detect_null
pub struct SyncState {
    env_buffer: Vec<f32>,
    sync_buffer_index: usize,
    current_strength: f32,
    attempts: i16,
    counter: i32,
    phase: SyncPhase,
    t_f: usize,
    t_null: usize,
    /// Set to `true` when the 5th consecutive timeout occurs.
    /// Consumed (cleared) by [`false_sync_pending`].
    ///
    /// [`false_sync_pending`]: SyncState::false_sync_pending
    false_sync: bool,
}

impl SyncState {
    /// Create a new synchroniser state machine.
    ///
    /// `t_f`    – DAB frame length in samples (determines null-search timeout).
    /// `t_null` – null symbol length in samples (determines end-of-null timeout).
    pub fn new(t_f: usize, t_null: usize) -> Self {
        SyncState {
            env_buffer: vec![0.0f32; SYNC_BUFFER_SIZE],
            sync_buffer_index: 0,
            current_strength: 0.0,
            attempts: 0,
            counter: 0,
            phase: SyncPhase::SeekingNull,
            t_f,
            t_null,
            false_sync: false,
        }
    }

    /// Reset sliding window state for a new acquisition attempt.
    ///
    /// Resets the circular buffer, current strength, and counter back to zero.
    /// The attempt counter (`attempts`) is preserved across resets so that
    /// consecutive timeouts accumulate correctly.
    pub fn reset(&mut self) {
        self.sync_buffer_index = 0;
        self.current_strength = 0.0;
        self.counter = 0;
        self.phase = SyncPhase::SeekingNull;
        // env_buffer values from the previous attempt are overwritten naturally
        // as the new window fills, but we zero them for correctness.
        self.env_buffer.fill(0.0);
    }

    /// Add one amplitude sample to initialise the sliding window (no subtract).
    ///
    /// Must be called exactly [`SLIDING_WINDOW`] (50) times after [`reset`]
    /// before the first [`detect_null`] call.
    ///
    /// [`reset`]: SyncState::reset
    /// [`detect_null`]: SyncState::detect_null
    pub fn prefill(&mut self, amplitude: f32) {
        self.env_buffer[self.sync_buffer_index] = amplitude;
        self.current_strength += amplitude;
        self.sync_buffer_index = (self.sync_buffer_index + 1) & SYNC_BUFFER_MASK;
    }

    /// Process one sample amplitude through the sync state machine.
    ///
    /// Returns:
    /// - [`Searching`]  – still looking for the null or end-of-null
    /// - [`NullFound`]  – amplitude dropped below 0.50 × s_level (entering null)
    /// - [`EndOfNull`]  – amplitude rose above 0.75 × s_level (null period over)
    /// - [`Timeout`]    – exceeded frame-length budget without finding sync
    ///
    /// After a [`Timeout`], call [`false_sync_pending`] to check whether 5
    /// consecutive timeouts occurred (caller should emit `sync_signal(false)`).
    ///
    /// [`Searching`]: NullState::Searching
    /// [`NullFound`]: NullState::NullFound
    /// [`EndOfNull`]: NullState::EndOfNull
    /// [`Timeout`]: NullState::Timeout
    /// [`false_sync_pending`]: SyncState::false_sync_pending
    pub fn detect_null(&mut self, amplitude: f32, s_level: f32) -> NullState {
        // Update sliding window.
        self.env_buffer[self.sync_buffer_index] = amplitude;
        let old_idx =
            (self.sync_buffer_index + SYNC_BUFFER_SIZE - SLIDING_WINDOW) & SYNC_BUFFER_MASK;
        self.current_strength += amplitude - self.env_buffer[old_idx];
        self.sync_buffer_index = (self.sync_buffer_index + 1) & SYNC_BUFFER_MASK;

        match self.phase {
            SyncPhase::SeekingNull => {
                // Null detected: mean amplitude dropped below half the long-term level.
                if self.current_strength / SLIDING_WINDOW as f32 <= 0.50 * s_level {
                    self.phase = SyncPhase::SeekingEndOfNull;
                    self.counter = 0;
                    return NullState::NullFound;
                }
                self.counter += 1;
                if self.counter > self.t_f as i32 {
                    self.attempts += 1;
                    self.counter = 0; // reset for next round within this acquisition
                    if self.attempts >= 5 {
                        self.false_sync = true;
                        self.attempts = 0;
                    }
                    return NullState::Timeout;
                }
                NullState::Searching
            }
            SyncPhase::SeekingEndOfNull => {
                // End-of-null: mean amplitude recovered above 75% of long-term level.
                if self.current_strength / SLIDING_WINDOW as f32 >= 0.75 * s_level {
                    self.phase = SyncPhase::SeekingNull;
                    self.counter = 0;
                    return NullState::EndOfNull;
                }
                self.counter += 1;
                if self.counter > self.t_null as i32 + SLIDING_WINDOW as i32 {
                    self.phase = SyncPhase::SeekingNull;
                    self.counter = 0;
                    return NullState::Timeout;
                }
                NullState::Searching
            }
        }
    }

    /// Returns `true` once if the 5th consecutive timeout just triggered.
    ///
    /// Consuming: the flag is cleared after this call returns `true`.
    pub fn false_sync_pending(&mut self) -> bool {
        if self.false_sync {
            self.false_sync = false;
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn null_detection_state_transitions() {
        // t_f large enough that we won't hit SeekingNull timeout during the test.
        let mut state = SyncState::new(200_000, 2_656);

        // Prefill 50 samples with amplitude=1.0 → current_strength=50, window filled.
        for _ in 0..SLIDING_WINDOW {
            state.prefill(1.0);
        }

        // Step 1: drive amplitude to 0 until NullFound.
        // Each call subtracts one 1.0 and adds one 0.0.
        // After 25 calls: current_strength/50 = 0.5 ≤ 0.5 → NullFound.
        let mut null_found = false;
        for _ in 0..200 {
            match state.detect_null(0.0, 1.0) {
                NullState::NullFound => {
                    null_found = true;
                    break;
                }
                NullState::Timeout => panic!("unexpected timeout before NullFound"),
                _ => {}
            }
        }
        assert!(
            null_found,
            "NullFound should be detected within 200 samples"
        );

        // Step 2: flush remaining 1.0 s from the window (50 more zero-amplitude calls).
        for _ in 0..SLIDING_WINDOW {
            state.detect_null(0.0, 1.0);
        }

        // Step 3: drive amplitude back up until EndOfNull.
        // Each call adds 1.0; after 38 calls: 38/50 = 0.76 ≥ 0.75 → EndOfNull.
        let mut end_found = false;
        for _ in 0..200 {
            match state.detect_null(1.0, 1.0) {
                NullState::EndOfNull => {
                    end_found = true;
                    break;
                }
                NullState::Timeout => panic!("unexpected timeout before EndOfNull"),
                _ => {}
            }
        }
        assert!(end_found, "EndOfNull should be detected within 200 samples");
    }

    #[test]
    fn timeout_emits_false_sync_after_five_attempts() {
        // With t_f=10: counter > 10 triggers timeout on the 11th detect_null call.
        // After 5 such rounds (5×11 = 55 calls total), false_sync_pending() is true.
        let mut state = SyncState::new(10, 20);

        // Prefill 50 samples with amplitude=1.0.
        for _ in 0..SLIDING_WINDOW {
            state.prefill(1.0);
        }

        // Feed 55 samples at amplitude=1.0 (above the null threshold of 0.5 * s_level).
        // The null condition (current_strength/50 ≤ 0.5) is never met while amplitude=1.0.
        let mut got_false_sync = false;
        for _ in 0..(5 * 11) {
            match state.detect_null(1.0, 1.0) {
                NullState::Timeout => {
                    if state.false_sync_pending() {
                        got_false_sync = true;
                        break;
                    }
                }
                _ => {}
            }
        }
        assert!(
            got_false_sync,
            "false_sync_pending should be true after 5 consecutive timeouts"
        );
    }

    // ── reset ─────────────────────────────────────────────────────────────────

    /// `reset` must zero the sliding-window strength and counter so that the
    /// next acquisition attempt starts from a clean state.
    #[test]
    fn reset_clears_strength_and_counter() {
        let mut state = SyncState::new(200_000, 2_656);
        for _ in 0..SLIDING_WINDOW {
            state.prefill(1.0);
        }
        // current_strength is now 50.0; reset should drop it back to 0.
        state.reset();
        assert_eq!(
            state.current_strength, 0.0,
            "current_strength must be zero after reset"
        );
        assert_eq!(state.counter, 0, "counter must be zero after reset");
    }

    /// After `reset`, the sliding window is empty; the phase must be
    /// `SeekingNull` so the first `detect_null` call looks for a null symbol.
    #[test]
    fn reset_restores_seeking_null_phase() {
        let mut state = SyncState::new(200_000, 2_656);
        for _ in 0..SLIDING_WINDOW {
            state.prefill(1.0);
        }
        // Manually drive to SeekingEndOfNull.
        for _ in 0..60 {
            if matches!(state.detect_null(0.0, 1.0), NullState::NullFound) {
                break;
            }
        }
        state.reset();
        // After reset, prefill with a strong signal so the window is full.
        for _ in 0..SLIDING_WINDOW {
            state.prefill(2.0);
        }
        // A strong signal (current_strength/50 = 2.0 >> 0.5 × s_level = 0.5)
        // must NOT trigger NullFound — the state machine is correctly in SeekingNull.
        assert_eq!(
            state.detect_null(2.0, 1.0),
            NullState::Searching,
            "state after reset+prefill must be SeekingNull (Searching on strong signal)"
        );
    }

    // ── prefill ───────────────────────────────────────────────────────────────

    /// `prefill` must accumulate amplitude into `current_strength`.
    #[test]
    fn prefill_accumulates_strength() {
        let mut state = SyncState::new(200_000, 2_656);
        for _ in 0..SLIDING_WINDOW {
            state.prefill(2.0);
        }
        // After 50 prefill calls with amplitude 2.0: current_strength = 100.0.
        assert!(
            (state.current_strength - 100.0).abs() < 1e-3,
            "current_strength after 50×2.0 prefill should be ≈100.0, got {}",
            state.current_strength
        );
    }

    // ── false_sync_pending ────────────────────────────────────────────────────

    /// `false_sync_pending` must return `true` exactly once after the 5th
    /// consecutive timeout and then revert to `false` (consuming the flag).
    #[test]
    fn false_sync_pending_is_consumed_after_read() {
        let mut state = SyncState::new(10, 20);
        for _ in 0..SLIDING_WINDOW {
            state.prefill(1.0);
        }
        // Drive 5 × (t_f+1) timeouts.
        for _ in 0..(5 * 11) {
            state.detect_null(1.0, 1.0);
        }
        let first = state.false_sync_pending();
        let second = state.false_sync_pending();
        assert!(first, "flag must be true on first read after 5 timeouts");
        assert!(!second, "flag must be false (consumed) on second read");
    }

    // ── end-of-null timeout ───────────────────────────────────────────────────

    /// When the signal stays silent after a NullFound, the end-of-null counter
    /// must eventually expire and return `Timeout`.
    #[test]
    fn end_of_null_timeout_when_signal_stays_quiet() {
        // t_null=20: end-of-null counter expires after t_null + SLIDING_WINDOW calls.
        let mut state = SyncState::new(200_000, 20);
        for _ in 0..SLIDING_WINDOW {
            state.prefill(1.0);
        }

        // Find NullFound by driving amplitude to zero.
        let mut found_null = false;
        for _ in 0..200 {
            if matches!(state.detect_null(0.0, 1.0), NullState::NullFound) {
                found_null = true;
                break;
            }
        }
        assert!(found_null, "NullFound must fire first");

        // Keep feeding zero amplitude; eventually the end-of-null timer expires.
        let mut got_timeout = false;
        for _ in 0..(20 + SLIDING_WINDOW + 10) {
            if matches!(state.detect_null(0.0, 1.0), NullState::Timeout) {
                got_timeout = true;
                break;
            }
        }
        assert!(
            got_timeout,
            "Timeout must fire when end-of-null window is exhausted"
        );
    }
}
