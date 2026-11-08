//! The applet: its state, its messages and how it handles them.
//!
//! A left click on the panel icon applies the next theme toggled on, in
//! list order; its tooltip names the theme shown. A right click opens the popup
//! the standard COSMIC way: it is opened through `surface_task` and
//! libcosmic renders it by calling `view_window` with the popup's ID; its
//! widgets are in `view.rs`. In the popup, resting the pointer on a theme
//! shows it on the desktop, and leaving puts back what was there.
//!
//! The list includes the theme COSMIC showed before the applet first
//! changed it, and when the applet is taken out of the panel it puts that
//! theme back.
//!
//! Nothing here blocks: reading theme files and writing COSMIC's theme
//! config run through [`background::run`], and their results come back as
//! messages. The theme changes only when the user asks.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use cosmic::app::{Core, Task};
use cosmic::cosmic_config;
use cosmic::cosmic_theme::ThemeBuilder;
use cosmic::dialog::file_chooser::{self, FileFilter};
use cosmic::iced::Subscription;
use cosmic::iced::window::Id;
use cosmic::surface::action::{LiveSettings, app_popup, destroy_popup};
use cosmic::surface::surface_task;
use cosmic::widget::mouse_area;
use cosmic::{Element, applet, iced};
use cycler_core::Variant;
use cycler_core::cycle;
use cycler_core::library::{self, LibraryError, Scan, Source, Theme};
use cycler_core::panel::{self, PanelApplets};
use cycler_core::settings::{CONFIG_VERSION, Settings};

use crate::saver::Saver;
use crate::{APP_ID, ICON, apply, background, fl, restore, shutdown, tooltip};

/// How long the pointer must rest on a theme before it is shown, or be away
/// before what was there comes back: passing over the list doesn't flash
/// the desktop.
const DWELL: Duration = Duration::from_millis(200);

/// The key stem of the list entry for the theme COSMIC showed before the
/// applet; its key is `original:theme`.
const ORIGINAL_STEM: &str = "theme";

pub struct ThemeCycler {
    pub(crate) core: Core,
    /// The open popup, if any.
    pub(crate) popup: Option<Id>,
    /// Writes the settings in the background; `None` if they can't be
    /// saved (no config folder), or in tests.
    saver: Option<Saver>,
    pub(crate) settings: Settings,
    /// The themes found; `None` until the first scan finishes.
    pub(crate) scan: Option<Scan>,
    /// The last thing worth telling the user, shown in the popup.
    pub(crate) notice: Option<Notice>,
    /// A theme is being written; other writes wait for it.
    writing: bool,
    /// When a left click came that is waiting for a theme write to finish,
    /// or for the themes to be found; it runs as soon as it can. Clicks
    /// that come meanwhile count as one.
    click_waiting: Option<Instant>,
    /// When the click being handled came, for the timing in the debug log.
    click_started: Option<Instant>,
    /// A settings change on disk arrived while the saver was writing and
    /// was set aside; it is read again once the saver is idle.
    settings_stale: bool,
    pub(crate) preview: Preview,
    /// Each panel's config ID, to watch for the applet being removed.
    panel_ids: Vec<String>,
    /// Whether each panel (by config ID) holds the applet, as last read.
    in_panels: HashMap<String, bool>,
    /// The applet has been seen in a panel; until then (run on its own,
    /// say) not being in one means nothing. Shared with the thread that
    /// puts the original back when the panel stops the applet.
    was_in_panel: Arc<AtomicBool>,
    /// The tooltip's text, shared with its popup so an open tooltip
    /// follows a click.
    tooltip_name: tooltip::Name,
    /// The original theme is being, or has been, put back after the applet
    /// was taken out of the panel.
    reverted: bool,
}

/// Showing a theme while the pointer rests on it.
#[derive(Debug, Default)]
pub(crate) struct Preview {
    /// Bumped by every pointer move in or out, so a dwell from an earlier
    /// move is ignored.
    generation: u64,
    /// What COSMIC showed before the preview began, exactly, to put back;
    /// `Some` until it has been put back (or replaced by a click's theme).
    original: Option<Box<apply::Shown>>,
    /// A write that came while another write was running.
    waiting: Option<Pending>,
    /// The preview has written to the desktop at least once.
    written: bool,
    /// The original is being put back.
    putting_back: bool,
}

/// A write held back until the one running finishes.
#[derive(Debug)]
enum Pending {
    /// Showing a hovered theme.
    Show(Box<ThemeBuilder>),
    /// Putting the original back.
    PutBack,
}

/// What the popup tells the user after an action.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Notice {
    NoneSelected,
    /// Why the theme wasn't shown, already as text for the user.
    ApplyFailed(String),
    Imported(String),
    ImportFailed(ImportError),
}

