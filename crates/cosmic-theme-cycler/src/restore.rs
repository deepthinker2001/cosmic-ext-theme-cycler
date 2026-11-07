//! Putting back the theme the user had before the applet changed it.
//!
//! Before the applet's first write, [`ensure_snapshot`] records both of
//! COSMIC's theme slots: the theme and builder as COSMIC resolves them, and
//! which keys the user's current-version folder held. cosmic-config reads a
//! key missing from that folder from the previous version's folder, then
//! from the system defaults, so the user's own theme may live entirely in
//! an older folder (`v1`) while `v2` is empty.
//!
//! `cosmic-theme-cycler --restore` ([`run`]) writes the snapshot back as
//! cosmic-config writes keys, so running apps, the settings daemon and the
//! compositor all see it (they don't notice copied files), then removes the
//! keys the folders didn't hold before, so COSMIC reads the older folder or the defaults again
//! exactly as it did, puts COSMIC's light/dark setting back, and forgets the
//! applied theme. The snapshot is then deleted, so the next change starts a
//! new one. The applet does the same when it is taken out of the panel
//! ([`restore_now`]).
//!
//! Only the two theme slots and the light/dark setting are written.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::atomic::{AtomicU64, Ordering};

use cosmic::cosmic_config::{self, ConfigSet, CosmicConfigEntry};
use cosmic::cosmic_theme::{
    DARK_THEME_BUILDER_ID, DARK_THEME_ID, LIGHT_THEME_BUILDER_ID, LIGHT_THEME_ID, THEME_MODE_ID,
    Theme, ThemeBuilder, ThemeMode,
};
use cycler_core::library::{self, ReadError};
use cycler_core::settings::{APP_ID, CONFIG_VERSION, Settings};
use serde::{Deserialize, Serialize};

use crate::fastwrite::{self, Durability};
use crate::fl;

/// The key holding COSMIC's light/dark setting.
const MODE_KEY: &str = "is_dark";

/// The snapshot's file name in the applet's state folder.
const SNAPSHOT_FILE: &str = "original-theme.ron";
/// The lock file beside it, held while the snapshot or COSMIC's theme is
/// written, so applet processes and `--restore` take turns.
const LOCK_FILE: &str = "lock";
/// A snapshot holds two themes and two builders, a few tens of KiB; a file
/// far bigger than this was not written by the applet.
const MAX_SNAPSHOT_BYTES: u64 = 4 << 20;

/// Where COSMIC's config lives: the real one, or a test folder.
#[derive(Clone, Debug)]
pub(crate) enum Where {
    /// `$XDG_CONFIG_HOME` (or `~/.config`), with COSMIC's system defaults.
    System,
    /// `<path>/cosmic/...`, without system defaults; for tests.
    #[cfg_attr(not(test), expect(dead_code, reason = "only tests use a test folder"))]
    Custom(PathBuf),
}

impl Where {
    fn config(
        &self,
        id: &str,
        version: u64,
    ) -> Result<cosmic_config::Config, cosmic_config::Error> {
        match self {
            Self::System => cosmic_config::Config::new(id, version),
            Self::Custom(path) => {
                cosmic_config::Config::with_custom_path(id, version, path.clone())
            }
        }
    }

    /// The user's folder for one config at one version, as cosmic-config
    /// lays it out.
    pub(crate) fn folder(&self, id: &str, version: u64) -> Option<PathBuf> {
        let base = match self {
            Self::System => config_home()?,
            Self::Custom(path) => path.clone(),
        };
        Some(base.join("cosmic").join(id).join(format!("v{version}")))
    }
}

/// One of COSMIC's theme slots as it was.
#[derive(Debug, Deserialize, PartialEq, Serialize)]
struct Slot {
    theme: Theme,
    builder: ThemeBuilder,
    /// The keys the theme's current-version folder held; `None` if the
    /// folder couldn't be found or read then, in which case restoring
    /// removes nothing from it rather than guessing.
    theme_keys: Option<Vec<String>>,
    /// The same for the builder's folder.
    builder_keys: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, PartialEq, Serialize)]
struct Snapshot {
    dark: Slot,
    light: Slot,
    /// COSMIC's light/dark setting as it was. A snapshot written by an
    /// older version of the applet, which left the setting alone, lacks it;
    /// [`ensure_snapshot`] then adds the setting as it is now.
    #[serde(default)]
    mode: Option<ModeRecord>,
}

