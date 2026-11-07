//! Saving the applet's settings without freezing the popup.
//!
//! Changes go to one background thread, which writes only the keys that
//! differ from what it last wrote, through `fastwrite`. If several changes
//! are queued while it is writing, only the newest is written.
//! `last_applied` is never written here: the theme write records it under
//! the lock (`apply.rs`).

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;

use cycler_core::settings::Settings;

use crate::fastwrite::{self, Key};

/// The key the saver leaves alone.
const LAST_APPLIED: &str = "last_applied";

pub(crate) struct Saver {
    sender: mpsc::Sender<Settings>,
    /// Changes sent and not yet written.
    pending: Arc<AtomicUsize>,
}

impl Saver {
    /// Starts the saver for the settings folder `folder`, whose files hold
    /// `on_disk`.
    pub(crate) fn start(folder: PathBuf, on_disk: &Settings) -> Self {
        let (sender, receiver) = mpsc::channel::<Settings>();
        let pending = Arc::new(AtomicUsize::new(0));
        let written = fastwrite::keys(on_disk).unwrap_or_default();
        let counter = Arc::clone(&pending);
        std::thread::spawn(move || run(&folder, &receiver, written, &counter));
        Self { sender, pending }
    }

    /// Queues `settings` to be written.
    pub(crate) fn save(&self, settings: Settings) {
        self.pending.fetch_add(1, Ordering::SeqCst);
        if self.sender.send(settings).is_err() {
            // The thread is gone (it never stops on its own); nothing to do.
            self.pending.fetch_sub(1, Ordering::SeqCst);
        }
    }

    /// Whether changes are still being written: the settings watch then
    /// reports the applet's own half-written state, which is ignored.
    pub(crate) fn busy(&self) -> bool {
        self.pending.load(Ordering::SeqCst) > 0
    }
}

fn run(
    folder: &std::path::Path,
    receiver: &mpsc::Receiver<Settings>,
    mut written: Vec<Key>,
    pending: &AtomicUsize,
) {
    while let Ok(mut latest) = receiver.recv() {
        let mut taken = 1;
        // Only the newest of the changes waiting matters.
        while let Ok(newer) = receiver.try_recv() {
            latest = newer;
            taken += 1;
        }
        match changed_keys(&written, &latest) {
            Ok((keys, all)) => {
                let changed: Vec<&Key> = keys.iter().collect();
                match fastwrite::write_keys(folder, &changed, fastwrite::Durability::Synced) {
                    Ok(()) => written = all,
                    Err(error) => tracing::warn!(%error, "settings not saved"),
                }
            }
            Err(error) => tracing::warn!(%error, "settings not saved"),
        }
        pending.fetch_sub(taken, Ordering::SeqCst);
    }
}

/// The keys of `latest` that differ from `written`, leaving out
/// `last_applied`, and all of `latest`'s keys for next time.
fn changed_keys(
    written: &[Key],
    latest: &Settings,
) -> Result<(Vec<Key>, Vec<Key>), fastwrite::WriteError> {
    let all = fastwrite::keys(&latest.clamped())?;
    let changed = fastwrite::changed(written, &all)
        .into_iter()
        .filter(|(name, _)| *name != LAST_APPLIED)
        .cloned()
        .collect();
    Ok((changed, all))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(keys: &[Key]) -> Vec<&'static str> {
        keys.iter().map(|(name, _)| *name).collect()
    }

    #[test]
    fn only_the_changed_setting_is_written() {
        let before = Settings::default();
        let written = fastwrite::keys(&before).unwrap();
        let after = Settings {
            selected: vec!["system:nebula-dark".into()],
            ..before.clone()
        };
        let (changed, _) = changed_keys(&written, &after).unwrap();
        assert_eq!(names(&changed), ["selected"]);
    }

    #[test]
    fn the_applied_theme_is_left_to_the_theme_write() {
        let written = fastwrite::keys(&Settings::default()).unwrap();
        let after = Settings {
            last_applied: Some("system:nebula-dark".into()),
            ..Settings::default()
        };
        let (changed, _) = changed_keys(&written, &after).unwrap();
        assert_eq!(names(&changed), Vec::<&str>::new());
    }

    #[test]
    fn the_saver_writes_the_newest_settings_and_goes_idle() {
        let scratch = crate::scratch::Scratch::new("saver");
        let folder = scratch.path().join("v1");
        let saver = Saver::start(folder.clone(), &Settings::default());
        for key in ["a", "b", "c"] {
            saver.save(Settings {
                selected: vec![key.into()],
                ..Settings::default()
            });
        }
        while saver.busy() {
            std::thread::yield_now();
        }
        let on_disk = std::fs::read_to_string(folder.join("selected")).unwrap();
        assert_eq!(on_disk, "[\n    \"c\",\n]");
    }
}
