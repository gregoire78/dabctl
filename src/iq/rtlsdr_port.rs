#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GainSelection {
    pub set_index: usize,
    pub effective_index: usize,
    pub set_gain_tenth_db: i32,
    pub effective_gain_tenth_db: i32,
}

/// Ported from eti-cmdline RTL gain behavior.
///
/// eti-cmdline uses two index formulas:
/// - set index: p * gainsCount / 100 (then clamped)
/// - effective index: p * (gainsCount - 1) / 100
pub fn eti_cmdline_gain_selection(gains: &[i32], percent: u32) -> Option<GainSelection> {
    if gains.is_empty() {
        return None;
    }

    let p = percent.min(100) as usize;
    let count = gains.len();

    let mut set_index = p * count / 100;
    if set_index >= count {
        set_index = count - 1;
    }

    let effective_index = p * (count - 1) / 100;

    Some(GainSelection {
        set_index,
        effective_index,
        set_gain_tenth_db: gains[set_index],
        effective_gain_tenth_db: gains[effective_index],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gain_selection_empty_table() {
        assert!(eti_cmdline_gain_selection(&[], 50).is_none());
    }

    #[test]
    fn gain_selection_zero_percent_first_entry() {
        let gains = [0, 10, 20, 30];
        let sel = eti_cmdline_gain_selection(&gains, 0).expect("selection");
        assert_eq!(sel.set_index, 0);
        assert_eq!(sel.effective_index, 0);
        assert_eq!(sel.set_gain_tenth_db, 0);
        assert_eq!(sel.effective_gain_tenth_db, 0);
    }

    #[test]
    fn gain_selection_hundred_percent_last_entry() {
        let gains = [0, 10, 20, 30];
        let sel = eti_cmdline_gain_selection(&gains, 100).expect("selection");
        assert_eq!(sel.set_index, 3);
        assert_eq!(sel.effective_index, 3);
        assert_eq!(sel.set_gain_tenth_db, 30);
    }

    #[test]
    fn gain_selection_over_hundred_clamped() {
        let gains = [0, 10, 20, 30];
        let sel = eti_cmdline_gain_selection(&gains, 250).expect("selection");
        assert_eq!(sel.set_index, 3);
        assert_eq!(sel.effective_index, 3);
    }

    #[test]
    fn gain_selection_midpoint_matches_port_formula() {
        let gains = [0, 14, 20, 29, 37, 43, 49];
        let sel = eti_cmdline_gain_selection(&gains, 50).expect("selection");
        assert_eq!(sel.set_index, 3);
        assert_eq!(sel.effective_index, 3);
        assert_eq!(sel.set_gain_tenth_db, 29);
    }

    #[test]
    fn gain_selection_set_and_effective_can_differ() {
        let gains = [0, 1, 2];
        let sel = eti_cmdline_gain_selection(&gains, 67).expect("selection");
        assert_eq!(sel.set_index, 2);
        assert_eq!(sel.effective_index, 1);
        assert_eq!(sel.set_gain_tenth_db, 2);
        assert_eq!(sel.effective_gain_tenth_db, 1);
    }

    #[test]
    fn gain_selection_monotonic_set_gain() {
        let gains = [0, 10, 20, 30, 40, 50];
        let mut last = i32::MIN;
        for p in 0..=100 {
            let sel = eti_cmdline_gain_selection(&gains, p).expect("selection");
            assert!(sel.set_gain_tenth_db >= last);
            last = sel.set_gain_tenth_db;
        }
    }

    #[test]
    fn gain_selection_monotonic_effective_gain() {
        let gains = [0, 10, 20, 30, 40, 50];
        let mut last = i32::MIN;
        for p in 0..=100 {
            let sel = eti_cmdline_gain_selection(&gains, p).expect("selection");
            assert!(sel.effective_gain_tenth_db >= last);
            last = sel.effective_gain_tenth_db;
        }
    }
}
