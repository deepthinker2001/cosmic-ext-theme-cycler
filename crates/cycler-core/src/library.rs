//! Finding, reading and importing COSMIC theme files.
//!
//! A theme file is an exported `cosmic_theme::ThemeBuilder` in RON, the
//! format COSMIC Settings imports and exports. Themes come from the system
//! presets folder and from the user's own library, which **Import theme…**
//! fills.
//!
//! Every function that touches the disk takes its folders as arguments, so
//! the applet passes [`default_folders`] and tests pass temporary folders.
//! The functions block on file I/O; the applet runs them off its UI thread
//! so the popup never waits on them.

use std::ffi::OsStr;
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime};

use cosmic_theme::ThemeBuilder;
use cosmic_theme::palette::Srgba;

use crate::Variant;

/// Where COSMIC installs its theme presets.
pub const SYSTEM_THEMES_DIR: &str = "/usr/share/cosmic-themes";

/// The user's library, relative to the XDG data folder.
pub const USER_LIBRARY_SUBDIR: &str = "cosmic-theme-cycler/themes";

/// The largest theme file read. Exported themes are about 6 KiB; the bound
/// keeps a stray large file from stalling a scan or filling memory.
pub const MAX_THEME_FILE_BYTES: u64 = 1024 * 1024;

/// The longest file stem an imported theme gets, in bytes. Well
/// under the usual 255-byte file name limit, leaving room for the extension
/// and the temporary file's prefix and suffix.
const MAX_STEM_BYTES: usize = 100;

const THEME_EXTENSION: &str = "ron";

/// How old a temporary file must be before a scan removes it. An import
/// takes well under a second; an hour leaves a wide margin for one still running
/// in another process, even on a slow or stalled disk.
const STALE_TEMP_AGE: Duration = Duration::from_hours(1);

/// Where a theme came from. Themes of the same name are listed in this
/// order: the original theme, built-in, system, then the user's.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum Source {
    /// The theme COSMIC showed before the applet first changed it.
    Original,
    /// Comes with the applet: COSMIC's defaults and ten others (see
    /// [`crate::builtin`]).
    Builtin,
    /// A preset installed with COSMIC; read only.
    System,
    /// A theme in the user's library, imported.
    User,
}

impl Source {
    fn key_prefix(self) -> &'static str {
        match self {
            Source::Original => "original",
            Source::Builtin => "builtin",
            Source::System => "system",
            Source::User => "user",
        }
    }
}

/// A folder to look for themes in, and what kind of themes it holds.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Folder {
    pub path: PathBuf,
    pub source: Source,
}

/// A theme file that was read and parsed.
#[derive(Clone, Debug, PartialEq)]
pub struct Theme {
    /// Identifies the theme in the settings, e.g. `system:nebula-dark`. The
    /// source is part of it so a user theme named like a preset stays a
    /// separate entry.
    pub key: String,
    /// A readable name made from the file name, e.g. "Nebula Dark".
    pub name: String,
    pub source: Source,
    pub path: PathBuf,
    pub variant: Variant,
    /// The background colour COSMIC shows for the theme; the list orders
    /// each kind from the brightest background to the darkest.
    pub background: Srgba,
    /// The parsed theme, as applying it needs it.
    pub builder: ThemeBuilder,
}

/// Why a theme file was skipped.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ReadError {
    /// Bigger than [`MAX_THEME_FILE_BYTES`]; not read.
    TooLarge { bytes: u64 },
    /// Not a regular file (a folder, a pipe or a device).
    NotAFile,
    /// The file or its folder couldn't be read.
    Io(String),
    /// Not a COSMIC theme, or a damaged one: parsing stopped at this line
    /// and column (1-based). The file's text is never quoted, since a
    /// `.ron` link to some other file would carry its contents along.
    Parse { line: usize, column: usize },
    /// A colour or size is not a finite number (`NaN`, infinity, or too
    /// large for COSMIC's 32-bit numbers). Applied, it would reach COSMIC's
    /// theme config, which every COSMIC app reads.
    NotFinite,
    /// An earlier folder already has a theme with the same key, which wins,
    /// so settings never point at two themes.
    Duplicate { kept: PathBuf },
}

impl fmt::Display for ReadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ReadError::TooLarge { bytes } => write!(
                f,
                "file is {bytes} bytes, larger than the {MAX_THEME_FILE_BYTES}-byte limit"
            ),
            ReadError::NotAFile => f.write_str("not a regular file"),
            ReadError::Io(message) => write!(f, "cannot read: {message}"),
            ReadError::Parse { line, column } => write!(
                f,
                "not a COSMIC theme: unexpected content at line {line}, column {column}"
            ),
            ReadError::NotFinite => f.write_str("a colour or size is not a finite number"),
            ReadError::Duplicate { kept } => {
                write!(f, "same name as {}, which is used", kept.display())
            }
        }
    }
}

impl std::error::Error for ReadError {}

/// A file or folder skipped during a scan, and why.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Unreadable {
    pub path: PathBuf,
    pub reason: ReadError,
}

/// What a scan found: the themes, and the files it had to skip.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Scan {
    /// Sorted by name, then system before user.
    pub themes: Vec<Theme>,
    /// Sorted by path.
    pub unreadable: Vec<Unreadable>,
}

