//! Sunrise and sunset, and a smooth day curve built on them.
//!
//! Pure arithmetic, no clock and no I/O: the caller passes the place and the
//! date, and gets times back as minutes. The sunrise and sunset use the NOAA
//! solar calculator's equations (accurate to about a minute away from the
//! poles).

use std::f64::consts::PI;

/// Minutes in a day.
const DAY: f64 = 1440.0;
/// The sun's zenith angle at sunrise/sunset: 90° plus refraction and the
/// sun's radius.
const ZENITH_DEG: f64 = 90.833;

/// When the sun rises and sets on one date at one place.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SunTimes {
    /// Minutes after 00:00 UTC of the given date. Either may fall outside
    /// 0..1440 (for places far from the Greenwich meridian), meaning the
    /// previous or the next UTC day.
    Normal { sunrise: f64, sunset: f64 },
    /// The sun stays up all day.
    PolarDay,
    /// The sun stays down all day.
    PolarNight,
}

/// Days in `month` (1..=12) of `year`.
fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        2 if (year % 4 == 0 && year % 100 != 0) || year % 400 == 0 => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// Days since 1970-01-01 of a proleptic Gregorian date.
fn days_from_civil(year: i32, month: u32, day: u32) -> i64 {
    let y = i64::from(year) - i64::from(month <= 2);
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let m = i64::from(month);
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + i64::from(day) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Sunrise and sunset for `latitude`/`longitude` (degrees, north and east
/// positive) on the civil date `year-month-day`. `None` for an invalid
/// place or date.
#[must_use]
// The NOAA coefficients are written as published, not regrouped.
#[allow(
    clippy::many_single_char_names,
    clippy::unreadable_literal,
    clippy::inconsistent_digit_grouping
)]
pub fn sun_times(
    latitude: f64,
    longitude: f64,
    year: i32,
    month: u32,
    day: u32,
) -> Option<SunTimes> {
    if !(-90.0..=90.0).contains(&latitude)
        || !(-180.0..=180.0).contains(&longitude)
        || !(1..=12).contains(&month)
        || day < 1
        || day > days_in_month(year, month)
    {
        return None;
    }
    // Julian day of 12:00 UTC on the date. Day counts are far below 2^52, so
    // the conversion is exact.
    #[allow(clippy::cast_precision_loss)]
    let jd = days_from_civil(year, month, day) as f64 + 2_440_587.5 + 0.5;
    let t = (jd - 2_451_545.0) / 36_525.0;

    let l0 = (280.46646 + t * (36_000.76983 + t * 0.0003032)).rem_euclid(360.0);
    let m = (357.52911 + t * (35_999.05029 - 0.0001537 * t)).to_radians();
    let e = 0.016_708_634 - t * (0.000_042_037 + 0.000_000_1267 * t);
    let c = m.sin() * (1.914_602 - t * (0.004_817 + 0.000_014 * t))
        + (2.0 * m).sin() * (0.019_993 - 0.000_101 * t)
        + (3.0 * m).sin() * 0.000_289;
    let omega = (125.04 - 1934.136 * t).to_radians();
    let app_long = (l0 + c - 0.00569 - 0.00478 * omega.sin()).to_radians();
    let mean_obliq =
        23.0 + (26.0 + (21.448 - t * (46.815 + t * (0.00059 - t * 0.001_813))) / 60.0) / 60.0;
    let obliq = (mean_obliq + 0.00256 * omega.cos()).to_radians();
    let decl = (obliq.sin() * app_long.sin()).asin();

    let y = (obliq / 2.0).tan().powi(2);
    let l0r = l0.to_radians();
    let eq_time = 4.0
        * (y * (2.0 * l0r).sin() - 2.0 * e * m.sin() + 4.0 * e * y * m.sin() * (2.0 * l0r).cos()
            - 0.5 * y * y * (4.0 * l0r).sin()
            - 1.25 * e * e * (2.0 * m).sin())
        .to_degrees();

    let lat = latitude.to_radians();
    let cos_ha = ZENITH_DEG.to_radians().cos() / (lat.cos() * decl.cos()) - lat.tan() * decl.tan();
    if cos_ha.is_nan() {
        return None;
    }
    if cos_ha > 1.0 {
        return Some(SunTimes::PolarNight);
    }
    if cos_ha < -1.0 {
        return Some(SunTimes::PolarDay);
    }
    let ha = cos_ha.acos().to_degrees();
    Some(SunTimes::Normal {
        sunrise: 720.0 - 4.0 * (longitude + ha) - eq_time,
        sunset: 720.0 - 4.0 * (longitude - ha) - eq_time,
    })
}

