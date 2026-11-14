//! Writing a cosmic-config entry quickly: only the keys whose text changed,
//! all at once.
//!
//! cosmic-config's `write_entry` writes every key of an entry, one after
//! another, each through an atomic file synced to disk. A synced write can
//! take a large fraction of a second, and much longer while something else
//! is writing heavily to the disk, so a COSMIC theme (61 keys across theme
//! and builder) can take many seconds to apply that way. Here each key's
//! text is produced exactly as cosmic-config would write it, compared with
//! the file already there, and the changed ones are written the way
//! cosmic-config writes a key: a temporary file in the same folder, then
//! renamed over the key. The temporary files are written in parallel and
//! the renames done together, so COSMIC sees the change at once.
//!
//! Whether the files are synced to disk first is the caller's choice
//! ([`Durability`]). A theme being shown doesn't need it: the rename alone
//! means a reader sees the old file or the new one, never half of either.
//! Syncing protects only against a power cut or crash in the seconds after
//! a click (when a key could come back as before, or empty, and COSMIC uses
//! its default for it), at the cost of the click waiting on a busy disk. What
//! `--restore` writes back is synced, since it then deletes its record.
//! Running apps see the renames as they see cosmic-config's own writes. The
//! temporary names start with `.atomicwrite`, which cosmic-config's watcher
//! skips, as it does for its own. Tests check the files match
//! `write_entry`'s byte for byte.

use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use serde::Serialize;
use serde::ser::{self, Impossible, SerializeStruct};

/// One key of an entry and its text as cosmic-config writes it.
pub(crate) type Key = (&'static str, String);

/// Whether a write waits until it is on disk.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Durability {
    /// Synced to disk before returning, as cosmic-config writes.
    Synced,
    /// Atomic, but left for the system to write out: never waits on a busy
    /// disk.
    Unsynced,
}

/// Why keys couldn't be written.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum WriteError {
    /// The file system refused, e.g. a full disk; the detail names the key
    /// when there is one.
    Io(String),
    /// Something that can't happen in a working applet, such as a writer
    /// thread stopping or a value that isn't a config entry. The detail is
    /// for the log, not for the user.
    Internal(String),
}

impl WriteError {
    /// The text to show the user: the file system's own error, or for an
    /// internal error a general message, the detail going to the log.
    pub(crate) fn user_message(&self) -> String {
        match self {
            Self::Io(detail) => detail.clone(),
            Self::Internal(detail) => crate::internal_error(detail),
        }
    }
}

impl fmt::Display for WriteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(detail) | Self::Internal(detail) => f.write_str(detail),
        }
    }
}

/// Each field of `entry` as a cosmic-config key: its name and the text
/// `ron::ser::to_string_pretty` gives it with `PrettyConfig::new()`, as
/// cosmic-config writes it. Fields cosmic-config writes through a colour
/// wrapper use the same wrapper in their serde form.
///
/// # Errors
/// If `entry` isn't a struct or a field can't be written as RON.
pub(crate) fn keys<T: Serialize>(entry: &T) -> Result<Vec<Key>, WriteError> {
    entry
        .serialize(Capture)
        .map_err(|error| WriteError::Internal(error.0))
}

/// The keys in `new` whose text differs from `old`'s.
pub(crate) fn changed<'a>(old: &[Key], new: &'a [Key]) -> Vec<&'a Key> {
    new.iter().filter(|key| !old.contains(key)).collect()
}

/// Key files bigger than this aren't read to compare, just replaced; a
/// theme key is at most a few KiB.
const MAX_KEY_BYTES: u64 = 1 << 20;