/// Why a theme couldn't be added to the library.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LibraryError {
    /// The name has nothing usable for a file name once unsafe characters
    /// are dropped.
    InvalidName,
    /// A theme of that name is already in the library; it is never replaced
    /// or renamed.
    AlreadyExists(PathBuf),
    /// The file to import isn't a theme that can be used.
    Theme(ReadError),
    /// The library folder or the new file couldn't be written.
    Io(String),
}

impl fmt::Display for LibraryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LibraryError::InvalidName => f.write_str("the name has no usable characters"),
            LibraryError::AlreadyExists(path) => {
                write!(f, "{} already exists", path.display())
            }
            LibraryError::Theme(reason) => reason.fmt(f),
            LibraryError::Io(message) => write!(f, "cannot write: {message}"),
        }
    }
}

impl std::error::Error for LibraryError {}

impl From<io::Error> for LibraryError {
    fn from(error: io::Error) -> Self {
        LibraryError::Io(error.to_string())
    }
}

/// The folders themes are found in: the system presets, then the
/// user's library if a data folder is known.
#[must_use]
pub fn default_folders() -> Vec<Folder> {
    let mut folders = vec![Folder {
        path: PathBuf::from(SYSTEM_THEMES_DIR),
        source: Source::System,
    }];
    if let Some(path) = user_library_dir() {
        folders.push(Folder {
            path,
            source: Source::User,
        });
    }
    folders
}

/// The user's library folder, from `$XDG_DATA_HOME` or `$HOME`. `None` if
/// neither is usable.
#[must_use]
pub fn user_library_dir() -> Option<PathBuf> {
    library_dir_from_env(
        std::env::var_os("XDG_DATA_HOME").as_deref(),
        std::env::var_os("HOME").as_deref(),
    )
}

/// The library folder for these environment values. The XDG spec says a
/// relative `$XDG_DATA_HOME` is invalid and must be ignored; a relative
/// `$HOME` would put the library wherever the applet happened to start.
fn library_dir_from_env(xdg_data_home: Option<&OsStr>, home: Option<&OsStr>) -> Option<PathBuf> {
    let absolute = |value: &OsStr| Some(PathBuf::from(value)).filter(|path| path.is_absolute());
    xdg_data_home
        .and_then(absolute)
        .or_else(|| {
            home.and_then(absolute)
                .map(|home| home.join(".local/share"))
        })
        .map(|data| data.join(USER_LIBRARY_SUBDIR))
}

/// Reads every theme in `folders`, in order. A missing folder is skipped
/// quietly (the user library doesn't exist until something is added); any
/// other problem is reported in [`Scan::unreadable`] and the scan goes on.
#[must_use]
pub fn scan(folders: &[Folder]) -> Scan {
    let mut found = Scan::default();
    for folder in folders {
        scan_folder(folder, &mut found);
    }
    found
        .themes
        .sort_by_cached_key(|theme| (theme.name.to_lowercase(), theme.source, theme.key.clone()));
    found.unreadable.sort_by(|a, b| a.path.cmp(&b.path));
    found
}

/// The built-in themes and those in `folders`, sorted together by name.
/// Blocking: reads the folders and COSMIC's defaults.
#[must_use]
pub fn scan_with_builtins(folders: &[Folder]) -> Scan {
    let mut found = scan(folders);
    found.themes.extend(crate::builtin::themes());
    found
        .themes
        .sort_by_cached_key(|theme| (theme.name.to_lowercase(), theme.source, theme.key.clone()));
    found
}

fn scan_folder(folder: &Folder, found: &mut Scan) {
    // Only the user library: this module never writes to the system folder,
    // so nothing there is one of its temporary files.
    if folder.source == Source::User {
        remove_stale_temp_files(&folder.path, SystemTime::now());
    }
    for path in theme_paths(&folder.path, &mut found.unreadable) {
        match load_theme(&path, folder.source) {
            Ok(theme) => add_unless_duplicate(theme, found),
            Err(reason) => found.unreadable.push(Unreadable { path, reason }),
        }
    }
}

/// The `.ron` files in `dir`, sorted so a scan always visits them in the
/// same order. Other files (COSMIC ships a `.png` preview beside each
/// preset) are not themes and are ignored without a notice.
fn theme_paths(dir: &Path, unreadable: &mut Vec<Unreadable>) -> Vec<PathBuf> {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Vec::new(),
        Err(error) => {
            unreadable.push(io_unreadable(dir.to_path_buf(), &error));
            return Vec::new();
        }
    };
    let mut paths = Vec::new();
    for entry in entries {
        match entry {
            Ok(entry) if has_theme_extension(&entry.path()) => paths.push(entry.path()),
            Ok(_) => {}
            Err(error) => unreadable.push(io_unreadable(dir.to_path_buf(), &error)),
        }
    }
    paths.sort();
    paths
}

fn io_unreadable(path: PathBuf, error: &io::Error) -> Unreadable {
    Unreadable {
        path,
        reason: ReadError::Io(error.to_string()),
    }
}

fn has_theme_extension(path: &Path) -> bool {
    path.extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case(THEME_EXTENSION))
}

fn add_unless_duplicate(theme: Theme, found: &mut Scan) {
    match found.themes.iter().find(|kept| kept.key == theme.key) {
        Some(kept) => found.unreadable.push(Unreadable {
            reason: ReadError::Duplicate {
                kept: kept.path.clone(),
            },
            path: theme.path,
        }),
        None => found.themes.push(theme),
    }
}

