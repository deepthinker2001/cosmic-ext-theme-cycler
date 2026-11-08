//! The user's own COSMIC settings that live beside a theme's colours.
//!
//! COSMIC keeps several of its settings in the same theme config as the
//! colours: Interface density (spacing) and Style (corner roundness) from
//! Settings → Appearance, window gaps and the active window hint's width and
//! colour, and frosted glass with its strength and translucency table. A
//! theme file carries them too, but they are the user's choices, not the
//! theme's, so the applet keeps them as the desktop has them: showing a
//! theme changes its colours only, and `--restore` leaves any of them the
//! user changed since in place.
//!
//! Without this, a theme with COSMIC's Standard spacing made a Compact
//! desktop roomier, and while previewing, the popup's list (sized from that
//! spacing) grew and shrank under the pointer.
//!
//! The desktop's values come from COSMIC's config, which isn't checked as
//! theme files are, so they are used only when every number in them is
//! finite; otherwise the theme's own (checked) values are written.

use cosmic::cosmic_theme::{Theme, ThemeBuilder};
use cycler_core::library;
use serde::Serialize;

/// Defines everything over one list of the settings' fields, so the keys,
/// the copy and the comparison can't disagree. Each field has the same
/// name, type and key in a builder and in the theme built from it.
macro_rules! user_settings {
    ($($field:ident),+ $(,)?) => {
        /// The keys of the user's settings.
        #[cfg(test)]
        const KEYS: &[&str] = &[$(stringify!($field)),+];

        /// `theme` with the user's settings taken from `current`, the
        /// builder of what the desktop shows now.
        fn copy(theme: &ThemeBuilder, current: &ThemeBuilder) -> ThemeBuilder {
            ThemeBuilder {
                $($field: current.$field.clone(),)+
                ..theme.clone()
            }
        }

        /// The user's settings that differ between `before` and `now`,
        /// compared as written (so a NaN equals itself).
        pub(crate) fn changed(before: &ThemeBuilder, now: &ThemeBuilder) -> Vec<&'static str> {
            let mut changed = Vec::new();
            $(if text(&before.$field) != text(&now.$field) {
                changed.push(stringify!($field));
            })+
            changed
        }

        /// Whether every number in the user's settings in `builder` is
        /// finite, as theme files must be. Only these are copied, so a
        /// non-finite colour elsewhere doesn't matter.
        pub(crate) fn usable(builder: &ThemeBuilder) -> bool {
            $(finite(&builder.$field))&&+
        }

        /// Copies the settings named in `keys` from `now` into a slot's
        /// builder and theme.
        pub(crate) fn copy_some(
            keys: &[&str],
            now: &ThemeBuilder,
            builder: &mut ThemeBuilder,
            theme: &mut Theme,
        ) {
            $(if keys.contains(&stringify!($field)) {
                builder.$field = now.$field.clone();
                theme.$field = now.$field.clone();
            })+
        }
    };
}

// Checked against libcosmic's `ThemeBuilder`: every field that isn't a
// colour or the palette. A field a newer libcosmic adds isn't kept until
// it is listed here.
user_settings!(
    spacing,
    corner_radii,
    gaps,
    active_hint,
    window_hint,
    frosted,
    frosted_windows,
    frosted_system_interface,
    frosted_panel,
    frosted_applets,
    frosted_maximized_apps,
    alpha_map,
);

/// The settings that change a built theme's colours too (its container
/// backgrounds' translucency), not just their own keys.
pub(crate) const RECOLOURING: &[&str] = &[
    "frosted",
    "frosted_windows",
    "frosted_system_interface",
    "frosted_panel",
    "frosted_applets",
    "alpha_map",
];

/// `theme` with the user's settings from `current`, if `current` can be
/// used (every number finite); otherwise `theme` as it is.
pub(crate) fn user_settings(theme: &ThemeBuilder, current: Option<&ThemeBuilder>) -> ThemeBuilder {
    match current.filter(|current| usable(current)) {
        Some(current) => copy(theme, current),
        None => theme.clone(),
    }
}

