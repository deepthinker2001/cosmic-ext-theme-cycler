//! Learning when the user habitually switches between light and dark.
//!
//! Each manual choice is recorded as minutes relative to the day's sun time:
//! after sunrise for going light, after sunset for going dark (negative
//! means before). Choices more than six hours from the sun time are not
//! recorded. Nothing in the applet records them yet; that wiring is a
//! follow-up. From a bounded history, [`typical_offset`] derives a
//! recency-weighted median per variant; with no history the offset is 0, so
//! the switch happens at the sun time itself.
//!
//! Pure functions only: the caller supplies the sun times.

use serde::{Deserialize, Serialize};

use crate::Variant;

/// The most records kept; older ones are dropped.
pub const MAX_HISTORY: usize = 64;

/// Minutes in a day; offsets are wrapped around it.
const DAY_MINUTES: i64 = 1440;

/// A choice more than this many minutes (6 hours) either side of the sun
/// time is not a habit tied to it (a stray 3 a.m. switch, say) and is not
/// recorded; stored records beyond it are dropped on load.
pub const MAX_OFFSET_MINUTES: i32 = 360;

/// One manual choice.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SwitchRecord {
    /// What the user switched to.
    pub variant: Variant,
    /// Minutes after sunrise (for light) or sunset (for dark); negative is
    /// before.
    pub offset_minutes: i32,
}

/// Build a record from the time of the choice and the sun time it is
/// measured from, both in minutes on one common clock (since midnight or
/// since the epoch). Light is measured from `sunrise`, dark from `sunset`.
/// The difference wraps around the day into (-720, 720], so a choice at
/// 00:30 against a sunset at 23:50 is +40. `None` when the offset is
/// beyond ±[`MAX_OFFSET_MINUTES`]: such a choice is no habit.
#[must_use]
pub fn record_for(variant: Variant, now: i64, sunrise: i64, sunset: i64) -> Option<SwitchRecord> {
    let reference = match variant {
        Variant::Light => sunrise,
        Variant::Dark => sunset,
    };
    let mut wrapped = now.saturating_sub(reference).rem_euclid(DAY_MINUTES);
    if wrapped > DAY_MINUTES / 2 {
        wrapped -= DAY_MINUTES;
    }
    let offset_minutes = i32::try_from(wrapped).ok()?;
    in_range(offset_minutes).then_some(SwitchRecord {
        variant,
        offset_minutes,
    })
}

fn in_range(offset_minutes: i32) -> bool {
    (-MAX_OFFSET_MINUTES..=MAX_OFFSET_MINUTES).contains(&offset_minutes)
}

/// Append a record, dropping the oldest ones beyond [`MAX_HISTORY`]. A
/// record beyond ±[`MAX_OFFSET_MINUTES`] is ignored.
pub fn push(history: &mut Vec<SwitchRecord>, record: SwitchRecord) {
    if !in_range(record.offset_minutes) {
        return;
    }
    history.push(record);
    if history.len() > MAX_HISTORY {
        let excess = history.len() - MAX_HISTORY;
        history.drain(..excess);
    }
}

/// The history within bounds: out-of-range records dropped, the newest
/// [`MAX_HISTORY`] kept.
#[must_use]
pub fn clamp_history(history: &[SwitchRecord]) -> Vec<SwitchRecord> {
    let valid: Vec<SwitchRecord> = history
        .iter()
        .copied()
        .filter(|record| in_range(record.offset_minutes))
        .collect();
    let start = valid.len().saturating_sub(MAX_HISTORY);
    valid[start..].to_vec()
}