/// Reads one theme file and describes it.
///
/// # Errors
///
/// Why the file can't be used; see [`ReadError`].
pub fn load_theme(path: &Path, source: Source) -> Result<Theme, ReadError> {
    let builder = read_theme(path)?;
    let stem = path
        .file_stem()
        .map(OsStr::to_string_lossy)
        .unwrap_or_default();
    Ok(Theme {
        key: theme_key(source, &stem),
        name: display_name(&stem),
        source,
        path: path.to_path_buf(),
        variant: variant_of(&builder),
        background: background_of(&builder),
        builder,
    })
}

/// Reads and parses a theme file, refusing anything over
/// [`MAX_THEME_FILE_BYTES`] before reading it.
///
/// # Errors
///
/// Why the file can't be used; see [`ReadError`].
pub(crate) fn read_theme(path: &Path) -> Result<ThemeBuilder, ReadError> {
    parse_theme(&read_bounded(path)?)
}

fn parse_theme(text: &str) -> Result<ThemeBuilder, ReadError> {
    check_finite(text)?;
    ron::from_str(text).map_err(|error| parse_error(&error))
}

/// Where the text stopped parsing, without ron's own message: that message
/// can quote identifiers and text from the file, and a `.ron` symlink to some
/// other file would carry its contents into the popup and the logs.
#[must_use]
pub fn parse_error(error: &ron::error::SpannedError) -> ReadError {
    let at = &error.span.start;
    ReadError::Parse {
        line: at.line,
        column: at.col,
    }
}

/// Refuses text holding a number that isn't a finite `f32`: `NaN`, an
/// infinity, or a literal too large for an `f32` (which would become
/// infinity). ron accepts all of these for colours and radii.
///
/// The parsed values are checked, not the text, so comments, raw strings
/// and character literals can't hide a number. They are checked before the
/// theme is built because cosmic-theme rounds colours to 8-bit when writing
/// them back, which would hide a NaN.
///
/// # Errors
/// [`ReadError::Parse`] when the text isn't RON at all, and
/// [`ReadError::NotFinite`] for such a number.
pub fn check_finite(text: &str) -> Result<(), ReadError> {
    // Parsed with the same options as the theme itself, so both see the
    // same numbers.
    let value: ron::Value = ron::from_str(text).map_err(|error| parse_error(&error))?;
    if all_finite(&value) {
        Ok(())
    } else {
        Err(ReadError::NotFinite)
    }
}

/// Whether every number in `value` is a finite `f32`.
fn all_finite(value: &ron::Value) -> bool {
    match value {
        ron::Value::Number(number) => {
            let number = number.into_f64();
            number.is_finite() && number.abs() <= f64::from(f32::MAX)
        }
        ron::Value::Option(inner) => inner.as_deref().is_none_or(all_finite),
        ron::Value::Seq(items) => items.iter().all(all_finite),
        ron::Value::Map(map) => map
            .iter()
            .all(|(key, value)| all_finite(key) && all_finite(value)),
        ron::Value::Bool(_)
        | ron::Value::Char(_)
        | ron::Value::String(_)
        | ron::Value::Bytes(_)
        | ron::Value::Unit => true,
    }
}

/// The file's text, if it is a regular file within the size bound.
fn read_bounded(path: &Path) -> Result<String, ReadError> {
    let io_error = |error: io::Error| ReadError::Io(error.to_string());
    // Opened without blocking, then checked on the open file itself: a check
    // by path first could be raced by swapping in a named pipe, whose open
    // would wait for a writer forever. Reads of a regular file ignore the
    // flag.
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK)
        .open(path)
        .map_err(io_error)?;
    let metadata = file.metadata().map_err(io_error)?;
    if !metadata.is_file() {
        return Err(ReadError::NotAFile);
    }
    check_size(metadata.len())?;
    let mut text = String::new();
    // `take` still bounds the read if the file grew after the size check.
    file.take(MAX_THEME_FILE_BYTES + 1)
        .read_to_string(&mut text)
        .map_err(io_error)?;
    check_size(text.len() as u64)?;
    Ok(text)
}

fn check_size(bytes: u64) -> Result<(), ReadError> {
    if bytes > MAX_THEME_FILE_BYTES {
        return Err(ReadError::TooLarge { bytes });
    }
    Ok(())
}

/// The key a theme has in the settings: its source and file stem.
#[must_use]
pub fn theme_key(source: Source, stem: &str) -> String {
    format!("{}:{stem}", source.key_prefix())
}

/// A readable name from a file stem: words split at `-`, `_` and spaces,
/// each starting with a capital, so `nebula-dark` reads "Nebula Dark".
#[must_use]
pub fn display_name(stem: &str) -> String {
    stem.split(['-', '_', ' '])
        .filter(|word| !word.is_empty())
        .map(capitalize)
        .collect::<Vec<_>>()
        .join(" ")
}

