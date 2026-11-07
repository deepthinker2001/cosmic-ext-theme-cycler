//! Showing a theme on the desktop.
//!
//! COSMIC keeps a dark slot and a light slot and shows the one its
//! light/dark setting picks. The applet writes the theme into the slot of
//! its own kind, builder then theme, as cosmic-config writes them (only the
//! keys that changed, see `fastwrite.rs`). Then it sets COSMIC's light/dark
//! setting to that kind, writing only that key and only when it differs, so
//! COSMIC switches to a slot already holding the theme and apps choose text
//! colours for the right kind. No other COSMIC setting is changed: COSMIC's
//! automatic light/dark switching is left as it is. `--restore` puts the
//! light/dark setting back too.
//!
//! The theme changes only when the user asks (a click, or resting the
//! pointer on a theme to preview it), never by itself.

use std::time::Instant;

use cosmic::cosmic_config::{self, CosmicConfigEntry};
use cosmic::cosmic_theme::{THEME_MODE_ID, Theme, ThemeBuilder, ThemeMode};
use cycler_core::settings::{APP_ID, CONFIG_VERSION, Settings};

use crate::fastwrite::{self, Durability};
use crate::{fl, restore};

/// What COSMIC showed before a preview, exactly: both slots' theme and
/// builder, and the light/dark setting. A preview of the other kind writes
/// the other slot too, so both are put back.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Shown {
    dark: bool,
    /// The dark slot, then the light one.
    slots: [(Theme, ThemeBuilder); 2],
}

impl Shown {
    /// The builder of the theme on screen.
    pub(crate) fn builder(&self) -> &ThemeBuilder {
        &self.slots[usize::from(!self.dark)].1
    }
}

#[cfg(test)]
impl Shown {
    /// `builder` shown, in both slots, for tests.
    pub(crate) fn of(builder: &ThemeBuilder) -> Self {
        let slot = (builder.clone().build(), builder.clone());
        Self {
            dark: builder.palette.is_dark(),
            slots: [slot.clone(), slot],
        }
    }
}

/// Shows `builder` on the desktop and records `key` as the applied theme.
/// During a preview, `original` (what was shown before it) is put back
/// first, in the same locked step, so a theme only hovered doesn't stay in
/// the other slot. Blocking: run it with [`crate::background::run`].
///
/// # Errors
/// Why it couldn't be written, as text for the popup; then nothing is
/// recorded and a preview's original can still be put back.
pub(crate) fn apply(
    builder: &ThemeBuilder,
    key: &str,
    original: Option<&Shown>,
) -> Result<(), String> {
    let snapshot = restore::snapshot_path().ok_or_else(|| fl!("restore-no-home"))?;
    // Held until the theme is written and recorded: another applet process
    // or `--restore` waits rather than interleaving with this write.
    let started = Instant::now();
    let _lock = lock(&snapshot)?;
    let locked = started.elapsed();
    // Only while the snapshot exists: if --restore ran since the preview
    // began, its theme stands, and writing the preview's original first
    // would have the next snapshot record that instead of the user's own.
    if let Some(original) = original
        && snapshot.exists()
    {
        write_slots(original)?;
    }
    write(builder, &snapshot)?;
    record_applied(key);
    tracing::debug!(theme = key, waited_for_lock = ?locked, total = ?started.elapsed(), "theme written");
    Ok(())
}

/// Shows `builder` without recording it: a step of a preview. Returns
/// `false`, writing nothing, if `--restore` has run since the preview began
/// (`continuing`: this isn't its first write), so a preview doesn't undo a
/// restore. Blocking.
///
/// # Errors
/// Why it couldn't be written, as text for the popup.
pub(crate) fn preview(builder: &ThemeBuilder, continuing: bool) -> Result<bool, String> {
    let snapshot = restore::snapshot_path().ok_or_else(|| fl!("restore-no-home"))?;
    let _lock = lock(&snapshot)?;
    if continuing && !snapshot.exists() {
        return Ok(false);
    }
    write(builder, &snapshot)?;
    Ok(true)
}

/// Ends a preview: puts back exactly what was shown before it. Returns
/// `false`, writing nothing, if `--restore` has run meanwhile (its theme
/// stands). Blocking.
///
/// # Errors
/// Why it couldn't be written; the preview's original is then kept, so
/// putting it back can be tried again.
pub(crate) fn put_back(original: &Shown) -> Result<bool, String> {
    let snapshot = restore::snapshot_path().ok_or_else(|| fl!("restore-no-home"))?;
    let _lock = lock(&snapshot)?;
    if !snapshot.exists() {
        return Ok(false);
    }
    write_slots(original)?;
    set_cosmic_dark(original.dark)?;
    Ok(true)
}

