//! Themes that come with the applet, so there is a good set to cycle
//! through without importing anything: COSMIC's own default Dark and
//! Light, and ten more: Midnight, Frost and Overcast, and seven well-known
//! palettes (Moonfly, Catppuccin Latte and Mocha, Tokyo Night and Tokyo
//! Night Light, Nord, Kanagawa Wave), whose colours are iced 0.14's named
//! palettes (`iced_core/src/theme/palette.rs`).
//!
//! They are built the way System76 builds its presets in
//! `/usr/share/cosmic-themes`: COSMIC's default builder for the kind (dark
//! or light), with the background colour, a text tint, a neutral tint and
//! the accent, success, warning and destructive colours set, and the
//! container colours left for COSMIC to derive.
//!
//! COSMIC's defaults are read as COSMIC reads them: from the user's data
//! folder (`~/.local/share/cosmic`) if it has them, else `/usr/share/cosmic`,
//! which is what an untouched desktop shows. Like theme files they must hold
//! only finite numbers; libcosmic's own defaults stand in if they can't be
//! read or don't pass.

use std::path::PathBuf;

use cosmic_config::{Config, CosmicConfigEntry};
use cosmic_theme::palette::{Srgb, Srgba};
use cosmic_theme::{DARK_THEME_BUILDER_ID, LIGHT_THEME_BUILDER_ID, ThemeBuilder};

use crate::library::{Source, Theme, background_of, check_finite, theme_key, variant_of};

/// One theme's colours, as `0xRRGGBB`.
struct Palette {
    /// The key's stem, e.g. `moonfly` for `builtin:moonfly`.
    stem: &'static str,
    name: &'static str,
    dark: bool,
    background: u32,
    text: u32,
    accent: u32,
    success: u32,
    warning: u32,
    destructive: u32,
}

/// The ten beyond COSMIC's defaults.
const PALETTES: [Palette; 10] = [
    Palette {
        stem: "midnight",
        name: "Midnight",
        dark: true,
        background: 0x11_11_1b,
        text: 0xcd_d6_f4,
        accent: 0x89_b4_fa,
        success: 0xa6_e3_a1,
        warning: 0xfa_b3_87,
        destructive: 0xf3_8b_a8,
    },
    Palette {
        stem: "frost",
        name: "Frost",
        dark: false,
        background: 0xdc_e0_e8,
        text: 0x4c_4f_69,
        accent: 0x1e_66_f5,
        success: 0x40_a0_2b,
        warning: 0xfe_64_0b,
        destructive: 0xd2_0f_39,
    },
    Palette {
        stem: "overcast",
        name: "Overcast",
        dark: false,
        background: 0x96_9a_a8,
        text: 0x15_16_1e,
        accent: 0x1e_40_af,
        success: 0x14_53_2d,
        warning: 0x9a_34_12,
        destructive: 0x9f_12_39,
    },
    Palette {
        stem: "moonfly",
        name: "Moonfly",
        dark: true,
        background: 0x08_08_08,
        text: 0xbd_bd_bd,
        accent: 0x80_a0_ff,
        success: 0x8c_c8_5f,
        warning: 0xe3_c7_8a,
        destructive: 0xff_54_54,
    },
    Palette {
        stem: "catppuccin-latte",
        name: "Catppuccin Latte",
        dark: false,
        background: 0xef_f1_f5,
        text: 0x4c_4f_69,
        accent: 0x1e_66_f5,
        success: 0x40_a0_2b,
        warning: 0xdf_8e_1d,
        destructive: 0xd2_0f_39,
    },
    Palette {
        stem: "catppuccin-mocha",
        name: "Catppuccin Mocha",
        dark: true,
        background: 0x1e_1e_2e,
        text: 0xcd_d6_f4,
        accent: 0x89_b4_fa,
        success: 0xa6_e3_a1,
        warning: 0xf9_e2_af,
        destructive: 0xf3_8b_a8,
    },
    Palette {
        stem: "tokyo-night",
        name: "Tokyo Night",
        dark: true,
        background: 0x1a_1b_26,
        text: 0x9a_a5_ce,
        accent: 0x2a_c3_de,
        success: 0x9e_ce_6a,
        warning: 0xe0_af_68,
        destructive: 0xf7_76_8e,
    },
    Palette {
        stem: "tokyo-night-light",
        name: "Tokyo Night Light",
        dark: false,
        background: 0xd5_d6_db,
        text: 0x56_5a_6e,
        accent: 0x16_67_75,
        success: 0x48_5e_30,
        warning: 0x8f_5e_15,
        destructive: 0x8c_43_51,
    },
    Palette {
        stem: "nord",
        name: "Nord",
        dark: true,
        background: 0x2e_34_40,
        text: 0xec_ef_f4,
        accent: 0x8f_bc_bb,
        success: 0xa3_be_8c,
        warning: 0xeb_cb_8b,
        destructive: 0xbf_61_6a,
    },
    Palette {
        stem: "kanagawa-wave",
        name: "Kanagawa Wave",
        dark: true,
        background: 0x1f_1f_28,
        text: 0xdc_d7_ba,
        accent: 0x7f_b4_ca,
        success: 0x76_94_6a,
        warning: 0xff_9e_3b,
        destructive: 0xc3_40_43,
    },
];

/// Every built-in theme: COSMIC's two defaults, then the ten others.
/// Reads COSMIC's defaults from disk (see the module docs); blocking.
#[must_use]
pub fn themes() -> Vec<Theme> {
    let mut themes = vec![
        theme("cosmic-dark", "COSMIC Dark", cosmic_default(true)),
        theme("cosmic-light", "COSMIC Light", cosmic_default(false)),
    ];
    themes.extend(
        PALETTES
            .iter()
            .map(|palette| theme(palette.stem, palette.name, builder(palette))),
    );
    themes
}