/// Writes the keys of `entry` whose text differs from the files in
/// `folder` (a missing or unreadable file counts as different). Returns how
/// many were written. Blocking.
///
/// # Errors
/// Why the keys couldn't be written; then none of them is.
pub(crate) fn write_entry<T: Serialize>(
    folder: &Path,
    entry: &T,
    durability: Durability,
) -> Result<usize, WriteError> {
    let keys = keys(entry)?;
    let changed: Vec<&Key> = keys
        .iter()
        .filter(|(name, text)| read_key(&folder.join(name)).as_ref() != Some(text))
        .collect();
    write_keys(folder, &changed, durability)?;
    if durability == Durability::Synced {
        // Keys that already held their text may have been written unsynced
        // (a preview put back), so they are synced too, and the folder.
        let unchanged = keys
            .iter()
            .filter(|key| !changed.iter().any(|changed| changed.0 == key.0));
        for (name, _) in unchanged {
            sync_file(&folder.join(name))
                .map_err(|error| WriteError::Io(format!("{name}: {error}")))?;
        }
        sync_file(folder).map_err(|error| WriteError::Io(error.to_string()))?;
    }
    Ok(changed.len())
}

/// Syncs a file or folder already there to disk. Opened without blocking
/// or following a link, so a named pipe or link planted at a key path is
/// skipped rather than waited on.
fn sync_file(path: &Path) -> std::io::Result<()> {
    let file = match OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK | libc::O_NOFOLLOW)
        .open(path)
    {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) if error.raw_os_error() == Some(libc::ELOOP) => return Ok(()),
        Err(error) => return Err(error),
    };
    let kind = file.metadata()?.file_type();
    if kind.is_file() || kind.is_dir() {
        file.sync_all()?;
    }
    Ok(())
}

/// Writes the one key `name` of `entry` into `folder`, if its text there
/// differs. Returns whether it was written. Blocking.
///
/// # Errors
/// Why the key couldn't be written, or `entry` has no key `name`.
pub(crate) fn write_key<T: Serialize>(
    folder: &Path,
    entry: &T,
    name: &str,
    durability: Durability,
) -> Result<bool, WriteError> {
    let keys = keys(entry)?;
    let key = keys
        .iter()
        .find(|(key, _)| *key == name)
        .ok_or_else(|| WriteError::Internal(format!("{name}: no such key")))?;
    if read_key(&folder.join(name)).as_ref() == Some(&key.1) {
        return Ok(false);
    }
    write_keys(folder, &[key], durability)?;
    Ok(true)
}

/// A key file's text, if it is a regular file of a sane size. Opened
/// without blocking and checked on the open file, as cosmic-config only
/// reads regular files: a named pipe planted at a key path must not hang a
/// theme write or `--restore`. Anything else reads as `None`, so the key is
/// written, which replaces it as cosmic-config's own write would.
fn read_key(path: &Path) -> Option<String> {
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK)
        .open(path)
        .ok()?;
    let metadata = file.metadata().ok()?;
    if !metadata.is_file() || metadata.len() > MAX_KEY_BYTES {
        return None;
    }
    let mut text = String::new();
    file.take(MAX_KEY_BYTES).read_to_string(&mut text).ok()?;
    Some(text)
}

/// Writes `keys` into `folder`, creating it if needed: first every key to
/// its own temporary file, in parallel; then, only if all of them were
/// written, every rename one straight after the other, so COSMIC sees the
/// whole change at once. With [`Durability::Synced`], each temporary file is
/// synced before the renames and the folder after them, so the change is on
/// disk when this returns.
///
/// # Errors
/// Why a key couldn't be written; then no key is replaced.
pub(crate) fn write_keys(
    folder: &Path,
    keys: &[&Key],
    durability: Durability,
) -> Result<(), WriteError> {
    if keys.is_empty() {
        return Ok(());
    }
    fs::create_dir_all(folder).map_err(|error| WriteError::Io(error.to_string()))?;
    let prepared = prepare_all(folder, keys, durability);
    let temporaries: Vec<PathBuf> = prepared
        .iter()
        .filter_map(|(_, temporary)| temporary.clone().ok())
        .collect();
    if let Some(error) = prepared
        .iter()
        .find_map(|(_, temporary)| temporary.as_ref().err())
    {
        for temporary in &temporaries {
            let _ = fs::remove_file(temporary);
        }
        return Err(error.clone());
    }
    let mut renamed = Ok(());
    for (name, temporary) in prepared {
        let temporary = temporary.expect("checked above that every key was prepared");
        if let Err(error) = fs::rename(&temporary, folder.join(name)) {
            let _ = fs::remove_file(&temporary);
            renamed = renamed.and(Err(WriteError::Io(format!("{name}: {error}"))));
        }
    }
    if durability == Durability::Unsynced {
        return renamed;
    }
    let synced = File::open(folder)
        .and_then(|folder| folder.sync_all())
        .map_err(|error| WriteError::Io(error.to_string()));
    renamed.and(synced)
}