/// Why a theme file wasn't imported; the popup words each case.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ImportError {
    /// The library refused the file or couldn't write it.
    Library(LibraryError),
    /// There is no folder for the user's library (no usable `HOME`).
    NoLibrary,
    /// The file chooser gave a file that isn't on this computer.
    NotALocalFile,
    /// The file chooser didn't open.
    ChooserFailed,
    /// The import stopped unexpectedly.
    Crashed,
}

/// The list's groups, in list order.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum Group {
    Light,
    Dark,
    /// Toggled on but since deleted, so its kind is unknown.
    Deleted,
}

#[derive(Clone, Debug)]
pub enum Message {
    /// A left click on the panel icon.
    Next,
    /// A right click on the panel icon.
    TogglePopup,
    PopupClosed(Id),
    Surface(cosmic::surface::Action<Message>),
    Scanned(Option<Scan>),
    Select(String, bool),
    Applied(String, Result<(), String>),
    Import,
    ImportChosen(Option<PathBuf>),
    Imported(Result<String, ImportError>),
    /// The settings changed on disk: by `--restore`, or by this applet.
    SettingsChanged(Settings),
    /// Settings set aside while the saver was busy may be read now.
    SettingsCheck,
    HoverTheme(String),
    LeaveTheme,
    /// The pointer rested long enough: show this theme (`None`: put back
    /// what was there), if nothing moved since the generation given.
    PreviewDue(u64, Option<String>),
    /// What COSMIC showed before the preview, read in the background.
    PreviewStart(u64, String, Result<Box<apply::Shown>, String>),
    /// A preview write finished; `Ok(false)`: `--restore` ran meanwhile, so
    /// nothing was written and the preview is over.
    Previewed(Result<bool, String>),
    /// Whether a panel (by config ID) holds the applet.
    PanelChanged(String, bool),
    /// The original theme was put back after the applet's removal.
    Reverted(Result<bool, String>),
}

impl cosmic::Application for ThemeCycler {
    type Executor = cosmic::SingleThreadExecutor;
    type Flags = ();
    type Message = Message;
    const APP_ID: &'static str = APP_ID;

    fn core(&self) -> &Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut Core {
        &mut self.core
    }

    fn init(core: Core, _flags: Self::Flags) -> (Self, Task<Message>) {
        let config = Settings::config()
            .inspect_err(|error| tracing::warn!(%error, "settings will not be saved"))
            .ok();
        let settings = config.as_ref().map(Settings::load).unwrap_or_default();
        let saver = config
            .and(restore::Where::System.folder(APP_ID, CONFIG_VERSION))
            .map(|folder| Saver::start(folder, &settings));
        let mut applet = Self::new(core, saver, settings);
        applet.panel_ids = panel::panel_ids().unwrap_or_default();
        shutdown::watch(Arc::clone(&applet.was_in_panel));
        (applet, rescan())
    }

    fn on_close_requested(&self, id: Id) -> Option<Message> {
        Some(Message::PopupClosed(id))
    }

    fn subscription(&self) -> Subscription<Message> {
        let mut subscriptions = vec![
            self.core
                .watch_config::<Settings>(APP_ID)
                .map(|update| Message::SettingsChanged(update.config)),
        ];
        // Read only: whether each panel still holds the applet.
        for id in &self.panel_ids {
            let id = id.clone();
            subscriptions.push(
                cosmic_config::config_subscription::<_, PanelApplets>(
                    ("panel", id.clone()),
                    id.clone().into(),
                    panel::PANEL_VERSION,
                )
                .with(id)
                .map(|(id, update)| Message::PanelChanged(id, update.config.holds(APP_ID))),
            );
        }
        if self.settings_stale {
            subscriptions
                .push(iced::time::every(Duration::from_secs(1)).map(|_| Message::SettingsCheck));
        }
        Subscription::batch(subscriptions)
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Next => return self.next(),
            Message::TogglePopup => return self.toggle_popup(),
            Message::PopupClosed(id) => {
                if self.popup == Some(id) {
                    self.popup = None;
                    return self.end_preview();
                }
            }
            Message::Surface(action) => return surface_task(action),
            Message::Scanned(scan) => return self.scanned(scan),
            Message::Select(key, selected) => self.select(key, selected),
            Message::Applied(key, result) => return self.applied(key, result),
            Message::Import => return choose_import(),
            Message::ImportChosen(path) => return path.map_or_else(Task::none, import),
            Message::Imported(result) => return self.imported(result),
            Message::SettingsChanged(settings) => self.settings_changed(settings.clamped()),
            Message::SettingsCheck => return self.settings_check(),
            Message::HoverTheme(key) => return self.pointer_moved(Some(key)),
            Message::LeaveTheme => return self.pointer_moved(None),
            Message::PreviewDue(generation, key) => return self.preview_due(generation, key),
            Message::PreviewStart(generation, key, shown) => {
                return self.preview_start(generation, &key, shown);
            }
            Message::Previewed(result) => return self.previewed(result),
            Message::PanelChanged(id, holds) => return self.panel_changed(id, holds),
            Message::Reverted(result) => self.reverted(result),
        }
        Task::none()
    }

    fn view(&self) -> Element<'_, Message> {
        let button = self
            .core
            .applet
            .icon_button(ICON)
            .on_press_down(Message::Next);
        let button = mouse_area(button).on_right_press(Message::TogglePopup);
        Element::from(tooltip::applet_tooltip(
            self.core.applet.anchor,
            button,
            &self.tooltip_name,
            self.popup.is_some(),
            Message::Surface,
        ))
    }

    fn view_window(&self, id: Id) -> Element<'_, Message> {
        if self.popup != Some(id) {
            return cosmic::widget::text::body("").into();
        }
        self.core
            .applet
            .popup_container(self.popup_content())
            .into()
    }

    fn style(&self) -> Option<iced::theme::Style> {
        Some(applet::style())
    }
}

