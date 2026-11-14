//! The theme cycler's rules and files, without any GUI.
//!
//! - [`library`]: finding, reading and importing theme files.
//! - [`builtin`]: the themes that come with the applet.
//! - [`cycle`]: which theme comes next.
//! - [`habit`]: the user's habitual light/dark switch times.
//! - [`schedule`]: light or dark by the time of day, for Auto.
//! - [`settings`]: the applet's saved settings, through cosmic-config.
//! - [`panel`]: whether the applet is still in a panel.
//! - [`solar`]: sunrise/sunset and a smooth day curve.
//! - [`migrate`]: moving 0.1.x folders to the new name.
//!
//! Showing a theme on the desktop is the applet's job. When it applies a
//! theme, the applet also sets COSMIC's light/dark setting to match the
//! theme's kind, [`Variant`].

pub mod builtin;
pub mod cycle;
pub mod habit;
pub mod library;
pub mod migrate;
pub mod panel;
pub mod schedule;
pub mod settings;
pub mod solar;

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
