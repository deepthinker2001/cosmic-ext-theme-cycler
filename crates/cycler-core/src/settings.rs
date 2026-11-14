//! The applet's settings: which themes are toggled on for the cycle and the
//! last theme applied.
//!
//! Stored as COSMIC applets store their settings: through cosmic-config under
//! the App ID, version 1, one key per field
//! (`~/.config/cosmic/<APP_ID>/v1/selected` and so on), so a change to
//! one setting rewrites one small file and a watcher can say which changed.
//!
//! Loading never fails: a key that is missing or unreadable takes its
//! default, and every value is brought within bounds, so no file can stop
//! the applet from starting. Unknown keys are ignored.

use std::collections::HashSet;

use cosmic_config::{CosmicConfigEntry, cosmic_config_derive::CosmicConfigEntry};
use serde::{Deserialize, Serialize};

/// The applet's App ID, which names its cosmic-config namespace. It matches
/// the applet crate's `APP_ID`; the core crate can't depend on the applet,
/// so it is repeated here.
pub const APP_ID: &str = "io.github.deepthinker2001.CosmicExtThemeCycler";

/// The settings' config version, the `v1` in the path. Bump it only for a
/// change old files can't be read as.
pub const CONFIG_VERSION: u64 = 1;

/// The most themes a selection keeps; far more than anyone has installed.
pub const MAX_SELECTED: usize = 1024;

/// The applet's settings as written to disk. Read them through
/// [`Settings::load`], which brings them within bounds.
#[derive(Clone, CosmicConfigEntry, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[version = 1]
#[serde(default)]
pub struct Settings {
    /// The keys of the themes toggled on for the cycle, e.g.
    /// `system:nebula-dark`. Their order here doesn't matter: the cycle
    /// follows the popup's list order.
    pub selected: Vec<String>,
    /// The key of the theme the applet applied last, which the cycle moves
    /// on from, kept across restarts.
    pub last_applied: Option<String>,
    /// The user's manual light/dark choices, oldest first, bounded by
    /// [`crate::habit::MAX_HISTORY`]. Absent in older files: empty.
    pub switch_history: Vec<crate::habit::SwitchRecord>,
}

impl Settings {
    /// The cosmic-config namespace the settings live in.
    ///
    /// # Errors
    /// The cosmic-config error when the config folder can't be found or
    /// created.
    pub fn config() -> Result<cosmic_config::Config, cosmic_config::Error> {
        cosmic_config::Config::new(APP_ID, CONFIG_VERSION)
    }

    /// The same settings with every value usable: the selection without
    /// empty or repeated keys, and no empty last key.
    #[must_use]
    pub fn clamped(&self) -> Self {
        Self {
            selected: unique_keys(&self.selected),
            last_applied: self.last_applied.clone().filter(|key| !key.is_empty()),
            switch_history: crate::habit::clamp_history(&self.switch_history),
        }
    }

    /// Load from the applet's config, falling back to defaults key by key;
    /// never fails.
    #[must_use]
    pub fn load(config: &cosmic_config::Config) -> Self {
        let settings = match Self::get_entry(config) {
            Ok(settings) => settings,
            Err((errors, partial)) => {
                // The derive only reports real errors; a missing key (a first
                // run, or a setting added later) just keeps its default.
                for error in &errors {
                    tracing::warn!(%error, "a saved setting is unreadable; using its default");
                }
                partial
            }
        };
        let clamped = settings.clamped();
        if clamped != settings {
            tracing::warn!("saved settings out of bounds; corrected");
        }
        clamped
    }

    /// Save, clamped.
    ///
    /// # Errors
    /// The cosmic-config error when a write fails.
    pub fn save(&self, config: &cosmic_config::Config) -> Result<(), cosmic_config::Error> {
        self.clamped().write_entry(config)
    }
}