impl ThemeCycler {
    /// The applet before anything has been read from disk; `saver` writes
    /// the settings (`None` in tests, so nothing is saved).
    pub(crate) fn new(core: Core, saver: Option<Saver>, settings: Settings) -> Self {
        let applet = Self {
            core,
            popup: None,
            saver,
            settings,
            scan: None,
            notice: None,
            writing: false,
            click_waiting: None,
            click_started: None,
            settings_stale: false,
            preview: Preview::default(),
            panel_ids: Vec::new(),
            in_panels: HashMap::new(),
            was_in_panel: Arc::new(AtomicBool::new(false)),
            tooltip_name: tooltip::Name::default(),
            reverted: false,
        };
        applet.sync_tooltip();
        applet
    }

    /// The panel icon's tooltip: the name of the theme the applet applied,
    /// or the applet's name before it has applied one.
    pub(crate) fn current_name(&self) -> String {
        self.settings
            .last_applied
            .as_deref()
            .and_then(|key| self.theme(key))
            .map_or_else(|| fl!("applet-name"), |theme| theme.name.clone())
    }

    /// Updates the tooltip's shared text, so an open tooltip follows a
    /// click (see tooltip.rs). Called whenever the applied theme or the
    /// themes found change.
    fn sync_tooltip(&self) {
        let current = self.current_name();
        if let Ok(mut name) = self.tooltip_name.write()
            && *name != current
        {
            *name = current;
        }
    }

    fn scanned(&mut self, scan: Option<Scan>) -> Task<Message> {
        // A failed scan keeps the themes found before.
        if scan.is_some() {
            self.scan = scan;
        }
        self.sync_tooltip();
        self.click_if_waiting()
    }

    fn toggle_popup(&mut self) -> Task<Message> {
        if let Some(id) = self.popup.take() {
            return Task::batch([surface_task(destroy_popup(id)), self.end_preview()]);
        }
        let open = surface_task(app_popup::<Self>(
            |_| LiveSettings::default(),
            |state: &mut Self| {
                let id = Id::unique();
                state.popup = Some(id);
                state.core.applet.get_popup_settings(
                    // libcosmic gives every applet's main window
                    // Id::RESERVED; same fallback, without a panic.
                    state.core.main_window_id().unwrap_or(Id::RESERVED),
                    id,
                    None,
                    None,
                    None,
                )
            },
            // No view here: libcosmic renders the popup through view_window.
            None,
        ));
        // Look again each time the popup opens: themes come and go.
        Task::batch([open, rescan()])
    }

    pub(crate) fn theme(&self, key: &str) -> Option<&Theme> {
        self.scan
            .as_ref()?
            .themes
            .iter()
            .find(|theme| theme.key == key)
    }

    /// Every theme key in list order, which is also the cycle order: light
    /// themes, then dark ones, each from the brightest background to the
    /// darkest, then themes toggled on since deleted (whose kind is
    /// unknown).
    pub(crate) fn ordered_keys(&self) -> Vec<String> {
        let themes = self.scan.iter().flat_map(|scan| &scan.themes);
        let mut ranked: Vec<(Group, f32, &str)> = themes
            .map(|theme| {
                let group = match theme.variant {
                    Variant::Light => Group::Light,
                    Variant::Dark => Group::Dark,
                };
                (group, brightness(theme), theme.key.as_str())
            })
            .collect();
        let found: HashSet<&str> = ranked.iter().map(|(_, _, key)| *key).collect();
        for key in &self.settings.selected {
            if !found.contains(key.as_str()) {
                ranked.push((Group::Deleted, 0.0, key));
            }
        }
        ranked.sort_by(|(a_group, a_bright, a_key), (b_group, b_bright, b_key)| {
            a_group
                .cmp(b_group)
                .then(b_bright.total_cmp(a_bright))
                .then_with(|| a_key.cmp(b_key))
        });
        ranked
            .into_iter()
            .map(|(_, _, key)| key.to_owned())
            .collect()
    }

