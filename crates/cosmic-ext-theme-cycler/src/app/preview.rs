//! Previewing a theme while the pointer rests on it, and putting back what
//! was there.

use cosmic::app::Task;
use cosmic::cosmic_theme::ThemeBuilder;

use super::{DWELL, Message, Notice, Pending, Preview, ThemeCycler, after};
use crate::{apply, background, fl};

impl ThemeCycler {
    /// The pointer entered a theme's row (`Some`) or left one (`None`):
    /// after a short dwell, show it or put back what was there.
    pub(super) fn pointer_moved(&mut self, key: Option<String>) -> Task<Message> {
        self.preview.generation += 1;
        let generation = self.preview.generation;
        after(DWELL, Message::PreviewDue(generation, key))
    }

    pub(super) fn preview_due(&mut self, generation: u64, key: Option<String>) -> Task<Message> {
        if generation != self.preview.generation {
            return Task::none();
        }
        match (key, self.preview.original.is_some()) {
            // Already previewing: show this one instead.
            (Some(key), true) => match self.theme(&key).map(|theme| theme.builder.clone()) {
                Some(builder) => self.show(builder),
                None => Task::none(),
            },
            // A write is running: wait for it, so what is read next isn't
            // half written.
            (Some(key), false) if self.writing => {
                after(DWELL, Message::PreviewDue(generation, Some(key)))
            }
            // A new preview: first read what COSMIC shows, to put it back.
            (Some(key), false) => cosmic::task::future(async move {
                let shown = background::run(|| apply::shown().map(Box::new))
                    .await
                    .unwrap_or_else(|| Err(fl!("apply-crashed")));
                Message::PreviewStart(generation, key, shown)
            }),
            (None, true) => self.end_preview(),
            (None, false) => Task::none(),
        }
    }

    pub(super) fn preview_start(
        &mut self,
        generation: u64,
        key: &str,
        shown: Result<Box<apply::Shown>, String>,
    ) -> Task<Message> {
        if generation != self.preview.generation {
            return Task::none();
        }
        let (Ok(shown), Some(builder)) =
            (shown, self.theme(key).map(|theme| theme.builder.clone()))
        else {
            return Task::none();
        };
        self.preview.original = Some(shown);
        self.preview.written = false;
        self.show(builder)
    }

    /// Shows a hovered theme, after any write still running.
    pub(super) fn show(&mut self, builder: ThemeBuilder) -> Task<Message> {
        if self.writing {
            self.preview.waiting = Some(Pending::Show(Box::new(builder)));
            return Task::none();
        }
        self.writing = true;
        let continuing = std::mem::replace(&mut self.preview.written, true);
        cosmic::task::future(async move {
            let result = background::run(move || apply::preview(&builder, continuing))
                .await
                .unwrap_or_else(|| Err(fl!("apply-crashed")));
            Message::Previewed(result)
        })
    }

    /// Puts back what was shown before the preview, after any write still
    /// running; the preview is over only once that has worked.
    pub(super) fn end_preview(&mut self) -> Task<Message> {
        let Some(original) = self.preview.original.clone() else {
            return Task::none();
        };
        // No more dwells from before.
        self.preview.generation += 1;
        if self.writing {
            self.preview.waiting = Some(Pending::PutBack);
            return Task::none();
        }
        self.writing = true;
        self.preview.putting_back = true;
        cosmic::task::future(async move {
            let result = background::run(move || apply::put_back(&original))
                .await
                .unwrap_or_else(|| Err(fl!("apply-crashed")));
            Message::Previewed(result)
        })
    }

    pub(super) fn previewed(&mut self, result: Result<bool, String>) -> Task<Message> {
        self.writing = false;
        let putting_back = std::mem::take(&mut self.preview.putting_back);
        match result {
            // Put back, or --restore ran meanwhile: the preview is over.
            Ok(true) if putting_back => self.clear_preview(),
            Ok(true) => {}
            Ok(false) => self.clear_preview(),
            // The original is kept, so leaving or closing tries again.
            Err(error) => self.notice = Some(Notice::ApplyFailed(error)),
        }
        self.write_waiting()
    }

    pub(super) fn clear_preview(&mut self) {
        self.preview = Preview {
            generation: self.preview.generation + 1,
            ..Preview::default()
        };
    }
}
