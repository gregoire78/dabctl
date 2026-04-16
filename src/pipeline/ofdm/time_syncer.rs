/// DABstar-style time synchroniser for null-symbol detection.
///
/// Uses the same direct sliding-window level test as DABstar around the DAB
/// null symbol boundary defined in ETSI EN 300 401 §14.8.1.
///
/// - null start when the short-term mean falls below $0.55 \times s\_level$
/// - null end when it rises above $0.75 \times s\_level$

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeSyncState {
    TimeSyncEstablished,
    NoDipFound,
    NoEndOfDipFound,
}

const SYNC_BUFFER_SIZE: usize = 4096;
const LEVEL_SEARCH_SIZE: usize = 50;
const NULL_DIP_START_RATIO: f32 = 0.55;
const NULL_DIP_END_RATIO: f32 = 0.75;

pub struct TimeSyncer {
    env_buffer: Vec<f32>,
    sync_buffer_mask: usize,
    sync_buffer_index: usize,
    level_search_size: usize,
}

impl Default for TimeSyncer {
    fn default() -> Self {
        Self::new(SYNC_BUFFER_SIZE, LEVEL_SEARCH_SIZE)
    }
}

impl TimeSyncer {
    pub fn new(sync_buffer_size: usize, level_search_size: usize) -> Self {
        assert!(sync_buffer_size.is_power_of_two());
        assert!(level_search_size > 0);
        assert!(level_search_size <= sync_buffer_size);

        Self {
            env_buffer: vec![0.0; sync_buffer_size],
            sync_buffer_mask: sync_buffer_size - 1,
            sync_buffer_index: 0,
            level_search_size,
        }
    }

    fn push_level(&mut self, level: f32, current_strength: &mut f32) {
        self.env_buffer[self.sync_buffer_index] = level;
        let old_idx = (self.sync_buffer_index + self.env_buffer.len() - self.level_search_size)
            & self.sync_buffer_mask;
        *current_strength += self.env_buffer[self.sync_buffer_index] - self.env_buffer[old_idx];
        self.sync_buffer_index = (self.sync_buffer_index + 1) & self.sync_buffer_mask;
    }

    #[inline]
    fn average_level(&self, current_strength: f32) -> f32 {
        current_strength / self.level_search_size as f32
    }

    fn prime_window<F>(&mut self, mut next_level: F) -> Option<f32>
    where
        F: FnMut() -> Option<f32>,
    {
        let mut current_strength = 0.0f32;
        self.sync_buffer_index = 0;

        for _ in 0..self.level_search_size {
            let sample = next_level()?;
            self.env_buffer[self.sync_buffer_index] = sample;
            current_strength += sample;
            self.sync_buffer_index = (self.sync_buffer_index + 1) & self.sync_buffer_mask;
        }

        Some(current_strength)
    }

    pub fn read_samples_until_end_of_level_drop<F>(
        &mut self,
        s_level: f32,
        t_f: usize,
        t_null: usize,
        mut next_level: F,
    ) -> Option<TimeSyncState>
    where
        F: FnMut() -> Option<f32>,
    {
        let mut current_strength = self.prime_window(&mut next_level)?;

        let mut counter = 0usize;
        while self.average_level(current_strength) > NULL_DIP_START_RATIO * s_level {
            let sample = next_level()?;
            self.push_level(sample, &mut current_strength);
            counter += 1;
            if counter > t_f {
                return Some(TimeSyncState::NoDipFound);
            }
        }

        counter = 0;
        while self.average_level(current_strength) < NULL_DIP_END_RATIO * s_level {
            let sample = next_level()?;
            self.push_level(sample, &mut current_strength);
            counter += 1;
            if counter > t_null + self.level_search_size {
                return Some(TimeSyncState::NoEndOfDipFound);
            }
        }

        Some(TimeSyncState::TimeSyncEstablished)
    }
}

#[cfg(test)]
mod tests {
    use super::{TimeSyncState, TimeSyncer};

    #[test]
    fn time_syncer_detects_null_drop_and_end() {
        let mut syncer = TimeSyncer::new(4096, 50);
        let mut levels = vec![1.0f32; 64];
        levels.extend(std::iter::repeat_n(0.1f32, 40));
        levels.extend(std::iter::repeat_n(1.0f32, 64));
        let mut iter = levels.into_iter();

        let state = syncer.read_samples_until_end_of_level_drop(1.0, 200, 80, || iter.next());

        assert_eq!(state, Some(TimeSyncState::TimeSyncEstablished));
    }

    #[test]
    fn time_syncer_reports_no_dip_found() {
        let mut syncer = TimeSyncer::new(4096, 50);
        let levels = vec![1.0f32; 256];
        let mut iter = levels.into_iter();

        let state = syncer.read_samples_until_end_of_level_drop(1.0, 64, 80, || iter.next());

        assert_eq!(state, Some(TimeSyncState::NoDipFound));
    }

    #[test]
    fn time_syncer_reports_no_end_of_dip_found() {
        let mut syncer = TimeSyncer::new(4096, 50);
        let mut levels = vec![1.0f32; 64];
        levels.extend(std::iter::repeat_n(0.1f32, 256));
        let mut iter = levels.into_iter();

        let state = syncer.read_samples_until_end_of_level_drop(1.0, 200, 40, || iter.next());

        assert_eq!(state, Some(TimeSyncState::NoEndOfDipFound));
    }

    #[test]
    fn time_syncer_uses_direct_threshold_crossing_like_dabstar() {
        let mut syncer = TimeSyncer::new(4096, 50);
        let mut levels = vec![1.0f32; 64];
        levels.extend(std::iter::repeat_n(0.0f32, 24));
        levels.extend(std::iter::repeat_n(1.0f32, 128));
        let mut iter = levels.into_iter();

        let state = syncer.read_samples_until_end_of_level_drop(1.0, 100, 80, || iter.next());

        assert_eq!(state, Some(TimeSyncState::TimeSyncEstablished));
    }
}