/// COSMIC's light/dark setting as it was.
#[derive(Debug, Deserialize, PartialEq, Serialize)]
struct ModeRecord {
    is_dark: bool,
    /// Whether its folder held an `is_dark` key (rather than COSMIC using
    /// its default); `None` if the folder couldn't be read.
    had_key: Option<bool>,
}

/// The IDs of one slot's theme and builder configs.
const fn ids(dark: bool) -> (&'static str, &'static str) {
    if dark {
        (DARK_THEME_ID, DARK_THEME_BUILDER_ID)
    } else {
        (LIGHT_THEME_ID, LIGHT_THEME_BUILDER_ID)
    }
}

/// Takes the lock beside `snapshot`, waiting for another holder. Hold the
/// returned file while taking the snapshot and writing the theme, or while
/// restoring: two applet processes can't then both record a snapshot (the
/// second would record the first's theme), and `--restore` can't run in
/// the middle of a write. Unlocked when dropped. Blocking.
///
/// # Errors
/// Why the lock file couldn't be opened or locked, as text for the user.
pub(crate) fn lock(snapshot: &Path) -> Result<File, String> {
    let io_error = |error: io::Error| error.to_string();
    let path = snapshot.with_file_name(LOCK_FILE);
    if let Some(folder) = path.parent() {
        fs::create_dir_all(folder).map_err(io_error)?;
    }
    // Not following a link and not blocking: a named pipe planted here would
    // otherwise hang every theme write and `--restore`.
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .custom_flags(libc::O_NONBLOCK | libc::O_NOFOLLOW)
        .open(path)
        .map_err(|error| not_a_file_or(&error))?;
    if !file.metadata().map_err(io_error)?.is_file() {
        return Err(fl!("lock-not-a-file"));
    }
    file.lock().map_err(io_error)?;
    Ok(file)
}

/// The text for an error opening a file with `O_NOFOLLOW`: a symbolic link
/// in its place is reported as not being a file.
fn not_a_file_or(error: &io::Error) -> String {
    if error.raw_os_error() == Some(libc::ELOOP) {
        fl!("not-a-file")
    } else {
        error.to_string()
    }
}

/// Takes the snapshot if there isn't one yet. Called, holding [`lock`],
/// before every write, so it only records the theme the applet hasn't
/// changed. Blocking.
///
/// # Errors
/// Why the snapshot couldn't be taken; the caller doesn't write the theme
/// then, since it couldn't be put back.
pub(crate) fn ensure_snapshot(place: &Where, snapshot: &Path) -> Result<(), String> {
    // Only a real file counts; anything else in its place (a folder, a
    // pipe) would leave nothing to restore, so no theme is written then.
    match fs::symlink_metadata(snapshot) {
        Ok(metadata) if metadata.is_file() => return add_mode_if_missing(place, snapshot),
        Ok(_) => return Err(fl!("not-a-file")),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.to_string()),
    }
    let taken = Snapshot {
        dark: read_slot(place, true)?,
        light: read_slot(place, false)?,
        mode: Some(read_mode(place)),
    };
    write_snapshot(snapshot, &taken)
}

/// Writes `taken` as the snapshot, holding [`lock`].
fn write_snapshot(snapshot: &Path, taken: &Snapshot) -> Result<(), String> {
    let text = ron::ser::to_string_pretty(taken, ron::ser::PrettyConfig::default())
        .map_err(|error| crate::internal_error(&format!("snapshot not serialized: {error}")))?;
    write_new(snapshot, &text).map_err(|error| error.to_string())
}

/// Adds COSMIC's light/dark setting to a snapshot that lacks it (see
/// [`Snapshot::mode`]): no applet has changed the setting since that
/// snapshot was taken, so it is still as it was then.
fn add_mode_if_missing(place: &Where, snapshot: &Path) -> Result<(), String> {
    let Some(text) = read_snapshot(snapshot)? else {
        return Ok(());
    };
    let Ok(mut taken) = ron::from_str::<Snapshot>(&text) else {
        // Unreadable: --restore reports it; it isn't replaced here.
        return Ok(());
    };
    if taken.mode.is_some() {
        return Ok(());
    }
    taken.mode = Some(read_mode(place));
    write_snapshot(snapshot, &taken)
}