/// The user's typical offset in minutes for switching to `variant`: the
/// weighted median of that variant's records, where the newest record
/// weighs most (weight = its age rank, 1 for the oldest of that variant).
/// `0` when there are none.
#[must_use]
pub fn typical_offset(history: &[SwitchRecord], variant: Variant) -> i32 {
    let mut weighted: Vec<(i32, u64)> = history
        .iter()
        .filter(|record| record.variant == variant)
        .zip(1_u64..)
        .map(|(record, weight)| (record.offset_minutes, weight))
        .collect();
    if weighted.is_empty() {
        return 0;
    }
    weighted.sort_by_key(|&(offset, _)| offset);
    let total: u64 = weighted.iter().map(|&(_, weight)| weight).sum();
    let mut below = 0;
    for &(offset, weight) in &weighted {
        below += weight;
        // First offset where at least half the weight is at or below it.
        if below * 2 >= total {
            return offset;
        }
    }
    0 // unreachable: the running sum reaches the total
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(variant: Variant, offset_minutes: i32) -> SwitchRecord {
        SwitchRecord {
            variant,
            offset_minutes,
        }
    }

    #[test]
    fn no_history_means_the_sun_time_itself() {
        assert_eq!(typical_offset(&[], Variant::Light), 0);
        let only_dark = [rec(Variant::Dark, 30)];
        assert_eq!(typical_offset(&only_dark, Variant::Light), 0);
    }

    #[test]
    fn a_single_record_is_the_offset() {
        let history = [rec(Variant::Light, -20)];
        assert_eq!(typical_offset(&history, Variant::Light), -20);
    }

    #[test]
    fn variants_are_kept_apart() {
        let history = [rec(Variant::Light, 10), rec(Variant::Dark, 90)];
        assert_eq!(typical_offset(&history, Variant::Light), 10);
        assert_eq!(typical_offset(&history, Variant::Dark), 90);
    }

    #[test]
    fn recent_records_outweigh_old_ones() {
        // Weights 1,2,3: offsets 0 (old), 0, 60 (new) -> total 6, below 0 = 3.
        let history = [
            rec(Variant::Dark, 0),
            rec(Variant::Dark, 0),
            rec(Variant::Dark, 60),
        ];
        assert_eq!(typical_offset(&history, Variant::Dark), 0);
        // Newest two at 60: weights 1,2,3 -> 60 holds 5 of 6.
        let history = [
            rec(Variant::Dark, 0),
            rec(Variant::Dark, 60),
            rec(Variant::Dark, 60),
        ];
        assert_eq!(typical_offset(&history, Variant::Dark), 60);
    }

    #[test]
    fn an_outlier_does_not_move_the_median() {
        let history = [
            rec(Variant::Light, 10),
            rec(Variant::Light, 300),
            rec(Variant::Light, 12),
            rec(Variant::Light, 11),
            rec(Variant::Light, 10),
        ];
        assert_eq!(typical_offset(&history, Variant::Light), 11);
    }

    #[test]
    fn records_are_measured_from_the_right_sun_time() {
        assert_eq!(
            record_for(Variant::Light, 460, 420, 1200),
            Some(rec(Variant::Light, 40))
        );
        assert_eq!(
            record_for(Variant::Dark, 1180, 420, 1200),
            Some(rec(Variant::Dark, -20))
        );
    }

    #[test]
    fn offsets_wrap_around_midnight() {
        // 00:30 against a sunset at 23:50.
        assert_eq!(
            record_for(Variant::Dark, 30, 420, 1430),
            Some(rec(Variant::Dark, 40))
        );
        // 23:50 against a sunrise at 00:10.
        assert_eq!(
            record_for(Variant::Light, 1430, 10, 1200),
            Some(rec(Variant::Light, -20))
        );
    }

    #[test]
    fn strays_and_extremes_are_not_recorded() {
        // 3 a.m. against a sunset at 18:00 is +9h.
        assert_eq!(record_for(Variant::Dark, 180, 420, 1080), None);
        // Extreme inputs saturate instead of overflowing.
        let _ = record_for(Variant::Dark, i64::MAX, 0, i64::MIN);
        let mut history = Vec::new();
        push(&mut history, rec(Variant::Dark, 400));
        push(&mut history, rec(Variant::Dark, i32::MIN));
        assert_eq!(history, Vec::new());
    }

    #[test]
    fn history_is_bounded_keeping_the_newest() {
        let mut history = Vec::new();
        for n in 0..i32::try_from(MAX_HISTORY).unwrap() + 5 {
            push(&mut history, rec(Variant::Light, n));
        }
        assert_eq!(history.len(), MAX_HISTORY);
        assert_eq!(history[0].offset_minutes, 5);
        let mut long: Vec<_> = (0..200).map(|n| rec(Variant::Dark, n)).collect();
        long.push(rec(Variant::Dark, 5000));
        assert_eq!(clamp_history(&[rec(Variant::Dark, i32::MIN)]), Vec::new());
        let clamped = clamp_history(&long);
        assert_eq!(clamped.len(), MAX_HISTORY);
        assert!(
            clamped
                .iter()
                .all(|r| r.offset_minutes.abs() <= MAX_OFFSET_MINUTES)
        );
        assert_eq!(clamp_history(&clamped), clamped);
    }
}