/// A smooth 0..1 "lightness" over the day: 0 at night, 1 by day, with a
/// half-cosine ramp around each of sunrise and sunset.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DayCurve {
    /// Always 1 (polar day).
    Always,
    /// Always 0 (polar night).
    Never,
    /// Ramp up centred on `rise`, down centred on `set` (minutes on one
    /// continuous axis, `set > rise`), each ramp `ramp` minutes long.
    Cycle { rise: f64, set: f64, ramp: f64 },
}

impl DayCurve {
    /// Build the curve for `times`. `rise_shift` and `set_shift` move the
    /// middle of each ramp, in minutes (negative is earlier). `ramp` is the
    /// length of each ramp in minutes (clamped to at least 1).
    ///
    /// Inputs that aren't finite are tolerated: a non-finite shift counts as
    /// 0, shifts are limited to ±1 day and the ramp to 1 minute..1 day.
    #[must_use]
    pub fn new(times: SunTimes, rise_shift: f64, set_shift: f64, ramp: f64) -> Self {
        let shift = |s: f64| {
            if s.is_finite() {
                s.clamp(-DAY, DAY)
            } else {
                0.0
            }
        };
        match times {
            SunTimes::PolarDay => Self::Always,
            SunTimes::PolarNight => Self::Never,
            SunTimes::Normal { sunrise, sunset } => {
                let rise = sunrise + shift(rise_shift);
                // Keep the set after the rise, and less than a day later.
                let set = rise + (sunset + shift(set_shift) - rise).rem_euclid(DAY);
                let ramp = if ramp.is_finite() {
                    ramp.clamp(1.0, DAY)
                } else {
                    1.0
                };
                Self::Cycle { rise, set, ramp }
            }
        }
    }

    /// Lightness, always within 0..=1, at `minute` (same axis as the sun
    /// times; any day). A non-finite `minute` gives 0.
    #[must_use]
    pub fn lightness(&self, minute: f64) -> f64 {
        match *self {
            Self::Always => 1.0,
            Self::Never => 0.0,
            Self::Cycle { rise, set, ramp } => {
                if !minute.is_finite() {
                    return 0.0;
                }
                // Put `minute` in the one day that starts at `rise`.
                let t = rise + (minute - rise).rem_euclid(DAY);
                let up = Self::step((t - rise) / ramp + 0.5);
                let down = Self::step((set - t) / ramp + 0.5);
                up * down
            }
        }
    }

    /// Half-cosine ease from 0 (x <= 0) to 1 (x >= 1).
    fn step(x: f64) -> f64 {
        0.5 - 0.5 * (PI * x.clamp(0.0, 1.0)).cos()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn normal(t: SunTimes) -> (f64, f64) {
        match t {
            SunTimes::Normal { sunrise, sunset } => (sunrise, sunset),
            other => panic!("expected a normal day, got {other:?}"),
        }
    }

    #[test]
    fn days_from_civil_known_dates() {
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(days_from_civil(2000, 3, 1), 11_017);
    }

    #[test]
    fn equinox_at_greenwich_is_about_six_and_eighteen() {
        let (rise, set) = normal(sun_times(0.0, 0.0, 2024, 3, 20).unwrap());
        assert!((rise - 6.0 * 60.0).abs() < 15.0, "rise {rise}");
        assert!((set - 18.0 * 60.0).abs() < 15.0, "set {set}");
    }

    #[test]
    fn london_midsummer_matches_almanac() {
        // 2024-06-21 London: sunrise 03:43 UTC, sunset 20:21 UTC.
        let (rise, set) = normal(sun_times(51.5074, -0.1278, 2024, 6, 21).unwrap());
        assert!((rise - (3.0 * 60.0 + 43.0)).abs() < 3.0, "rise {rise}");
        assert!((set - (20.0 * 60.0 + 21.0)).abs() < 3.0, "set {set}");
    }

    #[test]
    fn polar_day_and_night() {
        assert_eq!(sun_times(78.2, 15.6, 2024, 6, 21), Some(SunTimes::PolarDay));
        assert_eq!(
            sun_times(78.2, 15.6, 2024, 12, 21),
            Some(SunTimes::PolarNight)
        );
        assert_eq!(
            sun_times(-78.2, 15.6, 2024, 12, 21),
            Some(SunTimes::PolarDay)
        );
        assert_eq!(
            sun_times(-78.2, 15.6, 2024, 6, 21),
            Some(SunTimes::PolarNight)
        );
    }

    #[test]
    fn invalid_input_is_none() {
        assert_eq!(sun_times(91.0, 0.0, 2024, 1, 1), None);
        assert_eq!(sun_times(0.0, 181.0, 2024, 1, 1), None);
        assert_eq!(sun_times(0.0, 0.0, 2024, 13, 1), None);
        assert_eq!(sun_times(f64::NAN, 0.0, 2024, 1, 1), None);
        assert_eq!(sun_times(0.0, 0.0, 2023, 2, 29), None);
        assert!(sun_times(0.0, 0.0, 2024, 2, 29).is_some());
        assert_eq!(sun_times(0.0, 0.0, 2024, 4, 31), None);
        assert_eq!(sun_times(0.0, 0.0, 1900, 2, 29), None);
        assert!(sun_times(0.0, 0.0, 2000, 2, 29).is_some());
    }

    #[test]
    fn non_finite_inputs_still_give_values_in_range() {
        let times = SunTimes::Normal {
            sunrise: 360.0,
            sunset: 1080.0,
        };
        for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, 1e300] {
            let c = DayCurve::new(times, bad, bad, bad);
            for m in [0.0, 360.0, 720.0, bad] {
                let v = c.lightness(m);
                assert!((0.0..=1.0).contains(&v), "{bad} {m} {v}");
            }
        }
    }