/// Each key written to a temporary file, all at once.
fn prepare_all<'a>(
    folder: &Path,
    keys: &[&'a Key],
    durability: Durability,
) -> Vec<(&'a str, Result<PathBuf, WriteError>)> {
    std::thread::scope(|scope| {
        let writers: Vec<_> = keys
            .iter()
            .map(|(name, text)| {
                // A thread that can't start is an error to report, not a
                // panic that would take the caller down.
                let writer = std::thread::Builder::new()
                    .spawn_scoped(scope, move || prepare(folder, name, text, durability))
                    .map_err(|error| {
                        WriteError::Internal(format!("{name}: no writer thread: {error}"))
                    });
                (*name, writer)
            })
            .collect();
        writers
            .into_iter()
            .map(|(name, writer)| {
                let prepared = writer.and_then(|writer| {
                    writer.join().unwrap_or_else(|_| {
                        Err(WriteError::Internal(format!("{name}: its writer stopped")))
                    })
                });
                (name, prepared)
            })
            .collect()
    })
}

/// One key's text in a new temporary file in `folder`, named so
/// cosmic-config's watcher skips it; synced if `durability` says so.
fn prepare(
    folder: &Path,
    name: &str,
    text: &str,
    durability: Durability,
) -> Result<PathBuf, WriteError> {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let temporary = folder.join(format!(
        ".atomicwrite-cycler-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    let written = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        file.write_all(text.as_bytes())?;
        if durability == Durability::Synced {
            file.sync_all()?;
        }
        Ok::<(), std::io::Error>(())
    })();
    match written {
        Ok(()) => Ok(temporary),
        Err(error) => {
            let _ = fs::remove_file(&temporary);
            Err(WriteError::Io(format!("{name}: {error}")))
        }
    }
}

/// A serializer that accepts only a struct and returns each field's RON.
struct Capture;

#[derive(Debug)]
struct CaptureError(String);

impl fmt::Display for CaptureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for CaptureError {}

/// What every serializer method but `serialize_struct` returns: only an
/// entry struct has keys.
fn not_an_entry() -> CaptureError {
    CaptureError("not a config entry".into())
}

impl ser::Error for CaptureError {
    fn custom<T: fmt::Display>(message: T) -> Self {
        Self(message.to_string())
    }
}

struct Fields(Vec<Key>);

impl SerializeStruct for Fields {
    type Ok = Vec<Key>;
    type Error = CaptureError;

    fn serialize_field<T: ?Sized + Serialize>(
        &mut self,
        key: &'static str,
        value: &T,
    ) -> Result<(), CaptureError> {
        let text = ron::ser::to_string_pretty(value, ron::ser::PrettyConfig::new())
            .map_err(|error| CaptureError(format!("{key}: {error}")))?;
        self.0.push((key, text));
        Ok(())
    }

    fn end(self) -> Result<Vec<Key>, CaptureError> {
        Ok(self.0)
    }
}

/// Every serializer method but `serialize_struct` refuses: only an entry
/// struct has keys.
macro_rules! not_a_struct {
    ($($method:ident($($argument:ty),*)),* $(,)?) => {
        $(fn $method(self, $(_: $argument),*) -> Result<Vec<Key>, CaptureError> {
            Err(not_an_entry())
        })*
    };
}

impl ser::Serializer for Capture {
    type Ok = Vec<Key>;
    type Error = CaptureError;
    type SerializeSeq = Impossible<Vec<Key>, CaptureError>;
    type SerializeTuple = Impossible<Vec<Key>, CaptureError>;
    type SerializeTupleStruct = Impossible<Vec<Key>, CaptureError>;
    type SerializeTupleVariant = Impossible<Vec<Key>, CaptureError>;
    type SerializeMap = Impossible<Vec<Key>, CaptureError>;
    type SerializeStruct = Fields;
    type SerializeStructVariant = Impossible<Vec<Key>, CaptureError>;

