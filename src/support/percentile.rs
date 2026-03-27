/// Percentile computation utilities for ETI frame pacing statistics.
/// Reference implementation of percentile95 for testing purposes.
/// Used to validate the optimized histogram-based version.
#[allow(dead_code)]
pub fn percentile95(samples: &[u32]) -> u32 {
    if samples.is_empty() {
        return 0;
    }

    let mut sorted = samples.to_vec();
    sorted.sort_unstable();
    let idx = ((sorted.len() * 95).saturating_sub(1)) / 100;
    sorted[idx.min(sorted.len() - 1)]
}

/// Compute 95th percentile from a histogram of slot delays.
/// The histogram is indexed by slot count, with all values >= histogram.len() 
/// lumped into the final bucket.
pub fn percentile95_from_histogram(hist: &[u32], total_count: u64) -> u32 {
    if total_count == 0 {
        return 0;
    }

    // Position de percentile identique à percentile95(samples).
    let rank = (((total_count as usize) * 95).saturating_sub(1)) / 100;
    let mut cumulative = 0usize;

    for (value, count) in hist.iter().enumerate() {
        cumulative += *count as usize;
        if cumulative > rank {
            return value as u32;
        }
    }

    (hist.len().saturating_sub(1)) as u32
}