fn theme(stem: &str, name: &str, builder: ThemeBuilder) -> Theme {
    Theme {
        key: theme_key(Source::Builtin, stem),
        name: name.to_owned(),
        source: Source::Builtin,
        path: PathBuf::new(),
        variant: variant_of(&builder),
        background: background_of(&builder),
        builder,
    }
}

/// COSMIC's default builder for the kind, from the user's data folder, else
/// `/usr/share/cosmic`.
fn cosmic_default(dark: bool) -> ThemeBuilder {
    let (id, fallback): (&str, fn() -> ThemeBuilder) = if dark {
        (DARK_THEME_BUILDER_ID, ThemeBuilder::dark)
    } else {
        (LIGHT_THEME_BUILDER_ID, ThemeBuilder::light)
    };
    // A key that can't be read keeps libcosmic's default (some installed
    // system keys don't parse with this libcosmic).
    Config::system(id, ThemeBuilder::VERSION)
        .ok()
        .map(|config| ThemeBuilder::get_entry(&config).unwrap_or_else(|(_, partial)| partial))
        // A system default of the wrong kind would list as the other one.
        .filter(|builder| builder.palette.is_dark() == dark)
        // The data folder is the user's to write, so the same check as for
        // theme files: no NaN or infinity reaches COSMIC's config.
        .filter(all_finite)
        .unwrap_or_else(fallback)
}

/// Whether the builder, and the theme built from it, hold only finite
/// numbers in the form they are written to COSMIC's config.
fn all_finite(builder: &ThemeBuilder) -> bool {
    let finite =
        |text: Result<String, ron::Error>| text.is_ok_and(|text| check_finite(&text).is_ok());
    finite(ron::to_string(builder)) && finite(ron::to_string(&builder.clone().build()))
}

/// A builder for `palette`, made like System76's presets.
fn builder(palette: &Palette) -> ThemeBuilder {
    let base = if palette.dark {
        ThemeBuilder::dark()
    } else {
        ThemeBuilder::light()
    };
    let background = rgb(palette.background);
    let text = rgb(palette.text);
    ThemeBuilder {
        bg_color: Some(Srgba::new(
            background.red,
            background.green,
            background.blue,
            1.0,
        )),
        text_tint: Some(text),
        // Greys lean towards the background's hue, so dividers and
        // disabled controls belong to the theme rather than to COSMIC's.
        neutral_tint: Some(mix(background, text, 0.45)),
        accent: Some(rgb(palette.accent)),
        success: Some(rgb(palette.success)),
        warning: Some(rgb(palette.warning)),
        destructive: Some(rgb(palette.destructive)),
        ..base
    }
}

fn rgb(hex: u32) -> Srgb {
    let [_, red, green, blue] = hex.to_be_bytes();
    let channel = |value: u8| f32::from(value) / 255.0;
    Srgb::new(channel(red), channel(green), channel(blue))
}

/// `from` moved `amount` of the way to `to`.
fn mix(from: Srgb, to: Srgb, amount: f32) -> Srgb {
    let step = |a: f32, b: f32| a + (b - a) * amount;
    Srgb::new(
        step(from.red, to.red),
        step(from.green, to.green),
        step(from.blue, to.blue),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Variant;

    #[test]
    fn there_are_twelve_with_unique_keys() {
        let themes = themes();
        assert_eq!(themes.len(), 12);
        let mut keys: Vec<&str> = themes.iter().map(|theme| theme.key.as_str()).collect();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), 12);
        assert!(keys.iter().all(|key| key.starts_with("builtin:")));
    }

    #[test]
    fn each_is_the_kind_its_palette_says() {
        for palette in &PALETTES {
            let theme = theme(palette.stem, palette.name, builder(palette));
            let expected = if palette.dark {
                Variant::Dark
            } else {
                Variant::Light
            };
            assert_eq!(theme.variant, expected, "{}", palette.name);
        }
        let themes = themes();
        assert_eq!(themes[0].variant, Variant::Dark, "COSMIC Dark");
        assert_eq!(themes[1].variant, Variant::Light, "COSMIC Light");
    }

    #[test]
    fn the_background_is_the_palette_s() {
        let moonfly = &PALETTES[3];
        let theme = theme(moonfly.stem, moonfly.name, builder(moonfly));
        let close = |a: f32, b: f32| (a - b).abs() < 0.01;
        let background = rgb(moonfly.background);
        assert!(close(theme.background.red, background.red));
        assert!(close(theme.background.blue, background.blue));
    }

    #[test]
    fn every_theme_is_finite_and_round_trips_as_a_theme_file() {
        for theme in themes() {
            let text =
                ron::ser::to_string_pretty(&theme.builder, ron::ser::PrettyConfig::default())
                    .unwrap();
            assert_eq!(check_finite(&text), Ok(()), "{}", theme.name);
            let read: ThemeBuilder = ron::from_str(&text).unwrap();
            assert_eq!(variant_of(&read), theme.variant, "{}", theme.name);
        }
    }

    #[test]
    fn hex_colours_are_read_channel_by_channel() {
        let colour = rgb(0xff_80_00);
        assert!((colour.red - 1.0).abs() < 1e-6);
        assert!((colour.green - 128.0 / 255.0).abs() < 1e-6);
        assert!(colour.blue.abs() < 1e-6);
    }

    #[test]
    fn a_default_with_a_non_finite_number_is_not_used() {
        let mut builder = ThemeBuilder::dark();
        assert!(all_finite(&builder));
        builder.corner_radii.radius_m = [f32::NAN; 4];
        assert!(!all_finite(&builder));
    }
}