/// What COSMIC shows now, exactly, to put back after a preview. Waits for
/// any write in progress (holding the lock), so it never reads one half
/// done. cosmic-config reads a key missing from the current-version slot
/// from the previous version's folder, then COSMIC's system defaults, as
/// COSMIC does. Blocking.
///
/// # Errors
/// Why COSMIC's config couldn't be opened.
pub(crate) fn shown() -> Result<Shown, String> {
    let snapshot = restore::snapshot_path().ok_or_else(|| fl!("restore-no-home"))?;
    let _lock = lock(&snapshot)?;
    let error = |error: cosmic_config::Error| error.to_string();
    let mode = ThemeMode::config().map_err(error)?;
    let dark = ThemeMode::get_entry(&mode)
        .unwrap_or_else(|(_, mode)| mode)
        .is_dark;
    let slot = |dark: bool| -> Result<(Theme, ThemeBuilder), String> {
        let (theme, builder) = if dark {
            (Theme::dark_config(), ThemeBuilder::dark_config())
        } else {
            (Theme::light_config(), ThemeBuilder::light_config())
        };
        Ok((
            Theme::get_entry(&theme.map_err(error)?).unwrap_or_else(|(_, theme)| theme),
            ThemeBuilder::get_entry(&builder.map_err(error)?).unwrap_or_else(|(_, b)| b),
        ))
    };
    Ok(Shown {
        dark,
        slots: [slot(true)?, slot(false)?],
    })
}

/// Writes both slots back as `original` had them (not the light/dark
/// setting), holding the lock.
fn write_slots(original: &Shown) -> Result<(), String> {
    for (index, (theme, builder)) in original.slots.iter().enumerate() {
        restore::write_slot_entries(
            &restore::Where::System,
            index == 0,
            builder,
            theme,
            Durability::Unsynced,
        )?;
    }
    Ok(())
}

fn lock(snapshot: &std::path::Path) -> Result<std::fs::File, String> {
    restore::lock(snapshot).map_err(|error| fl!("snapshot-failed", error = error))
}

/// Writes the theme, holding the lock.
fn write(builder: &ThemeBuilder, snapshot: &std::path::Path) -> Result<(), String> {
    // The user's own theme is recorded before the applet's first write, so
    // `--restore` can put it back. Without it, nothing is written.
    restore::ensure_snapshot(&restore::Where::System, snapshot)
        .map_err(|error| fl!("snapshot-failed", error = error))?;
    let dark = builder.palette.is_dark();
    let theme = builder.clone().build();
    // Builder first, then the theme, as COSMIC Settings applies an import:
    // the theme is what apps react to, so it lands last. Only the keys that
    // differ are written, all at once (see `fastwrite.rs`).
    restore::write_slot_entries(
        &restore::Where::System,
        dark,
        builder,
        &theme,
        Durability::Unsynced,
    )?;
    // Then COSMIC switches to that slot, as COSMIC Settings does when it
    // imports a theme of the other kind.
    set_cosmic_dark(dark)
}

/// Sets COSMIC's light/dark setting to `dark`, writing only that key and
/// only if it differs, without waiting for the disk (see `fastwrite.rs`).
fn set_cosmic_dark(dark: bool) -> Result<(), String> {
    /// The one key of COSMIC's `ThemeMode` written here, which serializes
    /// as cosmic-config writes it.
    #[derive(serde::Serialize)]
    struct Mode {
        is_dark: bool,
    }
    let folder = restore::Where::System
        .folder(THEME_MODE_ID, ThemeMode::VERSION)
        .ok_or_else(|| fl!("restore-no-home"))?;
    fastwrite::write_key(
        &folder,
        &Mode { is_dark: dark },
        "is_dark",
        Durability::Unsynced,
    )
    .map(|_| ())
    .map_err(|error| error.user_message())
}

/// Saves `key` as the applied theme, writing only that setting, without
/// waiting for the disk, while the lock is held: `--restore` clears it
/// holding the lock, so the two can't land in the wrong order.
fn record_applied(key: &str) {
    let Ok(config) = Settings::config() else {
        return;
    };
    let mut settings = Settings::load(&config);
    settings.last_applied = Some(key.to_owned());
    let Some(folder) = restore::Where::System.folder(APP_ID, CONFIG_VERSION) else {
        return;
    };
    if let Err(error) =
        fastwrite::write_key(&folder, &settings, "last_applied", Durability::Unsynced)
    {
        tracing::warn!(%error, "applied theme not saved");
    }
}