fn capitalize(word: &str) -> String {
    let mut chars = word.chars();
    chars
        .next()
        .map(|first| first.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

/// Dark or light, as the theme's palette says.
#[must_use]
pub fn variant_of(builder: &ThemeBuilder) -> Variant {
    if builder.palette.is_dark() {
        Variant::Dark
    } else {
        Variant::Light
    }
}

/// The background colour COSMIC would show for the theme.
///
/// Taken from the built theme, not the builder: the builder's `bg_color` is
/// an optional override, and when it is absent COSMIC derives the colour
/// from the palette. Building resolves that the same way COSMIC does.
#[must_use]
pub fn background_of(builder: &ThemeBuilder) -> Srgba {
    builder.clone().build().bg_color()
}

/// Copies the theme file at `source` into `library`, keeping its file name
/// (made safe). The file is checked first, so the library only ever holds
/// themes that parse, and the bytes checked are the bytes written.
///
/// # Errors
///
/// The file isn't a usable theme, its name has no usable characters, a
/// theme of that name is already in the library, or writing failed.
pub fn import_theme(source: &Path, library: &Path) -> Result<PathBuf, LibraryError> {
    let text = read_bounded(source).map_err(LibraryError::Theme)?;
    // Parsed only to check it; the original text is what gets copied.
    let _checked = parse_theme(&text).map_err(LibraryError::Theme)?;
    let name = source
        .file_stem()
        .map(OsStr::to_string_lossy)
        .unwrap_or_default();
    write_new_theme(library, &name, text.as_bytes())
}

/// A safe file stem for a theme name: lowercase letters and digits joined
/// by single dashes. Every other character is dropped or becomes a dash,
/// so the result has no path separators, no `..` and no leading dot, and
/// [`display_name`] turns it back into a readable name.
#[must_use]
pub(crate) fn file_stem_for(name: &str) -> Option<String> {
    let mut stem = name
        .split(|c: char| !c.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_lowercase)
        .collect::<Vec<_>>()
        .join("-");
    stem.truncate(stem.floor_char_boundary(MAX_STEM_BYTES));
    let stem = stem.trim_end_matches('-');
    (!stem.is_empty()).then(|| stem.to_owned())
}

/// Writes a new theme file without ever replacing an existing one, and
/// without leaving a partial file behind (except after a crash on a file
/// system without hard links; see [`write_in_place`]).
fn write_new_theme(library: &Path, name: &str, contents: &[u8]) -> Result<PathBuf, LibraryError> {
    let stem = file_stem_for(name).ok_or(LibraryError::InvalidName)?;
    let target = library.join(format!("{stem}.{THEME_EXTENSION}"));
    if target.exists() {
        return Err(LibraryError::AlreadyExists(target));
    }
    fs::create_dir_all(library)?;
    publish(library, &stem, &target, contents, |temp, target| {
        fs::hard_link(temp, target)
    })?;
    sync_dir(library);
    Ok(target)
}

/// Writes `contents` under a temporary name, then gives the finished file
/// its final name with `link`. A hard link appears in one step like a
/// rename, but unlike a rename it fails if the name is taken, so a theme
/// imported under the same name at the same moment is never replaced.
///
/// `link` is a parameter so tests can stand in for a file system without
/// hard links, which a test can't mount.
fn publish(
    library: &Path,
    stem: &str,
    target: &Path,
    contents: &[u8],
    link: impl FnOnce(&Path, &Path) -> io::Result<()>,
) -> Result<(), LibraryError> {
    let temp = temp_path(library, stem);
    let linked = write_synced(&temp, contents).map(|()| link(&temp, target));
    // The temporary name is only a staging step; it goes either way.
    let _ = fs::remove_file(&temp);
    match linked? {
        Ok(()) => Ok(()),
        Err(error) if links_unsupported(&error) => write_in_place(target, contents),
        Err(error) => Err(new_file_error(error, target)),
    }
}

/// Whether link(2) failed because the file system has no hard links, as
/// opposed to the name being taken or the disk being full.
///
/// - `EPERM`: what Linux returns when the file system has no `link`
///   operation at all (vfat, exfat; see `vfs_link`), and what some FUSE
///   file systems return.
/// - `EOPNOTSUPP` (the same value as `ENOTSUP` on Linux) and `ENOSYS`: what
///   FUSE and network file systems return for an operation they don't do.
/// - `EXDEV`: the two names are in one folder, so this only comes from a
///   file system that can't link within itself (some union and FUSE mounts).
///
/// A real permission problem also gives `EPERM`; the fallback then fails
/// with its own error, so reading it as "no hard links" costs nothing.
fn links_unsupported(error: &io::Error) -> bool {
    matches!(
        error.raw_os_error(),
        Some(libc::EPERM | libc::EOPNOTSUPP | libc::ENOSYS | libc::EXDEV)
    )
}

/// The fallback for file systems without hard links: writes the final name
/// directly. `create_new` still never replaces an existing theme.
///
/// Unlike the hard link this is not atomic: a crash part way through can
/// leave a truncated file under the final name. That costs little, since a
/// truncated theme doesn't parse, so a later scan reports it as unreadable
/// rather than applying it.
fn write_in_place(target: &Path, contents: &[u8]) -> Result<(), LibraryError> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(target)
        .map_err(|error| new_file_error(error, target))?;
    let written = file.write_all(contents).and_then(|()| file.sync_all());
    if let Err(error) = written {
        // This call created the file, so removing a partial one is safe.
        let _ = fs::remove_file(target);
        return Err(error.into());
    }
    Ok(())
}

/// An error creating the final name, with a taken name reported as such so
/// the applet can ask for another.
fn new_file_error(error: io::Error, target: &Path) -> LibraryError {
    if error.kind() == io::ErrorKind::AlreadyExists {
        LibraryError::AlreadyExists(target.to_path_buf())
    } else {
        error.into()
    }
}

/// A hidden name in the same folder, so the final step is on one file
/// system, and with a `.tmp` extension so a scan never takes it for a theme.
/// [`is_temp_name`] must keep recognising exactly this pattern.
fn temp_path(library: &Path, stem: &str) -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let count = COUNTER.fetch_add(1, Ordering::Relaxed);
    library.join(format!(".{stem}.{}-{count}.tmp", std::process::id()))
}

