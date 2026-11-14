//! Auto: light by day and dark by night. Its state, and the methods of
//! the applet that decide when to switch and take the place typed in.

use cosmic::app::Task;
use cycler_core::Variant;
use cycler_core::habit;
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
    /// Saves the settings in the background, if they can be saved.
    fn save_settings(&self) {
        if let Some(saver) = &self.saver {
            saver.save(self.settings.clone());
        }
    }

    /// The kind of the theme the applet applied last, if it is still listed.
    pub(super) fn shown_variant(&self) -> Option<Variant> {
        let key = self.settings.last_applied.as_deref()?;
        self.theme(key).map(|theme| theme.variant)
    }

    /// The user picked `key` by hand: it is the theme Auto applies for its
    /// kind from now on, and a switch of kind is a data point for the
    /// learner of the user's habitual switch times.
    pub(super) fn remember_choice(&mut self, key: &str, previous: Option<Variant>) {
        let Some(variant) = self.theme(key).map(|theme| theme.variant) else {
            return;
        };
        let slot = match variant {
            Variant::Light => &mut self.settings.light_theme,
            Variant::Dark => &mut self.settings.dark_theme,
        };
        *slot = Some(key.to_owned());
        if self.settings.auto
            && previous != Some(variant)
            && let Some(record) = schedule::record_choice(self.place(), now_minutes(), variant)
        {
            habit::push(&mut self.settings.switch_history, record);
        }
        self.save_settings();
    }

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

#[cfg(test)]
mod tests {
    use cosmic::Application as _;

    use super::*;
    use crate::app::tests::applet;

    #[test]
    fn typed_degrees_are_checked_and_shown_back() {
        assert_eq!(parse_degrees(" 51.5 ", 90), Some(51_500));
        assert_eq!(parse_degrees("-0,25", 90), Some(-250));
        for bad in ["", "abc", "91", "-90.001", "NaN", "inf", "1e400"] {
            assert_eq!(parse_degrees(bad, 90), None, "{bad}");
        }
        assert_eq!(parse_degrees("180", 180), Some(180_000));
        assert_eq!(degrees_text(Some(51_500)), "51.5");
        assert_eq!(degrees_text(Some(-250)), "-0.25");
        assert_eq!(degrees_text(Some(100_000)), "100");
        assert_eq!(degrees_text(Some(i32::MIN)), "-2147483.648");
        assert_eq!(degrees_text(None), "");
    }

    #[test]
    fn a_place_needs_both_boxes_and_empty_boxes_clear_it() {
        let mut applet = applet(&[]);
        let _ = applet.update(Message::Latitude("51.5".into()));
        assert_eq!(applet.settings.place_milli(), None, "half typed");
        let _ = applet.update(Message::Longitude("-0.12".into()));
        assert_eq!(applet.settings.place_milli(), Some((51_500, -120)));
        let _ = applet.update(Message::Longitude("200".into()));
        assert_eq!(applet.settings.place_milli(), Some((51_500, -120)), "kept");
        let _ = applet.update(Message::Latitude(String::new()));
        let _ = applet.update(Message::Longitude(String::new()));
        assert_eq!(applet.settings.place_milli(), None);
    }

    #[test]
    fn a_manual_pick_is_remembered_for_its_kind() {
        let mut applet = applet(&["grey-dark"]);
        let _ = applet.next();
        let _ = applet.applied("grey-dark", Ok(()));
        assert_eq!(applet.settings.dark_theme.as_deref(), Some("grey-dark"));
        assert_eq!(applet.settings.light_theme, None);
        assert!(applet.settings.switch_history.is_empty(), "Auto is off");
    }

    #[test]
    fn auto_applies_the_remembered_theme_for_the_period_once() {
        let mut applet = applet(&["dim-dark"]);
        applet.settings.light_theme = Some("pale-light".into());
        applet.settings.dark_theme = Some("grey-dark".into());
        let _ = applet.update(Message::SetAuto(true));
        assert!(applet.settings.auto);
        assert!(applet.writing, "the theme for the time of day is written");
        let wanted = applet.auto.period.expect("a period was chosen");
        let key = match wanted {
            Variant::Light => "pale-light",
            Variant::Dark => "grey-dark",
        };
        let _ = applet.applied(key, Ok(()));
        assert_eq!(applet.settings.last_applied.as_deref(), Some(key));
        // Not a manual pick: nothing remembered or learned from it.
        assert_eq!(applet.settings.light_theme.as_deref(), Some("pale-light"));
        let _ = applet.update(Message::AutoTick);
        assert!(!applet.writing, "once per period");
    }

    #[test]
    fn auto_falls_back_to_a_toggled_theme_of_the_kind_and_off_does_nothing() {
        let mut applet = applet(&["grey-light", "grey-dark"]);
        assert_eq!(
            applet.auto_theme(Variant::Light).as_deref(),
            Some("grey-light")
        );
        assert_eq!(
            applet.auto_theme(Variant::Dark).as_deref(),
            Some("grey-dark")
        );
        applet.settings.light_theme = Some("gone".into());
        assert_eq!(
            applet.auto_theme(Variant::Light).as_deref(),
            Some("grey-light")
        );
        let _ = applet.update(Message::AutoTick);
        assert!(!applet.writing, "Auto is off");
    }
}
