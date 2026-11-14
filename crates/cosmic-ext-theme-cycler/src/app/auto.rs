//! Auto: light by day and dark by night. Its state, and the methods of
//! the applet that decide when to switch and take the place typed in.

use cosmic::app::Task;
use cycler_core::Variant;
use cycler_core::schedule::{self, Place};

use super::{Message, ThemeCycler};

/// How often Auto looks at the time of day.
pub(super) const EVERY: std::time::Duration = std::time::Duration::from_secs(60);

/// What Auto remembers between ticks.
#[derive(Debug, Default)]
pub(crate) struct Auto {
    /// The period (light or dark) Auto last acted on, so it applies a theme
    /// once per period and doesn't fight a manual choice made since.
    pub(super) period: Option<Variant>,
    /// The theme being written was applied by Auto, not picked by the user.
    pub(super) applying: bool,
    /// What the popup's latitude and longitude boxes hold while typing.
    pub(crate) latitude_text: String,
    pub(crate) longitude_text: String,
}

impl ThemeCycler {
    /// Where the user is: the place entered, else an estimate from the time
    /// zone. Never from the network.
    pub(crate) fn place(&self) -> Place {
        self.settings.place_milli().map_or_else(
            || schedule::estimate_place(utc_offset_minutes()),
            Place::from_milli,
        )
    }

    /// The Auto toggle: on, the theme for the time of day is applied now.
    pub(super) fn set_auto(&mut self, on: bool) -> Task<Message> {
        self.settings.auto = on;
        self.auto.period = None;
        self.save_settings();
        self.auto_tick()
    }

    /// The theme Auto applies for `variant`: the one the user last picked
    /// for it, else the first toggled on for the cycle.
    pub(super) fn auto_theme(&self, variant: Variant) -> Option<String> {
        let preferred = match variant {
            Variant::Light => &self.settings.light_theme,
            Variant::Dark => &self.settings.dark_theme,
        };
        let of_kind = |key: &str| {
            self.theme(key)
                .is_some_and(|theme| theme.variant == variant)
        };
        preferred.clone().filter(|key| of_kind(key)).or_else(|| {
            self.selected_themes()
                .into_iter()
                .map(|(key, _)| key)
                .find(|key| of_kind(key))
        })
    }

    /// Auto's timer: when the time of day calls for the other kind than
    /// last time, apply the theme for it. Waits for a running write.
    pub(super) fn auto_tick(&mut self) -> Task<Message> {
        if !self.settings.auto || self.writing || self.scan.is_none() {
            return Task::none();
        }
        let history = &self.settings.switch_history;
        let Some(wanted) = schedule::wanted_variant(self.place(), now_minutes(), history) else {
            return Task::none();
        };
        if self.auto.period == Some(wanted) {
            return Task::none();
        }
        self.auto.period = Some(wanted);
        let Some(key) = self.auto_theme(wanted) else {
            return Task::none();
        };
        if self.settings.last_applied.as_deref() == Some(key.as_str()) {
            return Task::none();
        }
        // As a click does: no more dwells, and the write puts a preview's
        // original back first.
        self.preview.generation += 1;
        self.preview.waiting = None;
        self.apply_key(key, true)
    }

    /// The latitude or longitude box changed: once both hold a place within
    /// bounds (or both are empty: no place, so the estimate), keep it.
    pub(super) fn place_typed(&mut self) -> Task<Message> {
        let place = match (
            parse_degrees(&self.auto.latitude_text, 90),
            parse_degrees(&self.auto.longitude_text, 180),
        ) {
            (Some(latitude), Some(longitude)) => Some((latitude, longitude)),
            _ if self.auto.latitude_text.trim().is_empty()
                && self.auto.longitude_text.trim().is_empty() =>
            {
                None
            }
            // Half typed, or not a place: keep the last good one.
            _ => return Task::none(),
        };
        if place == self.settings.place_milli() {
            return Task::none();
        }
        self.settings.latitude_milli = place.map(|(latitude, _)| latitude);
        self.settings.longitude_milli = place.map(|(_, longitude)| longitude);
        self.save_settings();
        self.auto_tick()
    }
}

/// Minutes since the Unix epoch, UTC.
pub(super) fn now_minutes() -> i64 {
    chrono::Utc::now().timestamp().div_euclid(60)
}

/// The computer's offset from UTC now, in minutes (east positive).
fn utc_offset_minutes() -> i32 {
    chrono::Local::now().offset().local_minus_utc() / 60
}

/// A latitude or longitude as typed (a decimal comma is fine), in
/// thousandths of a degree; `None` unless it is a number within
/// ±`max_degrees`. What is typed is untrusted: NaN and infinity fail the
/// range check.
pub(super) fn parse_degrees(text: &str, max_degrees: i32) -> Option<i32> {
    let degrees: f64 = text.trim().replace(',', ".").parse().ok()?;
    let max = f64::from(max_degrees);
    if !(-max..=max).contains(&degrees) {
        return None;
    }
    // Within ±180000 after the range check.
    #[allow(clippy::cast_possible_truncation)]
    Some((degrees * 1000.0).round() as i32)
}

/// Thousandths of a degree as the box shows them; empty for none.
pub(super) fn degrees_text(milli: Option<i32>) -> String {
    milli.map_or_else(String::new, |milli| {
        let sign = if milli < 0 { "-" } else { "" };
        let milli = milli.unsigned_abs();
        let text = format!("{sign}{}.{:03}", milli / 1000, milli % 1000);
        text.trim_end_matches('0').trim_end_matches('.').to_owned()
    })
}
