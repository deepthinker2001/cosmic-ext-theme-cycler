//! Putting the original theme back when the applet is taken out of the
//! panel.
//!
//! cosmic-panel stops a removed applet at once: launch-pad sends SIGTERM and
//! waits for it to exit. Watching the panel's config from the UI loop isn't
//! enough, since SIGTERM's default action ends the process before that loop
//! sees the change. So SIGTERM is caught on a thread of its own: if the applet had been in a panel and is
//! now in none, the original theme is put back (as `--restore` does)
//! before exiting. Logging out stops the applet too, but leaves it in the
//! panel's lists, so nothing is put back then.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use cycler_core::panel;

use crate::{APP_ID, restore};

/// Starts the thread that waits for SIGTERM. `was_in_panel` says whether
/// the applet has been seen in a panel (run on its own, it never reverts).
pub(crate) fn watch(was_in_panel: Arc<AtomicBool>) {
    std::thread::spawn(move || {
        if wait_for_terminate().is_some() {
            // Held until the process exits: a theme write already running
            // finishes first, and none can start after the revert (or be
            // cut short by the exit).
            let _lock = restore::lock_for_shutdown();
            if should_revert(was_in_panel.load(Ordering::SeqCst), || {
                panel::in_any_panel(APP_ID)
            }) && let Err(error) = restore::restore_now_locked()
            {
                tracing::warn!(%error, "original theme not put back");
            }
            std::process::exit(0);
        }
    });
}

/// Waits for SIGTERM; `None` if it can't be watched (the default action,
/// ending the process, then still applies).
fn wait_for_terminate() -> Option<()> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .ok()?;
    runtime.block_on(async {
        let mut terminate =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).ok()?;
        terminate.recv().await
    })
}

/// Whether to put the original back on SIGTERM: the applet was in a panel
/// and now (read again from the panels' configs) is in none.
fn should_revert(was_in_panel: bool, in_any_panel: impl FnOnce() -> bool) -> bool {
    was_in_panel && !in_any_panel()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reverts_only_when_taken_out_of_the_panel() {
        assert!(should_revert(true, || false), "removed");
        assert!(!should_revert(true, || true), "logging out: still listed");
        assert!(!should_revert(false, || false), "run on its own");
    }

    #[test]
    fn the_panels_are_not_read_when_never_in_one() {
        assert!(!should_revert(false, || panic!("read the panels")));
    }
}
