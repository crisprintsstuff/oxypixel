//! Frame clock shared by the editor UI and tests.
//!
//! Playback does not mutate the document. The caller stores the frame index
//! and the millisecond remainder.

/// Advance an animation clock.
///
/// `durations` is one entry per frame, in milliseconds. The clock loops.
/// A huge `dt` is capped at four full cycles so a stalled UI cannot spin.
pub fn advance(index: usize, durations: &[u32], accumulator: u32, dt: u32) -> (usize, u32) {
    if durations.is_empty() {
        return (0, 0);
    }
    let mut index = index.min(durations.len() - 1);
    let mut left = u64::from(dt);
    let mut acc = u64::from(accumulator);
    let cap = durations.len().saturating_mul(4).max(1);
    for _ in 0..cap {
        let dur = u64::from(durations[index].max(1));
        if acc + left < dur {
            return (index, (acc + left) as u32);
        }
        let step = dur - acc;
        left -= step;
        acc = 0;
        index = (index + 1) % durations.len();
        if left == 0 {
            return (index, 0);
        }
    }
    (index, 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn advance_loops_and_keeps_the_remainder() {
        let d = [100, 100];
        assert_eq!(advance(0, &d, 0, 0), (0, 0));
        assert_eq!(advance(0, &d, 0, 100), (1, 0));
        assert_eq!(advance(0, &d, 40, 60), (1, 0));
        assert_eq!(advance(0, &d, 0, 250), (0, 50));
        assert_eq!(advance(1, &d, 0, 50), (1, 50));
    }

    #[test]
    fn empty_clock_is_idle() {
        assert_eq!(advance(3, &[], 10, 10), (0, 0));
    }
}