fn write_synced(path: &Path, contents: &[u8]) -> io::Result<()> {
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    file.write_all(contents)?;
    file.sync_all()
}

/// Whether `name` is one [`temp_path`] makes: `.{stem}.{pid}-{count}.tmp`,
/// with a stem [`file_stem_for`] could have made.
fn is_temp_name(name: &str) -> bool {
    let Some(inner) = name
        .strip_prefix('.')
        .and_then(|rest| rest.strip_suffix(".tmp"))
    else {
        return false;
    };
    let Some((stem, pid_count)) = inner.rsplit_once('.') else {
        return false;
    };
    let all_digits = |part: &str| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit());
    let ids_match = pid_count
        .split_once('-')
        .is_some_and(|(pid, count)| all_digits(pid) && all_digits(count));
    ids_match && file_stem_for(stem).as_deref() == Some(stem)
}

/// Removes temporary files a crashed import left in the user library (a
/// successful or failed import removes its own). Only names [`temp_path`]
/// makes, only regular files (a symlink is never followed or removed), and
/// only after [`STALE_TEMP_AGE`], so an import running in another process at
/// this moment keeps its file. Failures are ignored: the leftover is hidden
/// and harmless, and the next scan tries again.
fn remove_stale_temp_files(dir: &Path, now: SystemTime) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let is_temp = entry.file_name().to_str().is_some_and(is_temp_name);
        if is_temp && is_stale_file(&entry.path(), now) {
            let _ = fs::remove_file(entry.path());
        }
    }
}

fn is_stale_file(path: &Path, now: SystemTime) -> bool {
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return false;
    };
    // A modification time in the future gives an error here, so the file
    // is kept.
    metadata.is_file()
        && metadata
            .modified()
            .ok()
            .and_then(|modified| now.duration_since(modified).ok())
            .is_some_and(|age| age > STALE_TEMP_AGE)
}

