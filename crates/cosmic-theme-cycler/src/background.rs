//! Blocking work (reading theme files, writing COSMIC's theme config) off
//! the UI thread.
//!
//! The applet runs on libcosmic's single-threaded executor, where a blocking
//! call would stall every other task and subscription, freezing the panel
//! button and the popup. Each job gets its own
//! short-lived thread, and its result comes back as a message.

use std::future::Future;

use cosmic::iced::futures::channel::oneshot;

/// Runs `work` on a new thread; the future resolves to its result, or to
/// `None` if the thread panicked. That is only so in debug builds and
/// tests: the release profile aborts on panic (as cosmic-applets does), so
/// there a panic in `work` ends the applet. Validating theme files in
/// cycler-core is what keeps untrusted input from panicking here.
pub(crate) fn run<T, F>(work: F) -> impl Future<Output = Option<T>> + Send + 'static
where
    T: Send + 'static,
    F: FnOnce() -> T + Send + 'static,
{
    let (sender, receiver) = oneshot::channel();
    std::thread::spawn(move || {
        // The receiver is gone only if the applet is closing; nothing to do.
        let _ = sender.send(work());
    });
    async move { receiver.await.ok() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runs_the_work_and_returns_its_result() {
        let result = cosmic::iced::futures::executor::block_on(run(|| 6 * 7));
        assert_eq!(result, Some(42));
    }

    #[test]
    fn a_panic_becomes_none() {
        let result = cosmic::iced::futures::executor::block_on(run(|| -> u8 {
            panic!("the work failed");
        }));
        assert_eq!(result, None);
    }
}