    #[test]
    fn sunrise_equal_to_sunset_stays_in_range() {
        let c = DayCurve::new(
            SunTimes::Normal {
                sunrise: 360.0,
                sunset: 360.0,
            },
            0.0,
            0.0,
            60.0,
        );
        for m in 0..1440 {
            assert!((0.0..=1.0).contains(&c.lightness(f64::from(m))));
        }
    }

    #[test]
    fn curve_is_dark_at_night_light_by_day_half_at_sunrise() {
        let times = SunTimes::Normal {
            sunrise: 360.0,
            sunset: 1080.0,
        };
        let c = DayCurve::new(times, 0.0, 0.0, 60.0);
        assert!(c.lightness(0.0) < 1e-9);
        assert!((c.lightness(720.0) - 1.0).abs() < 1e-9);
        assert!((c.lightness(360.0) - 0.5).abs() < 1e-9);
        assert!((c.lightness(1080.0) - 0.5).abs() < 1e-9);
        assert!(c.lightness(1439.0) < 1e-9);
        // Same a day later.
        assert!((c.lightness(360.0 + DAY) - 0.5).abs() < 1e-9);
    }

    #[test]
    fn curve_is_monotonic_through_sunrise() {
        let c = DayCurve::new(
            SunTimes::Normal {
                sunrise: 360.0,
                sunset: 1080.0,
            },
            0.0,
            0.0,
            90.0,
        );
        let mut last = -1.0;
        for m in (300..=420).step_by(5) {
            let v = c.lightness(f64::from(m));
            assert!(v >= last && (0.0..=1.0).contains(&v));
            last = v;
        }
    }

    #[test]
    fn shifts_move_the_ramps() {
        let times = SunTimes::Normal {
            sunrise: 360.0,
            sunset: 1080.0,
        };
        let c = DayCurve::new(times, -30.0, 45.0, 60.0);
        assert!((c.lightness(330.0) - 0.5).abs() < 1e-9);
        assert!((c.lightness(1125.0) - 0.5).abs() < 1e-9);
    }

    #[test]
    fn sunset_wrapping_past_utc_midnight() {
        // Sunrise 22:00 UTC, sunset 08:00 UTC next day (given as 480).
        let c = DayCurve::new(
            SunTimes::Normal {
                sunrise: 1320.0,
                sunset: 480.0,
            },
            0.0,
            0.0,
            60.0,
        );
        assert!((c.lightness(1320.0) - 0.5).abs() < 1e-9);
        assert!((c.lightness(480.0) - 0.5).abs() < 1e-9);
        assert!((c.lightness(0.0) - 1.0).abs() < 1e-9);
        assert!(c.lightness(900.0) < 1e-9);
    }

    #[test]
    fn polar_curves_are_constant() {
        let day = DayCurve::new(SunTimes::PolarDay, 30.0, 0.0, 60.0);
        let night = DayCurve::new(SunTimes::PolarNight, 0.0, 0.0, 60.0);
        for m in [0.0, 300.0, 720.0, 1200.0] {
            assert_eq!(day.lightness(m), 1.0);
            assert_eq!(night.lightness(m), 0.0);
        }
    }
}