/// COSMIC's light/dark setting now, as COSMIC reads it (dark if unset).
fn read_mode(place: &Where) -> ModeRecord {
    let is_dark = place
        .config(THEME_MODE_ID, ThemeMode::VERSION)
        .ok()
        .is_none_or(|config| {
            ThemeMode::get_entry(&config)
                .unwrap_or_else(|(_, mode)| mode)
                .is_dark
        });
    let had_key = keys_in(place.folder(THEME_MODE_ID, ThemeMode::VERSION).as_deref())
        .map(|keys| keys.iter().any(|key| key == MODE_KEY));
    ModeRecord { is_dark, had_key }
}

/// Puts COSMIC's light/dark setting back: written through cosmic-config so
/// COSMIC switches, then the key file removed if there was none before, so
/// COSMIC uses its default again (the same value).
fn write_mode(place: &Where, mode: &ModeRecord) -> Result<(), String> {
    let config = place
        .config(THEME_MODE_ID, ThemeMode::VERSION)
        .map_err(|error| error.to_string())?;
    config
        .set(MODE_KEY, mode.is_dark)
        .map_err(|error| error.to_string())?;
    if mode.had_key == Some(false)
        && let Some(folder) = place.folder(THEME_MODE_ID, ThemeMode::VERSION)
    {
        let _ = fs::remove_file(folder.join(MODE_KEY));
    }
    Ok(())
}

fn read_slot(place: &Where, dark: bool) -> Result<Slot, String> {
    let (theme_id, builder_id) = ids(dark);
    let theme_config = place
        .config(theme_id, Theme::VERSION)
        .map_err(|error| error.to_string())?;
    let builder_config = place
        .config(builder_id, ThemeBuilder::VERSION)
        .map_err(|error| error.to_string())?;
    Ok(Slot {
        // Partial reads keep what COSMIC would show for each key.
        theme: Theme::get_entry(&theme_config).unwrap_or_else(|(_, theme)| theme),
        builder: ThemeBuilder::get_entry(&builder_config).unwrap_or_else(|(_, builder)| builder),
        theme_keys: keys_in(place.folder(theme_id, Theme::VERSION).as_deref()),
        builder_keys: keys_in(place.folder(builder_id, ThemeBuilder::VERSION).as_deref()),
    })
}

/// The key files in a config folder, as cosmic-config sees them: files,
/// following symlinks (a dotfile manager may link them), and not
/// cosmic-config's hidden temporary files. A missing folder holds no keys;
/// `None` when the folder is unknown or can't be read.
fn keys_in(folder: Option<&Path>) -> Option<Vec<String>> {
    let folder = folder?;
    let entries = match fs::read_dir(folder) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Some(Vec::new()),
        Err(_) => return None,
    };
    let mut keys: Vec<String> = entries
        .filter_map(Result::ok)
        .filter(|entry| fs::metadata(entry.path()).is_ok_and(|metadata| metadata.is_file()))
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| !name.starts_with('.'))
        .collect();
    keys.sort();
    Some(keys)
}

/// Writes `text` to `path` through a uniquely named temporary file, so a
/// crash never leaves half a snapshot, and syncs the folder so the snapshot
/// is on disk before the first theme write. Callers hold [`lock`], so no
/// other process replaces it.
fn write_new(path: &Path, text: &str) -> io::Result<()> {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let folder = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(folder)?;
    let temporary = folder.join(format!(
        ".{SNAPSHOT_FILE}.{}-{}.tmp",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    let written = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        file.write_all(text.as_bytes())?;
        file.sync_all()?;
        fs::rename(&temporary, path)
    })();
    if written.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    written?;
    File::open(folder)?.sync_all()
}

/// Puts the snapshot back. Returns `false` when there was none.
/// `under_lock` runs first once the lock is held: `--restore` uses it to
/// forget the applied theme, ordered with the applet recording one. It
/// can't stop the restore: getting the theme back matters more, so it
/// reports its own failures.
///
/// # Errors
/// Why it couldn't be read or written back.
pub(crate) fn restore(
    place: &Where,
    snapshot: &Path,
    under_lock: impl FnOnce(),
) -> Result<bool, String> {
    let _lock = lock(snapshot)?;
    restore_locked(place, snapshot, under_lock)
}