    /// The themes toggled on, in list order, each with whether it still
    /// exists (a deleted one is kept so the cycle goes on from its place).
    fn selected_themes(&self) -> Vec<(String, bool)> {
        self.ordered_keys()
            .into_iter()
            .filter(|key| self.settings.selected.contains(key))
            .map(|key| {
                let exists = self.theme(&key).is_some();
                (key, exists)
            })
            .collect()
    }

    /// A left click: the next theme toggled on, in list order. With none
    /// toggled on, the popup opens instead so themes can be chosen.
    fn next(&mut self) -> Task<Message> {
        let came = self.click_waiting.take().unwrap_or_else(Instant::now);
        // A click ends any preview: no more dwells. The original is kept
        // until the click's theme is written, which puts it back first.
        self.preview.generation += 1;
        // Never dropped: it waits for the write running, or for the themes.
        if self.writing || self.scan.is_none() {
            self.click_waiting = Some(came);
            return Task::none();
        }
        self.click_started = Some(came);
        let themes = self.selected_themes();
        let last = self.settings.last_applied.as_deref();
        if let Some(key) = cycle::next_after(&themes, last) {
            let key = key.to_owned();
            // The click's write puts any preview's original back first.
            self.preview.waiting = None;
            return self.apply_key(key);
        }
        self.click_started = None;
        self.notice = Some(Notice::NoneSelected);
        // Nothing to write, so a put-back that was waiting still happens.
        let put_back = match self.preview.waiting.take() {
            Some(Pending::PutBack) => self.end_preview(),
            _ => Task::none(),
        };
        if self.popup.is_none() {
            Task::batch([put_back, self.toggle_popup()])
        } else {
            put_back
        }
    }

    /// Writes the theme `key` names to the desktop, off the UI thread.
    fn apply_key(&mut self, key: String) -> Task<Message> {
        let Some(builder) = self.theme(&key).map(|theme| theme.builder.clone()) else {
            return Task::none();
        };
        let original = self.preview.original.clone();
        self.writing = true;
        cosmic::task::future(async move {
            let applied_key = key.clone();
            let result =
                background::run(move || apply::apply(&builder, &applied_key, original.as_deref()))
                    .await
                    .unwrap_or_else(|| Err(fl!("apply-crashed")));
            Message::Applied(key, result)
        })
    }

    fn applied(&mut self, key: String, result: Result<(), String>) -> Task<Message> {
        self.writing = false;
        if let Some(started) = self.click_started.take() {
            tracing::debug!(theme = %key, elapsed = ?started.elapsed(), "click handled");
        }
        match result {
            Ok(()) => {
                self.notice = None;
                // `apply` saved it, holding the lock, ordered with --restore.
                self.settings.last_applied = Some(key);
                self.sync_tooltip();
                // Any preview is over: its original was put back first.
                self.clear_preview();
            }
            // A preview's original is kept, so closing the list puts it back.
            Err(error) => self.notice = Some(Notice::ApplyFailed(error)),
        }
        self.write_waiting()
    }

    /// Toggles a theme on or off for the cycle; the list doesn't move.
    fn select(&mut self, key: String, selected: bool) {
        let selection = &mut self.settings.selected;
        if selected {
            if !selection.contains(&key) {
                selection.push(key);
            }
        } else {
            selection.retain(|kept| *kept != key);
        }
        if selected && self.notice == Some(Notice::NoneSelected) {
            self.notice = None;
        }
        if let Some(saver) = &self.saver {
            saver.save(self.settings.clone());
        }
    }

    /// Takes settings written by someone else (`--restore`); the applet's
    /// own saves come back here too, unchanged.
    fn settings_changed(&mut self, settings: Settings) {
        if settings == self.settings {
            return;
        }
        if self.saver.as_ref().is_some_and(Saver::busy) {
            // Read again once the saver is idle, so a --restore isn't missed.
            self.settings_stale = true;
            return;
        }
        self.settings = settings;
        self.sync_tooltip();
    }

    fn settings_check(&mut self) -> Task<Message> {
        if !self.settings_stale || self.saver.as_ref().is_some_and(Saver::busy) {
            return Task::none();
        }
        self.settings_stale = false;
        reload_settings()
    }