/// Whether every number in `value` is finite.
fn finite<T: Serialize>(value: &T) -> bool {
    ron::to_string(value).is_ok_and(|text| library::check_finite(&text).is_ok())
}

/// A value's text as written.
fn text<T: Serialize>(value: &T) -> String {
    ron::to_string(value).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use cosmic::cosmic_theme::palette::Srgb;
    use cosmic::cosmic_theme::{BlurStrength, CornerRadii, Density};

    use super::*;
    use crate::fastwrite;

    /// A builder whose kept settings all differ from COSMIC's defaults.
    pub(crate) fn customised() -> ThemeBuilder {
        let mut builder = ThemeBuilder::light();
        builder.spacing = Density::Compact.into();
        builder.corner_radii = CornerRadii {
            radius_s: [2.0; 4],
            ..CornerRadii::default()
        };
        builder.gaps = (7, 9);
        builder.active_hint = 5;
        builder.window_hint = Some(Srgb::new(1.0, 0.0, 0.5));
        builder.frosted = BlurStrength::High;
        builder.frosted_windows = !builder.frosted_windows;
        builder.frosted_system_interface = !builder.frosted_system_interface;
        builder.frosted_panel = !builder.frosted_panel;
        builder.frosted_applets = !builder.frosted_applets;
        builder.frosted_maximized_apps = !builder.frosted_maximized_apps;
        builder.alpha_map.low = 0.123;
        builder
    }

    /// The keys whose text differs between two entries.
    fn differing<T: Serialize>(a: &T, b: &T) -> BTreeSet<&'static str> {
        let (a, b) = (fastwrite::keys(a).unwrap(), fastwrite::keys(b).unwrap());
        fastwrite::changed(&a, &b)
            .into_iter()
            .map(|(name, _)| *name)
            .collect()
    }

    #[test]
    fn exactly_the_user_s_settings_are_kept() {
        let theme = ThemeBuilder::dark();
        let current = customised();
        let shown = user_settings(&theme, Some(&current));
        // Every kept key is the user's...
        for key in differing(&current, &shown) {
            assert!(!KEYS.contains(&key), "{key} isn't the user's");
        }
        // ...and only those keys moved away from the theme's own.
        assert_eq!(differing(&theme, &shown), KEYS.iter().copied().collect());
        assert_eq!(changed(&theme, &current), KEYS);
    }

    #[test]
    fn the_built_theme_uses_the_same_key_names() {
        let theme = ThemeBuilder::dark();
        let shown = user_settings(&theme, Some(&customised()));
        let changed = differing(&theme.build(), &shown.build());
        for key in KEYS {
            assert!(
                changed.contains(key),
                "{key} isn't a key of the built theme"
            );
        }
    }

    #[test]
    fn a_desktop_with_a_non_finite_number_isn_t_copied() {
        let theme = ThemeBuilder::dark();
        let mut current = customised();
        current.alpha_map.low = f32::NAN;
        assert_eq!(user_settings(&theme, Some(&current)), theme);
        assert_eq!(user_settings(&theme, None), theme);
    }

    #[test]
    fn a_non_finite_colour_doesn_t_stop_the_settings_being_kept() {
        let theme = ThemeBuilder::dark();
        let mut current = customised();
        current.alpha_map.low = 0.5;
        current.accent = Some(Srgb::new(f32::NAN, 0.0, 0.0));
        assert_eq!(
            user_settings(&theme, Some(&current)).spacing,
            current.spacing
        );
    }

    #[test]
    fn a_nan_is_no_change() {
        let mut builder = ThemeBuilder::dark();
        builder.alpha_map.low = f32::NAN;
        assert_eq!(changed(&builder, &builder.clone()), Vec::<&str>::new());
    }

    #[test]
    fn some_settings_are_copied_to_builder_and_theme() {
        let mut builder = ThemeBuilder::dark();
        let mut theme = builder.clone().build();
        let now = customised();
        copy_some(&["gaps"], &now, &mut builder, &mut theme);
        assert_eq!((builder.gaps, theme.gaps), (now.gaps, now.gaps));
        assert_eq!(builder.spacing, ThemeBuilder::dark().spacing);
    }
}
