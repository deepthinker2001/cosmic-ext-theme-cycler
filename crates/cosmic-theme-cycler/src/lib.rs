//! Standalone COSMIC panel applet that cycles through the user's chosen
//! COSMIC themes.
//!
//! A left click on the panel icon applies the next theme toggled on; a
//! right click opens the popup, rendered the standard COSMIC way through
//! `view_window` and a surface task, which lists the themes with a toggle
//! each (resting the pointer on one previews it) and imports theme files.
//! The rules and theme files live in [`cycler_core`]; showing a theme on
//! the desktop is `apply.rs`.

mod app;
mod apply;
mod background;
mod fastwrite;
mod localize;
mod restore;
mod saver;
#[cfg(test)]
mod scratch;
mod shutdown;
mod tooltip;
mod view;

use std::os::unix::fs::MetadataExt;

use cosmic::{applet, iced};

/// Reverse-DNS application ID. It names the desktop entry in `data/` and the
/// applet's config namespace, so changing it later moves users' settings.
/// Defined in the core crate, which reads the settings too.
pub use cycler_core::settings::APP_ID;

/// Panel icon: the applet's own symbolic icon, installed into the hicolor
/// theme from `data/icons`.
const ICON: &str = "io.github.deepthinker2001.CosmicThemeCycler-symbolic";

/// Run the applet under cosmic-panel.
///
/// # Errors
/// Returns libcosmic's error if the Wayland connection or panel handshake fails.
pub fn run() -> iced::Result {
    localize::localize();
    applet::run::<app::ThemeCycler>(())
}

/// `cosmic-theme-cycler --restore`: puts back the theme the user had before
/// the applet changed it, forgets the applied theme and says what it did.
/// See `restore.rs`.
#[must_use]
pub fn restore_from_terminal() -> std::process::ExitCode {
    localize::localize();
    restore::run()
}

/// Why this process won't run, if it runs as root: every mode works on the
/// user's own settings, and as root (say through `sudo -E`) would leave
/// root-owned files in the user's config and state folders. `restoring`:
/// started as `--restore`.
#[must_use]
pub fn refusal_as_root(restoring: bool) -> Option<String> {
    // Refused too if the owner can't be read: never run as root by mistake.
    let as_root = !std::fs::metadata("/proc/self").is_ok_and(|process| process.uid() != 0);
    if !as_root {
        return None;
    }
    localize::localize();
    Some(if restoring {
        fl!("restore-as-root")
    } else {
        fl!("applet-as-root")
    })
}

/// A general message for the user about an error that can't happen in a
/// working applet; `detail`, which means nothing to the user, goes to the
/// log.
pub(crate) fn internal_error(detail: &str) -> String {
    tracing::error!(detail, "internal error");
    fl!("internal-error")
}

/// The command-line help text.
#[must_use]
pub fn usage() -> String {
    localize::localize();
    fl!("usage").replace(['\u{2068}', '\u{2069}'], "")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_id_matches_desktop_entry_file() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/data/");
        let entry = std::fs::read_to_string(format!("{path}{APP_ID}.desktop"))
            .expect("desktop entry named after APP_ID");
        assert!(entry.contains("X-CosmicApplet=true"));
        assert!(entry.contains("NoDisplay=true"));
        assert!(entry.contains("Exec=cosmic-theme-cycler\n"));
        assert!(entry.contains(&format!("Icon={ICON}\n")));
    }

    #[test]
    fn icon_is_shipped() {
        let icon = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/data/icons/hicolor/scalable/apps/"
        );
        assert!(std::path::Path::new(&format!("{icon}{ICON}.svg")).is_file());
    }

    #[test]
    fn fallback_strings_resolve() {
        assert_eq!(fl!("applet-name"), "Theme Cycler");
        assert!(usage().starts_with("Usage: cosmic-theme-cycler"));
    }
}
