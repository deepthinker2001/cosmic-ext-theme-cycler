//! The theme cycler's rules and files, without any GUI.
//!
//! - [`library`]: finding, reading and importing theme files.
//! - [`builtin`]: the themes that come with the applet.
//! - [`cycle`]: which theme comes next.
//! - [`settings`]: the applet's saved settings, through cosmic-config.
//! - [`panel`]: whether the applet is still in a panel.
//! - [`migrate`]: moving 0.1.x folders to the new name.
//!
//! Showing a theme on the desktop is the applet's job. When it applies a
//! theme, the applet also sets COSMIC's light/dark setting to match the
//! theme's kind, [`Variant`].

pub mod builtin;
pub mod cycle;
pub mod library;
pub mod migrate;
pub mod panel;
pub mod settings;

use serde::{Deserialize, Serialize};

/// Whether a COSMIC theme is a dark or a light one. An exported theme says
/// so in its palette: `palette: Dark(...)` or `palette: Light(...)`.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum Variant {
    Dark,
    Light,
}

#[cfg(test)]
mod scratch;
