//! Which of light and dark the time of day calls for.
//!
//! Pure functions: the caller passes the place, the time (minutes since the
//! Unix epoch, UTC) and the user's switch history. Nothing here reads a
//! clock, a file or the network.
//!
//! The place is either the one the user typed in or an estimate from the
//! computer's UTC offset ([`estimate_place`]); the estimate is rough on
//! purpose and never needs the network.

use crate::Variant;
use crate::habit::{self, SwitchRecord};
use crate::solar::{self, DayCurve, SunTimes};

/// Minutes in a day.
const DAY: i64 = 1440;

/// Length of each light/dark ramp of the day curve, in minutes. The switch
/// happens at the middle of a ramp, so this only shapes the curve.
const RAMP_MINUTES: f64 = 30.0;

/// A place on Earth, in degrees (north and east positive).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Place {
    pub latitude: f64,
    pub longitude: f64,
}

impl Place {
    /// From thousandths of a degree, as the settings keep them.
    #[must_use]
    pub fn from_milli((latitude, longitude): (i32, i32)) -> Self {
        Self {
            latitude: f64::from(latitude) / 1000.0,
            longitude: f64::from(longitude) / 1000.0,
        }
    }
}

/// A guess at where the user is from the computer's offset from UTC, for
/// when no place was entered: the longitude where solar noon matches (15
/// degrees per hour, kept within bounds) and a mid-latitude of 40 degrees
/// north, where most people live. Good to an hour or so, which is why the
/// popup lets the user enter a place.
#[must_use]
pub fn estimate_place(utc_offset_minutes: i32) -> Place {
    Place {
        latitude: 40.0,
        longitude: (f64::from(utc_offset_minutes) / 4.0).clamp(-180.0, 180.0),
    }
}

/// Days since 1970-01-01 to a civil date (year, month, day).
fn civil_from_days(days: i64) -> (i32, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    // Within the ranges of the types for any time a clock reports; a wild
    // value only gives a wrong date, which `sun_times` then refuses.
    (
        i32::try_from(year).unwrap_or(0),
        u32::try_from(month).unwrap_or(1),
        u32::try_from(day).unwrap_or(1),
    )
}

/// The sun's times for the UTC day of `now`, and the minute of that day.
fn today(place: Place, now: i64) -> Option<(SunTimes, i64)> {
    let days = now.div_euclid(DAY);
    let (year, month, day) = civil_from_days(days);
    let times = solar::sun_times(place.latitude, place.longitude, year, month, day)?;
    Some((times, now.rem_euclid(DAY)))
}

/// Light or dark at `now` (minutes since the epoch, UTC): the day curve
/// with its sunrise and sunset moved by the user's typical offsets, light
/// where its lightness is at least one half. `None` if the place or time is
/// unusable.
#[must_use]
pub fn wanted_variant(place: Place, now: i64, history: &[SwitchRecord]) -> Option<Variant> {
    let (times, minute) = today(place, now)?;
    let rise_shift = f64::from(habit::typical_offset(history, Variant::Light));
    let set_shift = f64::from(habit::typical_offset(history, Variant::Dark));
    let curve = DayCurve::new(times, rise_shift, set_shift, RAMP_MINUTES);
    // `minute` is at most 1439, exact in an f64.
    #[allow(clippy::cast_precision_loss)]
    let lightness = curve.lightness(minute as f64);
    Some(if lightness >= 0.5 {
        Variant::Light
    } else {
        Variant::Dark
    })
}

/// The record of the user choosing `variant` by hand at `now`, for the
/// learner: minutes from that day's sunrise (light) or sunset (dark). `None`
/// in polar day or night, for an unusable place, or when the choice is too
/// far from the sun time to be a habit.
#[must_use]
pub fn record_choice(place: Place, now: i64, variant: Variant) -> Option<SwitchRecord> {
    let (times, minute) = today(place, now)?;
    let SunTimes::Normal { sunrise, sunset } = times else {
        return None;
    };
    // Whole minutes are all the history keeps; both are within a few days
    // of the day's start, so the conversion is exact.
    #[allow(clippy::cast_possible_truncation)]
    let (sunrise, sunset) = (sunrise.round() as i64, sunset.round() as i64);
    habit::record_for(variant, minute, sunrise, sunset)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Greenwich, where sunrise and sunset fall on the UTC day.
    const GREENWICH: Place = Place {
        latitude: 51.48,
        longitude: 0.0,
    };

    /// 2024-03-20 00:00 UTC in minutes since the epoch.
    const EQUINOX: i64 = 19_802 * DAY;

    #[test]
    fn dates_round_trip() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(19_802), (2024, 3, 20));
        assert_eq!(civil_from_days(-1), (1969, 12, 31));
        assert_eq!(civil_from_days(11_016), (2000, 2, 29));
    }

    #[test]
    fn day_is_light_and_night_is_dark() {
        assert_eq!(
            wanted_variant(GREENWICH, EQUINOX + 12 * 60, &[]),
            Some(Variant::Light)
        );
        assert_eq!(
            wanted_variant(GREENWICH, EQUINOX + 2 * 60, &[]),
            Some(Variant::Dark)
        );
        assert_eq!(
            wanted_variant(GREENWICH, EQUINOX + 23 * 60, &[]),
            Some(Variant::Dark)
        );
    }

    #[test]
    fn learned_offsets_move_the_switch() {
        // Sunset at Greenwich on the equinox is about 18:10 UTC.
        let late = [SwitchRecord {
            variant: Variant::Dark,
            offset_minutes: 60,
        }];
        let at = EQUINOX + 18 * 60 + 40;
        assert_eq!(wanted_variant(GREENWICH, at, &[]), Some(Variant::Dark));
        assert_eq!(wanted_variant(GREENWICH, at, &late), Some(Variant::Light));
    }

    #[test]
    fn polar_days_and_bad_places() {
        let arctic = Place {
            latitude: 80.0,
            longitude: 0.0,
        };
        let june = EQUINOX + 90 * DAY;
        assert_eq!(wanted_variant(arctic, june, &[]), Some(Variant::Light));
        assert_eq!(record_choice(arctic, june, Variant::Dark), None);
        let nowhere = Place {
            latitude: f64::NAN,
            longitude: 0.0,
        };
        assert_eq!(wanted_variant(nowhere, EQUINOX, &[]), None);
    }

    #[test]
    fn a_manual_choice_is_recorded_relative_to_the_sun() {
        let record = record_choice(GREENWICH, EQUINOX + 18 * 60 + 40, Variant::Dark).unwrap();
        assert_eq!(record.variant, Variant::Dark);
        assert!((20..=40).contains(&record.offset_minutes), "{record:?}");
        assert_eq!(
            record_choice(GREENWICH, EQUINOX + 14 * 60, Variant::Light),
            None,
            "far from sunrise"
        );
    }

    #[test]
    fn the_estimate_follows_the_offset_within_bounds() {
        assert!((estimate_place(-420).longitude + 105.0).abs() < 1e-9);
        assert!(estimate_place(i32::MAX).longitude <= 180.0);
        assert!(estimate_place(i32::MIN).longitude >= -180.0);
    }
}