/// `keys` without empty keys and without repeats, keeping each key's first
/// place. The popup has one toggle per theme, so a repeat means nothing.
///
/// At most [`MAX_SELECTED`] keys are kept: the settings file can be written
/// by other software running as the user, and the cycle's lookups are linear
/// in the selection's length.
#[must_use]
pub fn unique_keys(keys: &[String]) -> Vec<String> {
    let mut seen = HashSet::new();
    keys.iter()
        .filter(|key| !key.is_empty() && seen.insert(key.as_str()))
        .take(MAX_SELECTED)
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scratch::Scratch;

    fn keys(keys: &[&str]) -> Vec<String> {
        keys.iter().map(|&key| key.to_owned()).collect()
    }

    /// A config in a scratch folder, never `~/.config`.
    fn temp_config(name: &str) -> (Scratch, cosmic_config::Config) {
        let root = Scratch::new(name);
        let config = cosmic_config::Config::with_custom_path(
            APP_ID,
            CONFIG_VERSION,
            root.path().to_path_buf(),
        )
        .unwrap();
        (root, config)
    }

    #[test]
    fn defaults_are_in_bounds_with_nothing_toggled_on() {
        let settings = Settings::default();
        assert_eq!(settings.clamped(), settings);
        assert_eq!(settings.selected, Vec::<String>::new());
        assert_eq!(settings.last_applied, None);
    }

    #[test]
    fn the_version_matches_the_derive() {
        assert_eq!(<Settings as CosmicConfigEntry>::VERSION, CONFIG_VERSION);
    }

    #[test]
    fn repeated_and_empty_keys_are_dropped_keeping_first_place() {
        let selected = keys(&["user:b", "system:a", "", "user:b", "system:c", "system:a"]);
        assert_eq!(
            unique_keys(&selected),
            keys(&["user:b", "system:a", "system:c"])
        );
        assert_eq!(unique_keys(&[]), Vec::<String>::new());
    }

    #[test]
    fn clamping_keeps_valid_values_and_is_stable() {
        let settings = Settings {
            selected: keys(&["system:a", "system:a", "user:b"]),
            last_applied: Some(String::new()),
            ..Settings::default()
        };
        let clamped = settings.clamped();
        assert_eq!(clamped.selected, keys(&["system:a", "user:b"]));
        assert_eq!(clamped.last_applied, None, "an empty key is no key");
        assert_eq!(clamped.clamped(), clamped);
    }

    #[test]
    fn round_trip_through_cosmic_config() {
        let (_root, config) = temp_config("round-trip");
        assert_eq!(Settings::load(&config), Settings::default(), "absent");
        let settings = Settings {
            selected: keys(&["system:nebula-dark", "user:my theme"]),
            last_applied: Some("user:my theme".into()),
            switch_history: vec![crate::habit::SwitchRecord {
                variant: crate::Variant::Dark,
                offset_minutes: -15,
            }],
        };
        settings.save(&config).unwrap();
        assert_eq!(Settings::load(&config), settings);
    }

    #[test]
    fn bad_keys_fall_back_alone_unknown_keys_are_ignored_and_values_are_clamped() {
        let (root, config) = temp_config("bad-keys");
        let settings = Settings {
            selected: keys(&["system:a"]),
            last_applied: Some("system:a".into()),
            ..Settings::default()
        };
        settings.save(&config).unwrap();
        let folder = root.path().join("cosmic").join(APP_ID).join("v1");
        std::fs::write(folder.join("last_applied"), "not a key").unwrap();
        std::fs::write(folder.join("interval_minutes"), "30").unwrap();
        std::fs::write(folder.join("selected"), r#"["a", "b", "a"]"#).unwrap();
        let loaded = Settings::load(&config);
        assert_eq!(loaded.last_applied, None, "unreadable key at its default");
        assert_eq!(loaded.selected, keys(&["a", "b"]), "other keys kept");
        assert!(
            loaded.switch_history.is_empty(),
            "a key added later defaults"
        );
    }

    #[test]
    fn selection_is_capped() {
        let keys: Vec<String> = (0..MAX_SELECTED + 10)
            .map(|n| format!("user:{n}"))
            .collect();
        let unique = unique_keys(&keys);
        assert_eq!(unique.len(), MAX_SELECTED);
        assert_eq!(unique[0], "user:0", "the first places are the ones kept");
    }
}