/// [`restore`] for a caller already holding the lock.
fn restore_locked(
    place: &Where,
    snapshot: &Path,
    under_lock: impl FnOnce(),
) -> Result<bool, String> {
    under_lock();
    let Some(text) = read_snapshot(snapshot)? else {
        return Ok(false);
    };
    let taken = parse_snapshot(&text)?;
    write_slot(place, true, &taken.dark)?;
    write_slot(place, false, &taken.light)?;
    // After the slots, so COSMIC switches to one already put back.
    if let Some(mode) = &taken.mode {
        write_mode(place, mode)?;
    }
    // Only once everything is back: a failed restore can be tried again.
    fs::remove_file(snapshot).map_err(|error| error.to_string())?;
    Ok(true)
}

/// The snapshot, checked as theme files are: it goes straight into
/// COSMIC's theme config, which every COSMIC app reads. A parse error gives
/// only its position, never the file's text.
fn parse_snapshot(text: &str) -> Result<Snapshot, String> {
    let unreadable =
        |line: usize, column: usize| fl!("snapshot-unreadable", line = line, column = column);
    match library::check_finite(text) {
        Ok(()) => {}
        Err(ReadError::Parse { line, column }) => return Err(unreadable(line, column)),
        Err(_) => return Err(fl!("snapshot-not-finite")),
    }
    ron::from_str(text).map_err(|error| match library::parse_error(&error) {
        ReadError::Parse { line, column } => unreadable(line, column),
        other => other.to_string(),
    })
}

/// The snapshot's text, if there is one. Opened without following a link
/// and without blocking, and checked on the open file, so a link, named
/// pipe or device in its place can't redirect or hang `--restore` or fill
/// memory; the read is bounded too.
fn read_snapshot(snapshot: &Path) -> Result<Option<String>, String> {
    let file = match OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK | libc::O_NOFOLLOW)
        .open(snapshot)
    {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(not_a_file_or(&error)),
    };
    let metadata = file.metadata().map_err(|error| error.to_string())?;
    if !metadata.is_file() {
        return Err(fl!("not-a-file"));
    }
    if metadata.len() > MAX_SNAPSHOT_BYTES {
        return Err(fl!("snapshot-too-large"));
    }
    let mut text = String::new();
    file.take(MAX_SNAPSHOT_BYTES + 1)
        .read_to_string(&mut text)
        .map_err(|error| error.to_string())?;
    if text.len() as u64 > MAX_SNAPSHOT_BYTES {
        return Err(fl!("snapshot-too-large"));
    }
    Ok(Some(text))
}

/// Writes one slot's builder, then its theme, changed keys only and in
/// parallel (see `fastwrite.rs`), synced to disk or not as `durability`
/// says. Blocking.
///
/// # Errors
/// Why a key couldn't be written.
pub(crate) fn write_slot_entries(
    place: &Where,
    dark: bool,
    builder: &ThemeBuilder,
    theme: &Theme,
    durability: Durability,
) -> Result<(), String> {
    let (theme_id, builder_id) = ids(dark);
    let no_folder = || fl!("restore-no-home");
    let builder_folder = place
        .folder(builder_id, ThemeBuilder::VERSION)
        .ok_or_else(no_folder)?;
    let theme_folder = place
        .folder(theme_id, Theme::VERSION)
        .ok_or_else(no_folder)?;
    fastwrite::write_entry(&builder_folder, builder, durability)
        .map_err(|error| error.user_message())?;
    fastwrite::write_entry(&theme_folder, theme, durability)
        .map_err(|error| error.user_message())?;
    Ok(())
}

fn write_slot(place: &Where, dark: bool, slot: &Slot) -> Result<(), String> {
    let (theme_id, builder_id) = ids(dark);
    // Synced: the snapshot is deleted once the restore is done.
    write_slot_entries(place, dark, &slot.builder, &slot.theme, Durability::Synced)?;
    // Every key now holds the snapshot's value, which apps have seen.
    // Removing the ones the folder didn't hold lets COSMIC read the older
    // folder or the defaults again for them; they hold the same values, so
    // nothing changes on screen.
    remove_added_keys(
        place.folder(builder_id, ThemeBuilder::VERSION),
        slot.builder_keys.as_deref(),
    );
    remove_added_keys(
        place.folder(theme_id, Theme::VERSION),
        slot.theme_keys.as_deref(),
    );
    Ok(())
}