    not_a_struct!(
        serialize_bool(bool),
        serialize_i8(i8),
        serialize_i16(i16),
        serialize_i32(i32),
        serialize_i64(i64),
        serialize_u8(u8),
        serialize_u16(u16),
        serialize_u32(u32),
        serialize_u64(u64),
        serialize_f32(f32),
        serialize_f64(f64),
        serialize_char(char),
        serialize_str(&str),
        serialize_bytes(&[u8]),
        serialize_none(),
        serialize_unit(),
        serialize_unit_struct(&'static str),
        serialize_unit_variant(&'static str, u32, &'static str),
    );

    fn serialize_some<T: ?Sized + Serialize>(self, _: &T) -> Result<Vec<Key>, CaptureError> {
        Err(not_an_entry())
    }

    fn serialize_newtype_struct<T: ?Sized + Serialize>(
        self,
        _: &'static str,
        _: &T,
    ) -> Result<Vec<Key>, CaptureError> {
        Err(not_an_entry())
    }

    fn serialize_newtype_variant<T: ?Sized + Serialize>(
        self,
        _: &'static str,
        _: u32,
        _: &'static str,
        _: &T,
    ) -> Result<Vec<Key>, CaptureError> {
        Err(not_an_entry())
    }

    fn serialize_seq(self, _: Option<usize>) -> Result<Self::SerializeSeq, CaptureError> {
        Err(not_an_entry())
    }

    fn serialize_tuple(self, _: usize) -> Result<Self::SerializeTuple, CaptureError> {
        Err(not_an_entry())
    }

    fn serialize_tuple_struct(
        self,
        _: &'static str,
        _: usize,
    ) -> Result<Self::SerializeTupleStruct, CaptureError> {
        Err(not_an_entry())
    }

    fn serialize_tuple_variant(
        self,
        _: &'static str,
        _: u32,
        _: &'static str,
        _: usize,
    ) -> Result<Self::SerializeTupleVariant, CaptureError> {
        Err(not_an_entry())
    }

    fn serialize_map(self, _: Option<usize>) -> Result<Self::SerializeMap, CaptureError> {
        Err(not_an_entry())
    }

    fn serialize_struct(
        self,
        _: &'static str,
        fields: usize,
    ) -> Result<Self::SerializeStruct, CaptureError> {
        Ok(Fields(Vec::with_capacity(fields)))
    }

    fn serialize_struct_variant(
        self,
        _: &'static str,
        _: u32,
        _: &'static str,
        _: usize,
    ) -> Result<Self::SerializeStructVariant, CaptureError> {
        Err(not_an_entry())
    }
}

#[cfg(test)]
mod tests {
    use cosmic::cosmic_config::{self, CosmicConfigEntry};
    use cosmic::cosmic_theme::{Theme, ThemeBuilder};
    use cycler_core::settings::Settings;

    use super::*;
    use crate::scratch::Scratch;

    /// The files `write_entry` of cosmic-config produces for `entry`.
    fn cosmic_config_files<T: CosmicConfigEntry>(
        entry: &T,
        id: &str,
        version: u64,
    ) -> Vec<(String, String)> {
        let scratch = Scratch::new("fastwrite-reference");
        let config =
            cosmic_config::Config::with_custom_path(id, version, scratch.path().to_path_buf())
                .unwrap();
        entry.write_entry(&config).unwrap();
        files(
            &scratch
                .path()
                .join("cosmic")
                .join(id)
                .join(format!("v{version}")),
        )
    }

    fn files(folder: &Path) -> Vec<(String, String)> {
        let mut files: Vec<(String, String)> = fs::read_dir(folder)
            .unwrap()
            .map(|entry| {
                let entry = entry.unwrap();
                let name = entry.file_name().into_string().unwrap();
                (name, fs::read_to_string(entry.path()).unwrap())
            })
            .collect();
        files.sort();
        files
    }

    fn assert_same_as_cosmic_config<T: CosmicConfigEntry + Serialize>(
        entry: &T,
        id: &str,
        version: u64,
    ) {
        let scratch = Scratch::new("fastwrite");
        let folder = scratch.path().join("v");
        write_entry(&folder, entry, Durability::Synced).unwrap();
        assert_eq!(files(&folder), cosmic_config_files(entry, id, version));
    }

