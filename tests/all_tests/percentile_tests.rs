use super::*;

#[test]
fn percentile95_returns_zero_for_empty_input() {
    assert_eq!(percentile95(&[]), 0);
}

#[test]
fn percentile95_returns_value_for_single_sample() {
    assert_eq!(percentile95(&[7]), 7);
}

#[test]
fn percentile95_works_on_unsorted_samples() {
    let samples = [4, 1, 8, 2, 16, 3, 9, 10, 5, 12];
    assert_eq!(percentile95(&samples), 16);
}

#[test]
fn percentile95_handles_duplicate_values() {
    let samples = [0, 0, 0, 1, 1, 1, 1, 1, 1, 1];
    assert_eq!(percentile95(&samples), 1);
}

#[test]
fn percentile95_from_histogram_matches_vector_strategy() {
    let samples = [0u32, 2, 1, 3, 3, 7, 8, 8, 12, 64, 64, 64];
    let mut hist = [0u32; ETI_LATE_SLOTS_HIST_MAX + 1];
    for sample in samples {
        let bin = (sample as usize).min(ETI_LATE_SLOTS_HIST_MAX);
        hist[bin] += 1;
    }

    assert_eq!(
        percentile95_from_histogram(&hist, samples.len() as u64),
        percentile95(&samples)
    );
}

#[test]
fn percentile95_from_histogram_returns_zero_for_empty_total() {
    let hist = [0u32; ETI_LATE_SLOTS_HIST_MAX + 1];
    assert_eq!(percentile95_from_histogram(&hist, 0), 0);
}