    /// After an import: say how it went and look at the library again so
    /// the new theme is listed.
    fn imported(&mut self, result: Result<String, ImportError>) -> Task<Message> {
        self.notice = Some(match result {
            Ok(name) => Notice::Imported(name),
            Err(error) => Notice::ImportFailed(error),
        });
        rescan()
    }

    /// The pointer entered a theme's row (`Some`) or left one (`None`):
    /// after a short dwell, show it or put back what was there.
    fn pointer_moved(&mut self, key: Option<String>) -> Task<Message> {
        self.preview.generation += 1;
        let generation = self.preview.generation;
        after(DWELL, Message::PreviewDue(generation, key))
    }

    fn preview_due(&mut self, generation: u64, key: Option<String>) -> Task<Message> {
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

    fn preview_start(
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
    fn show(&mut self, builder: ThemeBuilder) -> Task<Message> {
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
    fn end_preview(&mut self) -> Task<Message> {
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

    fn previewed(&mut self, result: Result<bool, String>) -> Task<Message> {
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

    fn clear_preview(&mut self) {
        self.preview = Preview {
            generation: self.preview.generation + 1,
            ..Preview::default()
        };
    }

    /// A click or write that came while another write ran.
    fn write_waiting(&mut self) -> Task<Message> {
        if self.click_waiting.is_some() {
            return self.click_if_waiting();
        }
        match self.preview.waiting.take() {
            Some(Pending::Show(builder)) if self.preview.original.is_some() => self.show(*builder),
            Some(Pending::PutBack) => self.end_preview(),
            _ => Task::none(),
        }
    }

    /// Runs a click that waited, unless a write is still running or the
    /// themes aren't found yet.
    fn click_if_waiting(&mut self) -> Task<Message> {
        if self.click_waiting.is_none() || self.writing || self.scan.is_none() {
            return Task::none();
        }
        self.next()
    }

    /// The original theme was put back after the applet's removal, or not.
    fn reverted(&mut self, result: Result<bool, String>) {
        match result {
            Ok(true) => {}
            // Still in a panel after all: watch on.
            Ok(false) => self.reverted = false,
            Err(error) => tracing::warn!(%error, "original theme not put back"),
        }
    }

    /// A panel's applet list changed. Once the applet is in no panel (it was
    /// taken out in COSMIC Settings), the original theme is put back. Every
    /// panel must have reported, and the applet must have been seen in one,
    /// so nothing is put back while starting up.
    fn panel_changed(&mut self, id: String, holds: bool) -> Task<Message> {
        self.in_panels.insert(id, holds);
        if holds {
            self.was_in_panel.store(true, Ordering::SeqCst);
        }
        let all_reported = self
            .panel_ids
            .iter()
            .all(|id| self.in_panels.contains_key(id));
        let in_none = self.in_panels.values().all(|holds| !holds);
        let was_in_panel = self.was_in_panel.load(Ordering::SeqCst);
        if self.reverted || !was_in_panel || !all_reported || !in_none {
            return Task::none();
        }
        self.reverted = true;
        cosmic::task::future(async {
            Message::Reverted(
                background::run(|| {
                    // Read every panel again first: a list caught mid-edit,
                    // or one that couldn't be read, must not count.
                    if panel::in_any_panel(APP_ID) {
                        Ok(false)
                    } else {
                        restore::restore_now()
                    }
                })
                .await
                .unwrap_or_else(|| Err(fl!("apply-crashed"))),
            )
        })
    }
}

/// How bright a theme's background is, by its relative luminance.
fn brightness(theme: &Theme) -> f32 {
    let background = theme.background;
    0.2126 * background.red + 0.7152 * background.green + 0.0722 * background.blue
}

/// `message` after `delay`, on libcosmic's runtime.
fn after(delay: Duration, message: Message) -> Task<Message> {
    cosmic::task::future(async move {
        tokio::time::sleep(delay).await;
        message
    })
}

/// Reads the theme folders again, with the user's original theme, off the
/// UI thread.
fn rescan() -> Task<Message> {
    cosmic::task::future(async {
        Message::Scanned(
            background::run(|| {
                let mut scan = library::scan_with_builtins(&library::default_folders());
                if let Some(original) = original_theme() {
                    scan.themes.push(original);
                }
                scan
            })
            .await,
        )
    })
}

/// The theme COSMIC showed before the applet first changed it: from the
/// snapshot once there is one, else what COSMIC shows now (which is that
/// theme). Blocking.
fn original_theme() -> Option<Theme> {
    // A snapshot that can't be read gives no entry rather than the theme
    // shown now, which may be one of the applet's under the wrong name.
    let snapshot_exists = restore::snapshot_path().is_some_and(|path| path.exists());
    let builder = if snapshot_exists {
        restore::original_builder()?
    } else {
        apply::shown().ok()?.builder().clone()
    };
    Some(Theme {
        key: library::theme_key(Source::Original, ORIGINAL_STEM),
        name: fl!("original-theme"),
        source: Source::Original,
        path: PathBuf::new(),
        variant: library::variant_of(&builder),
        background: library::background_of(&builder),
        builder,
    })
}

/// Asks the desktop's file chooser for a theme file to import.
fn choose_import() -> Task<Message> {
    cosmic::task::future(async {
        let files = fl!("theme-files");
        let dialog = file_chooser::open::Dialog::new()
            .title(fl!("import-title"))
            .filter(FileFilter::new(&files).glob("*.ron"));
        match dialog.open_file().await {
            // Not `response.url()`: it panics on an empty list, which a
            // faulty portal could send, and would take the applet down.
            Ok(response) => match response
                .0
                .uris()
                .first()
                .map(cosmic::dialog::ashpd::url::Url::to_file_path)
            {
                Some(Ok(path)) => Message::ImportChosen(Some(path)),
                Some(Err(())) => Message::Imported(Err(ImportError::NotALocalFile)),
                None => Message::ImportChosen(None),
            },
            Err(file_chooser::Error::Cancelled) => Message::ImportChosen(None),
            Err(error) => {
                tracing::warn!(%error, "file chooser failed");
                Message::Imported(Err(ImportError::ChooserFailed))
            }
        }
    })
}

/// Copies the chosen file into the user's library, off the UI thread.
fn import(path: PathBuf) -> Task<Message> {
    cosmic::task::future(async move {
        let result = background::run(move || {
            let library = library::user_library_dir().ok_or(ImportError::NoLibrary)?;
            let saved = library::import_theme(&path, &library).map_err(ImportError::Library)?;
            Ok(theme_name(&saved))
        })
        .await
        .unwrap_or(Err(ImportError::Crashed));
        Message::Imported(result)
    })
}

/// Reads the settings from disk again, off the UI thread.
fn reload_settings() -> Task<Message> {
    cosmic::task::future(async {
        let settings = background::run(|| {
            Settings::config()
                .ok()
                .map(|config| Settings::load(&config))
        })
        .await
        .flatten();
        settings.map_or(Message::SettingsCheck, Message::SettingsChanged)
    })
}

/// The name the popup lists a theme file under.
fn theme_name(path: &std::path::Path) -> String {
    path.file_stem()
        .map(|stem| library::display_name(&stem.to_string_lossy()))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use cosmic::cosmic_theme::palette::Srgba;

    use super::*;

    fn theme(key: &str, builder: ThemeBuilder) -> Theme {
        Theme {
            key: key.to_owned(),
            name: key.to_owned(),
            source: library::Source::System,
            path: PathBuf::from(format!("/nowhere/{key}.ron")),
            variant: library::variant_of(&builder),
            background: library::background_of(&builder),
            builder,
        }
    }

    fn with_background(builder: ThemeBuilder, grey: f32) -> ThemeBuilder {
        ThemeBuilder {
            bg_color: Some(Srgba::new(grey, grey, grey, 1.0)),
            ..builder
        }
    }

    /// Two dark and two light themes of different brightness, saving
    /// nothing: no saver, so no test touches the real settings.
    fn applet(selected: &[&str]) -> ThemeCycler {
        let settings = Settings {
            selected: selected.iter().map(|&key| key.to_owned()).collect(),
            ..Settings::default()
        };
        let mut applet = ThemeCycler::new(Core::default(), None, settings);
        applet.scan = Some(Scan {
            themes: vec![
                theme("dim-dark", with_background(ThemeBuilder::dark(), 0.05)),
                theme("grey-dark", with_background(ThemeBuilder::dark(), 0.25)),
                theme("pale-light", with_background(ThemeBuilder::light(), 0.95)),
                theme("grey-light", with_background(ThemeBuilder::light(), 0.7)),
            ],
            unreadable: Vec::new(),
        });
        applet
    }

    fn listed(applet: &ThemeCycler) -> Vec<String> {
        applet
            .listed_themes()
            .iter()
            .map(|theme| theme.key.clone())
            .collect()
    }

    fn start_preview(applet: &mut ThemeCycler, key: &str) {
        let _ = applet.pointer_moved(Some(key.into()));
        let generation = applet.preview.generation;
        let original = Box::new(apply::Shown::of(&ThemeBuilder::dark()));
        let _ = applet.preview_start(generation, key, Ok(original));
    }

    #[test]
    fn light_themes_come_first_each_group_brightest_first() {
        assert_eq!(
            listed(&applet(&[])),
            ["pale-light", "grey-light", "grey-dark", "dim-dark"]
        );
    }

    #[test]
    fn toggling_a_theme_leaves_the_list_alone() {
        let mut applet = applet(&[]);
        let before = listed(&applet);
        applet.select("dim-dark".into(), true);
        applet.select("dim-dark".into(), true);
        assert_eq!(applet.settings.selected, ["dim-dark"]);
        assert_eq!(listed(&applet), before);
        applet.select("dim-dark".into(), false);
        assert_eq!(applet.settings.selected, Vec::<String>::new());
    }

    #[test]
    fn the_cycle_follows_the_list_and_keeps_a_deleted_theme_s_place() {
        let applet = applet(&["dim-dark", "gone", "pale-light"]);
        assert_eq!(
            applet.selected_themes(),
            [
                ("pale-light".to_owned(), true),
                ("dim-dark".to_owned(), true),
                ("gone".to_owned(), false),
            ]
        );
    }

    #[test]
    fn the_tooltip_names_the_theme_shown() {
        let mut applet = applet(&["grey-dark"]);
        let tooltip = |applet: &ThemeCycler| applet.tooltip_name.read().unwrap().clone();
        assert_eq!(tooltip(&applet), fl!("applet-name"));
        let _ = applet.next();
        let _ = applet.applied("grey-dark".into(), Ok(()));
        assert_eq!(tooltip(&applet), "grey-dark");
    }

    #[test]
    fn a_click_with_nothing_toggled_asks_for_themes() {
        let mut applet = applet(&[]);
        let _ = applet.next();
        assert_eq!(applet.notice, Some(Notice::NoneSelected));
        assert!(!applet.writing);
    }

    #[test]
    fn a_click_writes_the_next_theme_and_records_it_when_it_worked() {
        let mut applet = applet(&["grey-light", "grey-dark"]);
        // The task is built but never run, so nothing is written.
        let _ = applet.next();
        assert!(applet.writing);
        let _ = applet.applied("grey-light".into(), Err("no config folder".into()));
        assert_eq!(applet.settings.last_applied, None);
        assert_eq!(
            applet.notice,
            Some(Notice::ApplyFailed("no config folder".into()))
        );
        let _ = applet.applied("grey-light".into(), Ok(()));
        assert_eq!(applet.settings.last_applied.as_deref(), Some("grey-light"));
        assert_eq!(applet.notice, None);
    }

    #[test]
    fn a_stale_dwell_does_nothing() {
        let mut applet = applet(&[]);
        let _ = applet.pointer_moved(Some("grey-dark".into()));
        let early = applet.preview.generation;
        let _ = applet.pointer_moved(Some("dim-dark".into()));
        let _ = applet.preview_due(early, Some("grey-dark".into()));
        assert!(applet.preview.original.is_none());
    }

    #[test]
    fn hovering_shows_a_theme_and_leaving_puts_the_original_back() {
        let mut applet = applet(&[]);
        start_preview(&mut applet, "grey-dark");
        assert!(applet.preview.original.is_some());
        assert!(applet.writing, "the hovered theme is written");
        let _ = applet.previewed(Ok(true));
        // Another theme: shown in turn.
        let _ = applet.pointer_moved(Some("dim-dark".into()));
        let generation = applet.preview.generation;
        let _ = applet.preview_due(generation, Some("dim-dark".into()));
        assert!(applet.writing);
        let _ = applet.previewed(Ok(true));
        // Leaving puts back exactly what was there, then the preview is over.
        let _ = applet.pointer_moved(None);
        let generation = applet.preview.generation;
        let _ = applet.preview_due(generation, None);
        assert!(applet.preview.putting_back);
        assert!(applet.preview.original.is_some(), "kept until it worked");
        let _ = applet.previewed(Ok(true));
        assert!(applet.preview.original.is_none());
        // A preview never becomes the applied theme.
        assert_eq!(applet.settings.last_applied, None);
    }

    #[test]
    fn a_failed_put_back_keeps_the_original_to_try_again() {
        let mut applet = applet(&[]);
        start_preview(&mut applet, "grey-dark");
        let _ = applet.previewed(Ok(true));
        let _ = applet.end_preview();
        assert!(applet.preview.putting_back);
        let _ = applet.previewed(Err("disk full".into()));
        assert!(applet.preview.original.is_some());
        let _ = applet.end_preview();
        assert!(applet.preview.putting_back, "tried again");
    }

    #[test]
    fn a_restore_during_a_preview_ends_it() {
        let mut applet = applet(&[]);
        start_preview(&mut applet, "grey-dark");
        let _ = applet.previewed(Ok(false));
        assert!(applet.preview.original.is_none());
    }

    #[test]
    fn a_hover_waits_for_a_running_write() {
        let mut applet = applet(&[]);
        start_preview(&mut applet, "grey-dark");
        // Still writing grey-dark when dim-dark is due.
        let _ = applet.pointer_moved(Some("dim-dark".into()));
        let generation = applet.preview.generation;
        let _ = applet.preview_due(generation, Some("dim-dark".into()));
        assert!(matches!(applet.preview.waiting, Some(Pending::Show(_))));
        let _ = applet.previewed(Ok(true));
        assert!(applet.writing, "the waiting theme is written next");
    }

    #[test]
    fn a_new_preview_waits_for_a_running_write() {
        let mut applet = applet(&[]);
        let _ = applet.pointer_moved(Some("grey-dark".into()));
        let generation = applet.preview.generation;
        applet.writing = true;
        let _ = applet.preview_due(generation, Some("grey-dark".into()));
        assert!(applet.preview.original.is_none(), "not started yet");
    }

    #[test]
    fn a_click_keeps_the_original_until_its_theme_is_written() {
        let mut applet = applet(&["grey-dark"]);
        start_preview(&mut applet, "grey-dark");
        let _ = applet.previewed(Ok(true));
        let generation = applet.preview.generation;
        let _ = applet.next();
        assert!(applet.preview.generation > generation, "no more dwells");
        assert!(applet.preview.original.is_some());
        let _ = applet.applied("grey-dark".into(), Err("disk full".into()));
        assert!(applet.preview.original.is_some(), "can still be put back");
        let _ = applet.next();
        let _ = applet.applied("grey-dark".into(), Ok(()));
        assert!(applet.preview.original.is_none());
    }

    #[test]
    fn a_click_during_a_write_runs_when_it_finishes() {
        let mut applet = applet(&["grey-dark", "dim-dark"]);
        let _ = applet.next();
        assert!(applet.writing);
        let _ = applet.next();
        let _ = applet.next();
        assert!(applet.click_waiting.is_some(), "not dropped");
        let _ = applet.applied("grey-dark".into(), Ok(()));
        assert!(applet.writing, "the waiting click is written next");
        assert!(
            applet.click_waiting.is_none(),
            "the clicks that waited count as one"
        );
        let _ = applet.applied("dim-dark".into(), Ok(()));
        assert!(!applet.writing);
        assert_eq!(applet.settings.last_applied.as_deref(), Some("dim-dark"));
    }

    #[test]
    fn a_click_with_nothing_toggled_on_still_puts_a_preview_back() {
        let mut applet = applet(&[]);
        start_preview(&mut applet, "grey-dark");
        // The list closed while the preview was being written.
        let _ = applet.end_preview();
        let _ = applet.next();
        let _ = applet.previewed(Ok(true));
        assert!(applet.writing, "the original is being put back");
        let _ = applet.previewed(Ok(true));
        assert!(applet.preview.original.is_none());
    }

    #[test]
    fn a_click_before_the_themes_are_found_runs_once_they_are() {
        let mut applet = applet(&["grey-dark"]);
        let scan = applet.scan.take();
        let _ = applet.next();
        assert!(applet.click_waiting.is_some() && !applet.writing);
        let _ = applet.scanned(scan);
        assert!(applet.writing, "the click runs");
    }

    #[test]
    fn taken_out_of_every_panel_puts_the_original_back_once() {
        let mut applet = applet(&[]);
        applet.panel_ids = vec!["panel".into(), "dock".into()];
        // Starting up: one panel holds it, the other hasn't reported.
        let _ = applet.panel_changed("panel".into(), true);
        let _ = applet.panel_changed("dock".into(), false);
        assert!(!applet.reverted);
        // Taken out.
        let _ = applet.panel_changed("panel".into(), false);
        assert!(applet.reverted);
    }

    #[test]
    fn nothing_is_put_back_before_every_panel_reported() {
        let mut applet = applet(&[]);
        applet.panel_ids = vec!["panel".into(), "dock".into()];
        let _ = applet.panel_changed("dock".into(), false);
        assert!(!applet.reverted);
    }

    #[test]
    fn run_outside_any_panel_nothing_is_put_back() {
        let mut applet = applet(&[]);
        applet.panel_ids = vec!["panel".into()];
        let _ = applet.panel_changed("panel".into(), false);
        assert!(!applet.reverted);
    }

    #[test]
    fn the_popup_builds_in_every_state() {
        let mut applet = applet(&["dim-dark"]);
        applet.notice = Some(Notice::NoneSelected);
        let _ = applet.popup_content();
        applet.scan = None;
        let _ = applet.popup_content();
    }
}
