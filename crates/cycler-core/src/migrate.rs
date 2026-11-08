//! Moves what 0.1.x kept under the old name to the new one.
//!
//! Up to 0.1.1 the project was called `cosmic-theme-cycler`, with the App ID
//! `io.github.deepthinker2001.CosmicThemeCycler`. From 0.2.0 it is
//! `cosmic-ext-theme-cycler` and `io.github.deepthinker2001.CosmicExtThemeCycler`,
//! so the settings, imported themes and the `--restore` snapshot live in
//! new folders. [`run`] renames each old folder to its new name, once,
//! before anything reads them. Without it an upgrade would lose the record
//! of the user's original theme.
//!
//! A folder is moved only when it is a real folder (not a symlink) and the
//! new one doesn't exist yet, so nothing is ever overwritten or merged.
//! A move that fails is logged and left; the applet then starts afresh.

use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

use crate::settings::APP_ID;

/// The App ID up to 0.1.1, which named the settings folder.
pub const OLD_APP_ID: &str = "io.github.deepthinker2001.CosmicThemeCycler";
/// The name up to 0.1.1, which named the library and state folders.
pub const OLD_NAME: &str = "cosmic-theme-cycler";
/// The name from 0.2.0.
pub const NAME: &str = "cosmic-ext-theme-cycler";

/// One folder to move: from the old name to the new.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Move {
    pub from: PathBuf,
    pub to: PathBuf,
}

/// The folders to move for these environment values, as the applet finds
/// them: `$XDG_CONFIG_HOME/cosmic`, `$XDG_DATA_HOME` and `$XDG_STATE_HOME`,
/// each falling back under `$HOME`. Relative values are ignored, as the
/// XDG spec asks.
#[must_use]
pub fn moves_from_env(
    config_home: Option<&OsStr>,
    data_home: Option<&OsStr>,
    state_home: Option<&OsStr>,
    home: Option<&OsStr>,
) -> Vec<Move> {
    let absolute =
        |value: Option<&OsStr>| value.map(PathBuf::from).filter(|path| path.is_absolute());
    let home = absolute(home);
    let base = |xdg: Option<&OsStr>, fallback: &str| {
        absolute(xdg).or_else(|| home.as_ref().map(|home| home.join(fallback)))
    };
    let mut moves = Vec::new();
    if let Some(config) = base(config_home, ".config") {
        let cosmic = config.join("cosmic");
        moves.push(Move {
            from: cosmic.join(OLD_APP_ID),
            to: cosmic.join(APP_ID),
        });
    }
    for root in [
        base(data_home, ".local/share"),
        base(state_home, ".local/state"),
    ]
    .into_iter()
    .flatten()
    {
        moves.push(Move {
            from: root.join(OLD_NAME),
            to: root.join(NAME),
        });
    }
    moves
}

/// Moves each old folder to its new name, for this user's environment.
/// Never fails: a folder that can't be moved is logged and left, and
/// returned with why, for `--restore` to tell the person at the terminal.
#[must_use]
pub fn run() -> Vec<(Move, std::io::Error)> {
    let var = std::env::var_os;
    let moves = moves_from_env(
        var("XDG_CONFIG_HOME").as_deref(),
        var("XDG_DATA_HOME").as_deref(),
        var("XDG_STATE_HOME").as_deref(),
        var("HOME").as_deref(),
    );
    let mut failed = Vec::new();
    for one in moves {
        match apply(&one) {
            Ok(true) => {
                tracing::info!(from = %one.from.display(), to = %one.to.display(), "moved to the new name");
            }
            Ok(false) => {}
            Err(error) => {
                tracing::warn!(
                    %error,
                    from = %one.from.display(),
                    "couldn't move to the new name; left where it is"
                );
                failed.push((one, error));
            }
        }
    }
    failed
}

/// Moves `one.from` to `one.to` if `from` is a real folder and `to` doesn't
/// exist. `Ok(false)`: nothing to move.
///
/// # Errors
/// The rename's error, e.g. when the two are on different filesystems.
pub fn apply(one: &Move) -> std::io::Result<bool> {
    if !is_real_dir(&one.from) || fs::symlink_metadata(&one.to).is_ok() {
        return Ok(false);
    }
    fs::rename(&one.from, &one.to)?;
    Ok(true)
}

fn is_real_dir(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|meta| meta.file_type().is_dir())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scratch::Scratch;

    #[test]
    fn folders_follow_xdg_then_home() {
        let moves = moves_from_env(None, None, None, Some(OsStr::new("/home/u")));
        assert_eq!(
            moves,
            [
                Move {
                    from: "/home/u/.config/cosmic/io.github.deepthinker2001.CosmicThemeCycler"
                        .into(),
                    to: "/home/u/.config/cosmic/io.github.deepthinker2001.CosmicExtThemeCycler"
                        .into(),
                },
                Move {
                    from: "/home/u/.local/share/cosmic-theme-cycler".into(),
                    to: "/home/u/.local/share/cosmic-ext-theme-cycler".into(),
                },
                Move {
                    from: "/home/u/.local/state/cosmic-theme-cycler".into(),
                    to: "/home/u/.local/state/cosmic-ext-theme-cycler".into(),
                },
            ]
        );
        let moves = moves_from_env(
            Some(OsStr::new("/c")),
            Some(OsStr::new("relative")),
            Some(OsStr::new("/s")),
            None,
        );
        let froms: Vec<_> = moves.iter().map(|one| one.from.clone()).collect();
        assert_eq!(
            froms,
            [
                PathBuf::from("/c/cosmic/io.github.deepthinker2001.CosmicThemeCycler"),
                PathBuf::from("/s/cosmic-theme-cycler"),
            ],
            "a relative XDG value with no HOME gives no folder"
        );
    }

    #[test]
    fn moves_once_and_never_overwrites() {
        let root = Scratch::new("migrate");
        let one = Move {
            from: root.path().join("old"),
            to: root.path().join("new"),
        };
        assert!(!apply(&one).unwrap(), "nothing to move");
        fs::create_dir(&one.from).unwrap();
        fs::write(one.from.join("selected"), "kept").unwrap();
        assert!(apply(&one).unwrap());
        assert_eq!(fs::read_to_string(one.to.join("selected")).unwrap(), "kept");
        assert!(!one.from.exists());

        fs::create_dir(&one.from).unwrap();
        fs::write(one.from.join("selected"), "older").unwrap();
        assert!(!apply(&one).unwrap(), "the new folder already exists");
        assert_eq!(fs::read_to_string(one.to.join("selected")).unwrap(), "kept");
    }

    #[test]
    fn a_symlink_is_not_moved() {
        let root = Scratch::new("migrate-link");
        let target = root.path().join("elsewhere");
        fs::create_dir(&target).unwrap();
        let one = Move {
            from: root.path().join("old"),
            to: root.path().join("new"),
        };
        std::os::unix::fs::symlink(&target, &one.from).unwrap();
        assert!(!apply(&one).unwrap());
        assert!(!one.to.exists());
    }
}