/// Removes the keys in `folder` that aren't in `kept`; nothing when either
/// is unknown. Only key files are touched (a symlinked key loses just its
/// link); failures are left for the user to see in COSMIC Settings rather
/// than stopping the restore.
fn remove_added_keys(folder: Option<PathBuf>, kept: Option<&[String]>) {
    let (Some(folder), Some(kept)) = (folder, kept) else {
        return;
    };
    for key in keys_in(Some(&folder)).unwrap_or_default() {
        if !kept.contains(&key) {
            let _ = fs::remove_file(folder.join(&key));
        }
    }
}

/// `$XDG_CONFIG_HOME`, else `~/.config`, as cosmic-config finds it.
fn config_home() -> Option<PathBuf> {
    absolute_env("XDG_CONFIG_HOME").or_else(|| Some(absolute_env("HOME")?.join(".config")))
}

/// `$XDG_STATE_HOME/cosmic-theme-cycler/original-theme.ron`, else under
/// `~/.local/state`.
pub(crate) fn snapshot_path() -> Option<PathBuf> {
    let state = absolute_env("XDG_STATE_HOME")
        .or_else(|| Some(absolute_env("HOME")?.join(".local").join("state")))?;
    Some(state.join("cosmic-theme-cycler").join(SNAPSHOT_FILE))
}

/// An environment variable holding an absolute path; relative ones are
/// ignored, as the XDG spec asks.
fn absolute_env(name: &str) -> Option<PathBuf> {
    let path = PathBuf::from(std::env::var_os(name)?);
    path.is_absolute().then_some(path)
}

/// `cosmic-theme-cycler --restore`: puts the theme back, forgets the
/// applied theme and says what it did. `main` has already refused to run
/// as root.
#[must_use]
pub(crate) fn run() -> ExitCode {
    let Some(snapshot) = snapshot_path() else {
        eprintln!("{}", fl!("restore-no-home"));
        return ExitCode::FAILURE;
    };
    let forget = || {
        if let Err(error) = forget_applied() {
            eprintln!("{}", fl!("restore-settings-failed", error = error));
        }
    };
    match restore(&Where::System, &snapshot, forget) {
        Ok(true) => {
            println!("{}", fl!("restored"));
            ExitCode::SUCCESS
        }
        Ok(false) => {
            println!("{}", fl!("nothing-to-restore"));
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{}", fl!("restore-failed", error = error));
            ExitCode::FAILURE
        }
    }
}

/// Puts the original theme back now, as `--restore` does, without
/// printing: for the applet when it is taken out of the panel. Blocking.
///
/// # Errors
/// Why it couldn't be put back.
pub(crate) fn restore_now() -> Result<bool, String> {
    let snapshot = snapshot_path().ok_or_else(|| fl!("restore-no-home"))?;
    restore(&Where::System, &snapshot, forget_quietly)
}

/// The lock beside the real snapshot, for the applet's shutdown: held until
/// the process exits, no theme write can start after it (or be cut short).
pub(crate) fn lock_for_shutdown() -> Option<File> {
    lock(&snapshot_path()?).ok()
}

/// [`restore_now`] for the shutdown, which holds [`lock_for_shutdown`].
pub(crate) fn restore_now_locked() -> Result<bool, String> {
    let snapshot = snapshot_path().ok_or_else(|| fl!("restore-no-home"))?;
    restore_locked(&Where::System, &snapshot, forget_quietly)
}

fn forget_quietly() {
    if let Err(error) = forget_applied() {
        tracing::warn!(%error, "applied theme not forgotten");
    }
}

/// The builder of the theme COSMIC showed before the applet first changed
/// it, from the snapshot; `None` if there is none (nothing changed yet) or
/// it can't be read. Blocking.
pub(crate) fn original_builder() -> Option<ThemeBuilder> {
    let text = read_snapshot(&snapshot_path()?).ok()??;
    let taken = parse_snapshot(&text).ok()?;
    // A snapshot without the light/dark setting (see `Snapshot::mode`):
    // COSMIC's default, dark.
    let dark = taken.mode.as_ref().is_none_or(|mode| mode.is_dark);
    Some(if dark {
        taken.dark.builder
    } else {
        taken.light.builder
    })
}

/// Forgets the applied theme, so the cycle starts afresh.
fn forget_applied() -> Result<(), String> {
    write_settings(|settings| settings.last_applied = None)
}