/// Makes the new name durable. Best effort: the file is complete either
/// way, and some file systems refuse to sync a folder.
fn sync_dir(dir: &Path) {
    if let Ok(dir) = File::open(dir) {
        let _ = dir.sync_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scratch::Scratch;

    const NEBULA_DARK: &str = include_str!("../tests/fixtures/nebula-dark.ron");
    const COMET_LIGHT: &str = include_str!("../tests/fixtures/comet-light.ron");

    /// A scratch folder, so no test touches the real library.
    struct TempDir(Scratch, PathBuf);

    impl TempDir {
        fn new() -> Self {
            let scratch = Scratch::new("library");
            let path = scratch.path().to_path_buf();
            Self(scratch, path)
        }

        fn write(&self, name: &str, contents: &str) -> PathBuf {
            let path = self.1.join(name);
            fs::write(&path, contents).unwrap();
            path
        }

        fn folder(&self, source: Source) -> Folder {
            Folder {
                path: self.0.path().to_path_buf(),
                source,
            }
        }
    }

    fn file_names(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn reads_name_variant_and_key_from_presets() {
        let system = TempDir::new();
        system.write("nebula-dark.ron", NEBULA_DARK);
        system.write("comet-light.ron", COMET_LIGHT);
        let found = scan(&[system.folder(Source::System)]);
        assert_eq!(found.unreadable, []);
        let described: Vec<_> = found
            .themes
            .iter()
            .map(|theme| (theme.key.as_str(), theme.name.as_str(), theme.variant))
            .collect();
        assert_eq!(
            described,
            [
                ("system:comet-light", "Comet Light", Variant::Light),
                ("system:nebula-dark", "Nebula Dark", Variant::Dark),
            ]
        );
    }

    #[test]
    fn the_background_comes_from_the_built_theme() {
        let builder: ThemeBuilder = ron::from_str(NEBULA_DARK).unwrap();
        let background = background_of(&builder);
        assert_eq!(background, builder.clone().build().bg_color());
        // A dark theme's background is darker than a light one's.
        let light: ThemeBuilder = ron::from_str(COMET_LIGHT).unwrap();
        assert!(background.red < background_of(&light).red);
    }

    #[test]
    fn user_theme_named_like_a_preset_is_a_separate_entry() {
        let system = TempDir::new();
        let user = TempDir::new();
        system.write("nebula-dark.ron", NEBULA_DARK);
        user.write("nebula-dark.ron", NEBULA_DARK);
        let found = scan(&[user.folder(Source::User), system.folder(Source::System)]);
        let keys: Vec<_> = found
            .themes
            .iter()
            .map(|theme| theme.key.as_str())
            .collect();
        assert_eq!(keys, ["system:nebula-dark", "user:nebula-dark"]);
    }

    #[test]
    fn same_key_in_two_folders_keeps_the_first() {
        let first = TempDir::new();
        let second = TempDir::new();
        let kept = first.write("nebula-dark.ron", NEBULA_DARK);
        let dropped = second.write("nebula-dark.ron", NEBULA_DARK);
        let found = scan(&[first.folder(Source::User), second.folder(Source::User)]);
        assert_eq!(found.themes.len(), 1);
        assert_eq!(found.themes[0].path, kept);
        assert_eq!(
            found.unreadable,
            [Unreadable {
                path: dropped,
                reason: ReadError::Duplicate { kept }
            }]
        );
    }

    #[test]
    fn bad_files_are_reported_and_the_rest_still_found() {
        let dir = TempDir::new();
        dir.write("nebula-dark.ron", NEBULA_DARK);
        let broken = dir.write("broken.ron", &NEBULA_DARK[..NEBULA_DARK.len() / 2]);
        let not_a_theme = dir.write("not-a-theme.ron", "(answer: 42)");
        let found = scan(&[dir.folder(Source::User)]);
        assert_eq!(found.themes.len(), 1);
        let reported: Vec<_> = found.unreadable.iter().map(|u| &u.path).collect();
        assert_eq!(reported, [&broken, &not_a_theme]);
        assert!(
            found
                .unreadable
                .iter()
                .all(|u| matches!(u.reason, ReadError::Parse { .. }))
        );
    }

    #[test]
    fn oversized_file_is_refused_unread() {
        let dir = TempDir::new();
        let path = dir.1.join("huge.ron");
        // A sparse file: the bound must hold without reading it.
        File::create(&path)
            .unwrap()
            .set_len(MAX_THEME_FILE_BYTES + 1)
            .unwrap();
        assert_eq!(
            read_theme(&path),
            Err(ReadError::TooLarge {
                bytes: MAX_THEME_FILE_BYTES + 1
            })
        );
    }

    #[test]
    fn ignores_other_files_and_missing_folders() {
        let dir = TempDir::new();
        dir.write("nebula-dark.png", "not a theme");
        dir.write("notes.txt", "not a theme");
        fs::create_dir(dir.1.join("folder.ron")).unwrap();
        let missing = Folder {
            path: dir.1.join("missing"),
            source: Source::User,
        };
        let found = scan(&[missing, dir.folder(Source::System)]);
        assert_eq!(found.themes, []);
        // A folder named like a theme is reported, the other files aren't.
        assert_eq!(found.unreadable.len(), 1);
        assert_eq!(found.unreadable[0].reason, ReadError::NotAFile);
    }

    #[test]
    fn readable_names() {
        assert_eq!(display_name("nebula-dark"), "Nebula Dark");
        assert_eq!(display_name("my_theme  two"), "My Theme Two");
        assert_eq!(display_name("été-sombre"), "Été Sombre");
        assert_eq!(display_name("---"), "");
    }

    #[test]
    fn library_folder_follows_xdg() {
        let dir = |xdg: Option<&str>, home: Option<&str>| {
            library_dir_from_env(xdg.map(OsStr::new), home.map(OsStr::new))
        };
        let expected = |base: &str| Some(Path::new(base).join(USER_LIBRARY_SUBDIR));
        assert_eq!(dir(Some("/data"), Some("/home/u")), expected("/data"));
        assert_eq!(
            dir(Some("relative"), Some("/home/u")),
            expected("/home/u/.local/share")
        );
        assert_eq!(dir(None, Some("/home/u")), expected("/home/u/.local/share"));
        assert_eq!(dir(None, Some("")), None);
        assert_eq!(dir(None, None), None);
    }

    #[test]
    fn hostile_names_become_safe_stems() {
        assert_eq!(file_stem_for("My Theme").as_deref(), Some("my-theme"));
        assert_eq!(
            file_stem_for("../../etc/passwd").as_deref(),
            Some("etc-passwd")
        );
        assert_eq!(file_stem_for(".hidden").as_deref(), Some("hidden"));
        assert_eq!(file_stem_for("a/b\\c\0d").as_deref(), Some("a-b-c-d"));
        assert_eq!(file_stem_for("Été  sombre!").as_deref(), Some("été-sombre"));
        for empty in ["", "..", "/", " . / .. "] {
            assert_eq!(file_stem_for(empty), None, "{empty:?}");
        }
        let long = file_stem_for(&"é".repeat(500)).unwrap();
        assert!(long.len() <= MAX_STEM_BYTES);
    }

    #[test]
    fn import_copies_a_valid_theme_byte_for_byte() {
        let outside = TempDir::new();
        let library = TempDir::new();
        let source = outside.write("Nebula Dark.ron", NEBULA_DARK);
        let themes = library.1.join("themes");
        let imported = import_theme(&source, &themes).unwrap();
        assert_eq!(imported, themes.join("nebula-dark.ron"));
        assert_eq!(fs::read_to_string(&imported).unwrap(), NEBULA_DARK);
        assert_eq!(file_names(&themes), ["nebula-dark.ron"]);
    }

    #[test]
    fn import_refuses_broken_and_oversized_files() {
        let outside = TempDir::new();
        let library = TempDir::new();
        let broken = outside.write("broken.ron", "(palette: Dark(");
        assert!(matches!(
            import_theme(&broken, &library.1),
            Err(LibraryError::Theme(ReadError::Parse { .. }))
        ));
        let huge = outside.1.join("huge.ron");
        File::create(&huge)
            .unwrap()
            .set_len(MAX_THEME_FILE_BYTES + 1)
            .unwrap();
        assert!(matches!(
            import_theme(&huge, &library.1),
            Err(LibraryError::Theme(ReadError::TooLarge { .. }))
        ));
        assert_eq!(file_names(&library.1), Vec::<String>::new());
    }

    #[test]
    fn import_never_overwrites() {
        let outside = TempDir::new();
        let library = TempDir::new();
        let existing = library.write("nebula-dark.ron", COMET_LIGHT);
        let source = outside.write("nebula-dark.ron", NEBULA_DARK);
        assert_eq!(
            import_theme(&source, &library.1),
            Err(LibraryError::AlreadyExists(existing.clone()))
        );
        assert_eq!(fs::read_to_string(&existing).unwrap(), COMET_LIGHT);
        assert_eq!(file_names(&library.1), ["nebula-dark.ron"]);
    }

    #[test]
    fn import_stays_inside_the_library_whatever_the_file_name() {
        let outside = TempDir::new();
        let root = TempDir::new();
        let library = root.1.join("themes");
        let source = outside.write("..Escape  Me..ron", NEBULA_DARK);
        let imported = import_theme(&source, &library).unwrap();
        assert_eq!(imported, library.join("escape-me.ron"));
        assert_eq!(file_names(&root.1), ["themes"]);
        // A name that makes the same file name is refused, not renamed.
        let same = outside.write("ESCAPE me.ron", NEBULA_DARK);
        assert_eq!(
            import_theme(&same, &library),
            Err(LibraryError::AlreadyExists(imported))
        );
        // A name with nothing usable in it.
        let unnamed = outside.write("....ron", NEBULA_DARK);
        assert_eq!(
            import_theme(&unnamed, &library),
            Err(LibraryError::InvalidName)
        );
        // Nothing left behind: no second file, no temporary file.
        assert_eq!(file_names(&library), ["escape-me.ron"]);
    }

    #[test]
    fn non_finite_numbers_are_refused() {
        // `replacen(.., 1)`: one bad colour component is enough to refuse.
        for bad in ["NaN", "inf", "-inf", "1e400"] {
            let text = NEBULA_DARK.replacen("red: 1.0,", &format!("red: {bad},"), 1);
            assert_ne!(text, NEBULA_DARK, "the fixture has a component to replace");
            assert_eq!(parse_theme(&text), Err(ReadError::NotFinite), "{bad}");
        }
    }

    #[test]
    fn numbers_too_large_for_cosmic_are_refused() {
        // Finite as an f64, infinite as the f32 COSMIC stores.
        for bad in ["1e39", "-1e39", "340282366920938463463374607431768211455"] {
            let text = NEBULA_DARK.replacen("red: 1.0,", &format!("red: {bad},"), 1);
            assert_eq!(parse_theme(&text), Err(ReadError::NotFinite), "{bad}");
        }
    }

    #[test]
    fn comments_and_literals_cannot_hide_a_non_finite_number() {
        for hidden in [
            "red: /* \" */ NaN /* \" */,",
            "red: // \"\n NaN, // \"\n",
            "red: /* /* \" */ */ inf,",
        ] {
            let text = NEBULA_DARK.replacen("red: 1.0,", hidden, 1);
            assert_ne!(text, NEBULA_DARK);
            assert_eq!(parse_theme(&text), Err(ReadError::NotFinite), "{hidden}");
        }
        // A quote in a character literal or a raw string elsewhere in the
        // file doesn't hide the number either.
        for literal in ["'\"'", "r#\"a \" b\"#"] {
            let text = format!("// {literal}\n{NEBULA_DARK}")
                .replacen("name: \"cosmic-dark\"", &format!("name: {literal}"), 1)
                .replacen("red: 1.0,", "red: NaN,", 1);
            assert!(parse_theme(&text).is_err(), "{literal}");
            let value = format!("({literal}, NaN)");
            assert_eq!(check_finite(&value), Err(ReadError::NotFinite), "{literal}");
        }
    }

    #[test]
    fn a_valid_theme_with_comments_still_parses() {
        let text = NEBULA_DARK
            .replacen("red: 1.0,", "red: /* full */ 1.0, // \"quoted\"\n", 1)
            .replacen("name: \"cosmic-dark\"", "name: r#\"cosmic \"dark\"\"#", 1);
        assert!(parse_theme(&text).is_ok());
    }

    #[test]
    fn number_words_inside_strings_are_not_numbers() {
        let text = NEBULA_DARK.replacen("name: \"cosmic-dark\"", "name: \"NaN inf\\\" inf\"", 1);
        assert_ne!(
            text, NEBULA_DARK,
            "the fixture has a palette name to replace"
        );
        assert!(parse_theme(&text).is_ok());
    }

    #[test]
    fn a_named_pipe_is_refused_without_waiting() {
        let dir = TempDir::new();
        let pipe = dir.1.join("pipe.ron");
        // mkfifo(1) rather than libc::mkfifo: the workspace denies unsafe code.
        let made = std::process::Command::new("mkfifo").arg(&pipe).status();
        assert!(made.is_ok_and(|status| status.success()), "mkfifo failed");
        // Without O_NONBLOCK this open would wait for a writer forever.
        assert_eq!(read_theme(&pipe), Err(ReadError::NotAFile));
    }

    #[test]
    fn parse_errors_give_a_position_not_the_file_text() {
        // ron's own message would quote the unknown variant name.
        let text = "(\n  palette: Hunter2(),\n)";
        let quoted = ron::from_str::<ThemeBuilder>(text).unwrap_err().to_string();
        assert!(quoted.contains("Hunter2"), "{quoted}");
        let error = parse_theme(text).unwrap_err();
        assert!(matches!(error, ReadError::Parse { .. }), "{error:?}");
        let message = error.to_string();
        assert!(!message.contains("Hunter2"), "{message}");
        let message = message.trim_start_matches("not a COSMIC theme: ");
        assert_eq!(message, "unexpected content at line 2, column 12");
    }

    /// A stand-in for a file system without hard links: link(2) fails as it
    /// does on vfat.
    fn no_hard_links(_: &Path, _: &Path) -> io::Result<()> {
        Err(io::Error::from_raw_os_error(libc::EPERM))
    }

    #[test]
    fn without_hard_links_the_final_name_is_written_directly() {
        let library = TempDir::new();
        let target = library.1.join("mine.ron");
        publish(&library.1, "mine", &target, b"theme", no_hard_links).unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"theme");
        // The temporary file is gone either way.
        assert_eq!(file_names(&library.1), ["mine.ron"]);
    }

    #[test]
    fn without_hard_links_an_existing_theme_is_still_kept() {
        let library = TempDir::new();
        let existing = library.write("mine.ron", COMET_LIGHT);
        assert_eq!(
            publish(&library.1, "mine", &existing, b"theme", no_hard_links),
            Err(LibraryError::AlreadyExists(existing.clone()))
        );
        assert_eq!(fs::read_to_string(&existing).unwrap(), COMET_LIGHT);
        assert_eq!(file_names(&library.1), ["mine.ron"]);
    }

    #[test]
    fn other_link_failures_do_not_fall_back() {
        let library = TempDir::new();
        let target = library.1.join("mine.ron");
        let disk_full = |_: &Path, _: &Path| Err(io::Error::from_raw_os_error(libc::ENOSPC));
        assert!(matches!(
            publish(&library.1, "mine", &target, b"theme", disk_full),
            Err(LibraryError::Io(_))
        ));
        assert_eq!(file_names(&library.1), Vec::<String>::new());
    }

    #[test]
    fn which_link_errors_mean_no_hard_links() {
        let unsupported = |errno| links_unsupported(&io::Error::from_raw_os_error(errno));
        for errno in [libc::EPERM, libc::ENOTSUP, libc::EOPNOTSUPP, libc::ENOSYS] {
            assert!(unsupported(errno), "{errno}");
        }
        assert!(unsupported(libc::EXDEV));
        for errno in [libc::EEXIST, libc::ENOSPC, libc::EACCES, libc::EIO] {
            assert!(!unsupported(errno), "{errno}");
        }
        assert!(!links_unsupported(&io::Error::other("not an OS error")));
    }

    #[test]
    fn temp_names_are_recognised_exactly() {
        let made = temp_path(Path::new("/lib"), "my-theme");
        let made = made.file_name().unwrap().to_str().unwrap();
        assert!(is_temp_name(made), "{made}");
        assert!(is_temp_name(".été-sombre.12-0.tmp"));
        for other in [
            "my-theme.ron",
            ".my-theme.tmp",
            "my-theme.1-2.tmp",
            ".my-theme.1-2.tmp.ron",
            ".My-Theme.1-2.tmp",
            ".my-theme.1-.tmp",
            ".my-theme.x-2.tmp",
            ".my-theme.12.tmp",
            "..1-2.tmp",
            ".a..b.1-2.tmp",
            ".my theme.1-2.tmp",
        ] {
            assert!(!is_temp_name(other), "{other}");
        }
    }

    /// Sets a file's modification time to `age` ago.
    fn age_by(path: &Path, age: Duration) {
        File::options()
            .write(true)
            .open(path)
            .unwrap()
            .set_modified(SystemTime::now() - age)
            .unwrap();
    }

    const TWO_HOURS: Duration = Duration::from_hours(2);

    #[test]
    fn scan_removes_stale_temp_files_from_the_user_library() {
        let library = TempDir::new();
        library.write("nebula-dark.ron", NEBULA_DARK);
        let stale = library.write(".nebula-dark.4242-0.tmp", "partial");
        age_by(&stale, TWO_HOURS);
        let fresh = library.write(".nebula-dark.4242-1.tmp", "being written");
        age_by(&fresh, Duration::from_mins(5));
        let found = scan(&[library.folder(Source::User)]);
        assert_eq!(found.themes.len(), 1);
        assert_eq!(found.unreadable, []);
        assert_eq!(
            file_names(&library.1),
            [".nebula-dark.4242-1.tmp", "nebula-dark.ron"]
        );
    }

    #[test]
    fn scan_leaves_everything_else_alone() {
        let system = TempDir::new();
        let user = TempDir::new();
        let outside = TempDir::new();
        let in_system = system.write(".nebula-dark.1-0.tmp", "x");
        age_by(&in_system, TWO_HOURS);
        for name in [".notes.tmp", "keep.1-0.tmp", ".Keep.1-0.tmp", "x.ron.tmp"] {
            age_by(&user.write(name, "x"), TWO_HOURS);
        }
        // A symlink and a folder named like temporary files: neither is a
        // regular file, so neither is removed, and the link isn't followed.
        let target = outside.write(".victim.1-0.tmp", "x");
        age_by(&target, TWO_HOURS);
        std::os::unix::fs::symlink(&target, user.1.join(".link.1-0.tmp")).unwrap();
        fs::create_dir(user.1.join(".folder.1-0.tmp")).unwrap();
        let before = file_names(&user.1);
        let _ = scan(&[system.folder(Source::System), user.folder(Source::User)]);
        assert_eq!(file_names(&user.1), before);
        assert!(in_system.exists());
        assert!(target.exists());
    }
}