    #[test]
    fn one_key_is_written_alone_and_only_when_it_differs() {
        #[derive(Serialize)]
        struct Mode {
            is_dark: bool,
        }
        for is_dark in [false, true] {
            let mode = cosmic::cosmic_theme::ThemeMode {
                is_dark,
                auto_switch: true,
            };
            let reference = cosmic_config_files(&mode, "mode", 1);
            let scratch = Scratch::new("fastwrite-key");
            let written = write_key(
                scratch.path(),
                &Mode { is_dark },
                "is_dark",
                Durability::Unsynced,
            );
            assert_eq!(written, Ok(true));
            let expected: Vec<_> = reference
                .into_iter()
                .filter(|(name, _)| name == "is_dark")
                .collect();
            assert_eq!(
                files(scratch.path()),
                expected,
                "as cosmic-config writes it"
            );
            let again = write_key(
                scratch.path(),
                &Mode { is_dark },
                "is_dark",
                Durability::Unsynced,
            );
            assert_eq!(again, Ok(false), "unchanged, not written");
        }
    }

    #[test]
    fn an_unknown_key_is_an_error_not_a_write() {
        let scratch = Scratch::new("fastwrite-key");
        let written = write_key(
            scratch.path(),
            &Settings::default(),
            "no_such_key",
            Durability::Unsynced,
        );
        assert!(matches!(written, Err(WriteError::Internal(_))));
        assert_eq!(files(scratch.path()), Vec::<(String, String)>::new());
    }

    #[test]
    fn themes_are_written_byte_for_byte_as_cosmic_config_does() {
        for builder in [ThemeBuilder::dark(), ThemeBuilder::light()] {
            assert_same_as_cosmic_config(&builder, "builder", ThemeBuilder::VERSION);
            assert_same_as_cosmic_config(&builder.build(), "theme", Theme::VERSION);
        }
    }

    #[test]
    fn settings_are_written_byte_for_byte_as_cosmic_config_does() {
        let settings = Settings {
            selected: vec![
                "system:nebula-dark".into(),
                "user:ünïcode \"quoted\"".into(),
            ],
            last_applied: Some("system:nebula-dark".into()),
            switch_history: vec![cycler_core::habit::SwitchRecord {
                variant: cycler_core::Variant::Light,
                offset_minutes: 15,
            }],
        };
        assert_same_as_cosmic_config(&settings, "settings", 1);
    }

    #[test]
    fn only_changed_keys_are_written() {
        let scratch = Scratch::new("fastwrite-changed");
        let dark = ThemeBuilder::dark().build();
        assert_eq!(
            write_entry(scratch.path(), &dark, Durability::Synced),
            Ok(keys(&dark).unwrap().len())
        );
        assert_eq!(
            write_entry(scratch.path(), &dark, Durability::Synced),
            Ok(0),
            "nothing changed"
        );
        let light = ThemeBuilder::light().build();
        let differing = changed(&keys(&dark).unwrap(), &keys(&light).unwrap()).len();
        assert!(differing > 0 && differing < keys(&light).unwrap().len());
        assert_eq!(
            write_entry(scratch.path(), &light, Durability::Unsynced),
            Ok(differing)
        );
        assert!(
            fs::read_dir(scratch.path()).unwrap().all(|entry| !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with('.')),
            "no temporary file left"
        );
    }

    #[test]
    fn a_value_that_is_not_a_struct_is_refused() {
        assert!(keys(&42).is_err());
    }

    #[test]
    fn a_named_pipe_at_a_key_is_replaced_without_waiting() {
        let scratch = Scratch::new("fastwrite-fifo");
        let pipe = scratch.path().join("is_dark");
        let made = std::process::Command::new("mkfifo").arg(&pipe).status();
        assert!(made.is_ok_and(|status| status.success()), "mkfifo failed");
        let theme = ThemeBuilder::dark().build();
        write_entry(scratch.path(), &theme, Durability::Synced).unwrap();
        assert!(
            fs::metadata(&pipe).unwrap().is_file(),
            "replaced by the key"
        );
    }
}