/// Loads the applet's settings, changes them with `change`, and writes the
/// keys that changed. Blocking.
fn write_settings(change: impl FnOnce(&mut Settings)) -> Result<(), String> {
    let config = Settings::config().map_err(|error| error.to_string())?;
    let mut settings = Settings::load(&config);
    change(&mut settings);
    let folder = Where::System
        .folder(APP_ID, CONFIG_VERSION)
        .ok_or_else(|| fl!("restore-no-home"))?;
    fastwrite::write_entry(&folder, &settings, Durability::Synced)
        .map(|_| ())
        .map_err(|error| error.user_message())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A config root and a snapshot path in a scratch folder; never the
    /// real config.
    struct Scratch(crate::scratch::Scratch);

    impl Scratch {
        fn new() -> Self {
            Self(crate::scratch::Scratch::new("restore"))
        }

        fn path(&self) -> &Path {
            self.0.path()
        }

        fn place(&self) -> Where {
            Where::Custom(self.path().to_path_buf())
        }

        fn snapshot(&self) -> PathBuf {
            self.path().join("state").join(SNAPSHOT_FILE)
        }

        fn keys(&self, id: &str) -> Vec<String> {
            keys_in(self.place().folder(id, Theme::VERSION).as_deref()).unwrap_or_default()
        }
    }

    fn write_dark(place: &Where, builder: &ThemeBuilder) {
        let config = place
            .config(DARK_THEME_BUILDER_ID, ThemeBuilder::VERSION)
            .unwrap();
        builder.write_entry(&config).unwrap();
        let config = place.config(DARK_THEME_ID, Theme::VERSION).unwrap();
        builder.clone().build().write_entry(&config).unwrap();
    }

    fn dark_theme(place: &Where) -> Theme {
        let config = place.config(DARK_THEME_ID, Theme::VERSION).unwrap();
        Theme::get_entry(&config).unwrap_or_else(|(_, theme)| theme)
    }

    #[test]
    fn a_changed_theme_is_put_back() {
        let scratch = Scratch::new();
        let place = scratch.place();
        let original =
            ThemeBuilder::dark().accent(cosmic::cosmic_theme::palette::Srgb::new(1.0, 0.0, 0.0));
        write_dark(&place, &original);
        let before = dark_theme(&place);
        ensure_snapshot(&place, &scratch.snapshot()).unwrap();
        write_dark(&place, &ThemeBuilder::light());
        assert_ne!(dark_theme(&place), before);

        assert_eq!(restore(&place, &scratch.snapshot(), || ()), Ok(true));
        assert_eq!(dark_theme(&place), before);
        assert!(!scratch.snapshot().exists(), "used up");
    }

    #[test]
    fn an_empty_slot_is_left_empty_again() {
        let scratch = Scratch::new();
        let place = scratch.place();
        ensure_snapshot(&place, &scratch.snapshot()).unwrap();
        write_dark(&place, &ThemeBuilder::light());
        assert_ne!(scratch.keys(DARK_THEME_ID), Vec::<String>::new());

        assert_eq!(restore(&place, &scratch.snapshot(), || ()), Ok(true));
        assert_eq!(scratch.keys(DARK_THEME_ID), Vec::<String>::new());
        assert_eq!(scratch.keys(DARK_THEME_BUILDER_ID), Vec::<String>::new());
    }

    #[test]
    fn the_first_snapshot_is_kept() {
        let scratch = Scratch::new();
        let place = scratch.place();
        write_dark(&place, &ThemeBuilder::dark());
        let original = dark_theme(&place);
        ensure_snapshot(&place, &scratch.snapshot()).unwrap();
        write_dark(&place, &ThemeBuilder::light());
        // A later write must not replace the snapshot with the applet's own
        // theme.
        ensure_snapshot(&place, &scratch.snapshot()).unwrap();
        restore(&place, &scratch.snapshot(), || ()).unwrap();
        assert_eq!(dark_theme(&place), original);
    }

    #[test]
    fn nothing_to_restore_is_not_an_error() {
        let scratch = Scratch::new();
        assert_eq!(
            restore(&scratch.place(), &scratch.snapshot(), || ()),
            Ok(false)
        );
    }

    #[test]
    fn a_broken_snapshot_is_reported_and_kept() {
        let scratch = Scratch::new();
        fs::create_dir_all(scratch.snapshot().parent().unwrap()).unwrap();
        fs::write(scratch.snapshot(), "not a snapshot").unwrap();
        assert!(restore(&scratch.place(), &scratch.snapshot(), || ()).is_err());
        assert!(scratch.snapshot().exists());
    }

    #[test]
    fn symlinked_keys_are_kept_and_restored() {
        // A dotfile manager may link the key files; cosmic-config reads
        // through the links and replaces them with files when it writes.
        let scratch = Scratch::new();
        let place = scratch.place();
        write_dark(&place, &ThemeBuilder::dark());
        let original = dark_theme(&place);
        let folder = place.folder(DARK_THEME_ID, Theme::VERSION).unwrap();
        let dotfiles = scratch.path().join("dotfiles");
        fs::create_dir_all(&dotfiles).unwrap();
        for key in scratch.keys(DARK_THEME_ID) {
            fs::rename(folder.join(&key), dotfiles.join(&key)).unwrap();
            std::os::unix::fs::symlink(dotfiles.join(&key), folder.join(&key)).unwrap();
        }
        ensure_snapshot(&place, &scratch.snapshot()).unwrap();
        write_dark(&place, &ThemeBuilder::light());
        restore(&place, &scratch.snapshot(), || ()).unwrap();
        assert_eq!(dark_theme(&place), original);
        assert_ne!(scratch.keys(DARK_THEME_ID), Vec::<String>::new());
    }

    #[test]
    fn unknown_key_lists_remove_nothing() {
        let scratch = Scratch::new();
        let place = scratch.place();
        write_dark(&place, &ThemeBuilder::light());
        let before = scratch.keys(DARK_THEME_ID);
        let folder = place.folder(DARK_THEME_ID, Theme::VERSION);
        remove_added_keys(None, Some(&[]));
        remove_added_keys(folder, None);
        assert_eq!(scratch.keys(DARK_THEME_ID), before);
    }

    #[test]
    fn a_snapshot_with_a_non_finite_number_is_refused() {
        let scratch = Scratch::new();
        let place = scratch.place();
        ensure_snapshot(&place, &scratch.snapshot()).unwrap();
        let text = fs::read_to_string(scratch.snapshot()).unwrap();
        let broken = text.replacen("0.0", "NaN", 1);
        assert_ne!(broken, text, "the snapshot has a number to replace");
        fs::write(scratch.snapshot(), broken).unwrap();
        assert!(restore(&place, &scratch.snapshot(), || ()).is_err());
        assert!(scratch.snapshot().exists(), "kept for another try");
    }

    #[test]
    fn the_lock_is_exclusive() {
        let scratch = Scratch::new();
        let held = lock(&scratch.snapshot()).unwrap();
        let other = OpenOptions::new()
            .write(true)
            .open(scratch.snapshot().with_file_name(LOCK_FILE))
            .unwrap();
        assert!(other.try_lock().is_err(), "a second holder must wait");
        drop(held);
        // Waits rather than trying once: a test elsewhere that starts a
        // process can hold a copy of the lock until that process execs.
        assert!(other.lock().is_ok());
    }

    #[test]
    fn a_named_pipe_in_place_of_the_snapshot_is_refused_without_waiting() {
        let scratch = Scratch::new();
        fs::create_dir_all(scratch.snapshot().parent().unwrap()).unwrap();
        let made = std::process::Command::new("mkfifo")
            .arg(scratch.snapshot())
            .status();
        assert!(made.is_ok_and(|status| status.success()), "mkfifo failed");
        assert!(read_snapshot(&scratch.snapshot()).is_err());
    }

    #[test]
    fn a_named_pipe_in_place_of_the_lock_is_refused_without_waiting() {
        let scratch = Scratch::new();
        let lock_path = scratch.snapshot().with_file_name(LOCK_FILE);
        fs::create_dir_all(lock_path.parent().unwrap()).unwrap();
        let made = std::process::Command::new("mkfifo")
            .arg(&lock_path)
            .status();
        assert!(made.is_ok_and(|status| status.success()), "mkfifo failed");
        assert!(lock(&scratch.snapshot()).is_err());
    }

    #[test]
    fn a_link_in_place_of_the_snapshot_is_refused() {
        let scratch = Scratch::new();
        let place = scratch.place();
        let elsewhere = scratch.path().join("elsewhere.ron");
        ensure_snapshot(&place, &elsewhere).unwrap();
        fs::create_dir_all(scratch.snapshot().parent().unwrap()).unwrap();
        std::os::unix::fs::symlink(&elsewhere, scratch.snapshot()).unwrap();
        assert_eq!(read_snapshot(&scratch.snapshot()), Err(fl!("not-a-file")));
        assert!(ensure_snapshot(&place, &scratch.snapshot()).is_err());
    }

    #[test]
    fn a_broken_snapshot_is_reported_by_position_only() {
        let error = parse_snapshot("(dark: Secret(),)").unwrap_err();
        assert!(!error.contains("Secret"), "{error}");
        assert_eq!(
            error.replace(['\u{2068}', '\u{2069}'], ""),
            fl!("snapshot-unreadable", line = 1, column = 8).replace(['\u{2068}', '\u{2069}'], "")
        );
    }

    #[test]
    fn a_folder_in_place_of_the_snapshot_stops_the_write() {
        let scratch = Scratch::new();
        fs::create_dir_all(scratch.snapshot()).unwrap();
        assert!(ensure_snapshot(&scratch.place(), &scratch.snapshot()).is_err());
    }

    #[test]
    fn the_step_under_the_lock_runs_before_the_theme_is_put_back() {
        let scratch = Scratch::new();
        let place = scratch.place();
        write_dark(&place, &ThemeBuilder::dark());
        let original = dark_theme(&place);
        ensure_snapshot(&place, &scratch.snapshot()).unwrap();
        write_dark(&place, &ThemeBuilder::light());
        let mut seen_before = None;
        let restored = restore(&place, &scratch.snapshot(), || {
            seen_before = Some(dark_theme(&place) == original);
        });
        assert_eq!(restored, Ok(true));
        assert_eq!(
            seen_before,
            Some(false),
            "ran before the theme was written back"
        );
        assert_eq!(dark_theme(&place), original);
    }

    fn set_mode(place: &Where, dark: bool) {
        place
            .config(THEME_MODE_ID, ThemeMode::VERSION)
            .unwrap()
            .set(MODE_KEY, dark)
            .unwrap();
    }

    fn mode_keys(place: &Where) -> Vec<String> {
        keys_in(place.folder(THEME_MODE_ID, ThemeMode::VERSION).as_deref()).unwrap_or_default()
    }

    #[test]
    fn an_unset_light_dark_setting_is_left_unset() {
        let scratch = Scratch::new();
        let place = scratch.place();
        // Unset: COSMIC's default, dark.
        ensure_snapshot(&place, &scratch.snapshot()).unwrap();
        set_mode(&place, false);
        assert!(!read_mode(&place).is_dark);
        restore(&place, &scratch.snapshot(), || ()).unwrap();
        assert!(read_mode(&place).is_dark);
        assert_eq!(mode_keys(&place), Vec::<String>::new(), "unset again");
    }

    #[test]
    fn a_set_light_dark_setting_is_put_back_as_set() {
        let scratch = Scratch::new();
        let place = scratch.place();
        set_mode(&place, false);
        ensure_snapshot(&place, &scratch.snapshot()).unwrap();
        set_mode(&place, true);
        restore(&place, &scratch.snapshot(), || ()).unwrap();
        assert!(!read_mode(&place).is_dark);
        assert_eq!(mode_keys(&place), [MODE_KEY]);
    }

    #[test]
    fn an_older_snapshot_gets_the_light_dark_setting_added() {
        let scratch = Scratch::new();
        let place = scratch.place();
        let older = Snapshot {
            dark: read_slot(&place, true).unwrap(),
            light: read_slot(&place, false).unwrap(),
            mode: None,
        };
        let text = ron::ser::to_string_pretty(&older, ron::ser::PrettyConfig::default()).unwrap();
        fs::create_dir_all(scratch.snapshot().parent().unwrap()).unwrap();
        fs::write(scratch.snapshot(), text).unwrap();
        ensure_snapshot(&place, &scratch.snapshot()).unwrap();
        let updated: Snapshot =
            ron::from_str(&fs::read_to_string(scratch.snapshot()).unwrap()).unwrap();
        assert_eq!(
            updated.mode,
            Some(ModeRecord {
                is_dark: true,
                had_key: Some(false)
            })
        );
    }
}
