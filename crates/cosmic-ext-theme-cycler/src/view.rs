//! The popup's widgets, as pure functions of the applet's state.
//!
//! Top to bottom: Import theme…, a notice when there is one, then the theme
//! list: light themes, then dark ones, each brightest first, every theme a
//! toggle for the cycle. Resting the pointer on a theme previews it. No
//! tooltips: in a popup they cover the buttons. All spacing comes from the
//! COSMIC theme, never fixed pixel values.

use cosmic::iced::{Color, Length};
use cosmic::widget::{
    button, column, container, mouse_area, row, scrollable, text, text_input, toggler,
};
use cosmic::{Element, applet, iced};
use cycler_core::Variant;
use cycler_core::library::{LibraryError, ReadError, Theme};

use crate::app::{ImportError, Message, Notice, ThemeCycler};
use crate::fl;

impl ThemeCycler {
    pub(crate) fn popup_content(&self) -> Element<'_, Message> {
        let spacing = cosmic::theme::spacing();
        column::with_capacity(5)
            .padding([spacing.space_xxs, 0])
            .spacing(spacing.space_xxs)
            .width(Length::Fill)
            .push(applet::padded_control(
                button::standard(fl!("import"))
                    .on_press(Message::Import)
                    .width(Length::Fill),
            ))
            .push(self.auto_view())
            .extend(
                self.notice.as_ref().map(|notice| {
                    applet::padded_control(text::caption(notice_text(notice))).into()
                }),
            )
            .push(self.themes_view())
            .into()
    }

    /// The Auto toggle and the place it works from: latitude and longitude
    /// boxes, with a line saying where the place comes from.
    fn auto_view(&self) -> Element<'_, Message> {
        let spacing = cosmic::theme::spacing();
        let toggle = toggler(self.settings.auto)
            .label(fl!("auto"))
            .on_toggle(Message::SetAuto)
            .width(Length::Fill);
        let boxes = row::with_capacity(2)
            .spacing(spacing.space_xxs)
            .push(
                text_input(fl!("latitude"), &self.latitude_text)
                    .on_input(Message::Latitude)
                    .width(Length::Fill),
            )
            .push(
                text_input(fl!("longitude"), &self.longitude_text)
                    .on_input(Message::Longitude)
                    .width(Length::Fill),
            );
        let hint = if self.settings.place_milli().is_some() {
            fl!("place-entered")
        } else {
            fl!("place-estimated")
        };
        column::with_capacity(3)
            .spacing(spacing.space_xxxs)
            .push(applet::padded_control(toggle))
            .push(applet::padded_control(boxes))
            .push(note(hint))
            .into()
    }

    /// Every theme found, grouped light then dark, each brightest first,
    /// with a toggle to put it in the cycle; the cycle goes through the
    /// themes toggled on in this order.
    fn themes_view(&self) -> Element<'_, Message> {
        let spacing = cosmic::theme::spacing();
        let Some(scan) = &self.scan else {
            return applet::padded_control(text::body(fl!("looking"))).into();
        };
        // A row per theme, plus the two group headings and a note.
        let mut list = column::with_capacity(scan.themes.len() + 3).spacing(spacing.space_xxxs);
        if scan.themes.is_empty() {
            list = list.push(applet::padded_control(text::body(fl!("no-themes"))));
        }
        let mut group = None;
        for theme in self.listed_themes() {
            if group != Some(theme.variant) {
                group = Some(theme.variant);
                let heading = match theme.variant {
                    Variant::Light => fl!("light-themes"),
                    Variant::Dark => fl!("dark-themes"),
                };
                list = list.push(applet::padded_control(text::heading(heading)));
            }
            let selected = self.settings.selected.contains(&theme.key);
            list = list.push(theme_row(theme, selected));
        }
        if !scan.unreadable.is_empty() {
            list = list.push(note(fl!("unreadable", count = scan.unreadable.len())));
        }
        // As tall as its themes, up to about four rows' worth (a padded
        // toggle row is close to `space_xxxl` tall); a longer library
        // scrolls.
        container(scrollable(list))
            .max_height(f32::from(spacing.space_xxxl) * 4.0)
            .into()
    }

    /// The themes the list shows, in list order: only themes that exist.
    pub(crate) fn listed_themes(&self) -> Vec<&Theme> {
        self.ordered_keys()
            .iter()
            .filter_map(|key| self.theme(key))
            .collect()
    }
}

/// One theme: a toggle with its name. Resting the pointer on it previews
/// the theme on the desktop.
fn theme_row(theme: &Theme, selected: bool) -> Element<'_, Message> {
    let key = theme.key.clone();
    let toggle = toggler(selected)
        .label(theme.name.clone())
        .on_toggle(move |on| Message::Select(key.clone(), on))
        .width(Length::Fill);
    mouse_area(applet::padded_control(toggle))
        .on_enter(Message::HoverTheme(theme.key.clone()))
        .on_exit(Message::LeaveTheme)
        .into()
}

/// A small muted line.
fn note<'a>(line: String) -> Element<'a, Message> {
    applet::padded_control(text::caption(line).class(cosmic::theme::Text::Custom(muted))).into()
}

/// Secondary text: the theme's text colour at reduced opacity.
fn muted(theme: &cosmic::Theme) -> iced::widget::text::Style {
    let mut color: Color = theme.cosmic().on_bg_color().into();
    color.a *= 0.7;
    iced::widget::text::Style {
        color: Some(color),
        ..Default::default()
    }
}

fn notice_text(notice: &Notice) -> String {
    match notice {
        Notice::NoneSelected => fl!("none-selected"),
        Notice::ApplyFailed(error) => fl!("apply-failed", error = error.as_str()),
        Notice::Imported(name) => fl!("imported", name = name.as_str()),
        Notice::ImportFailed(error) => fl!("import-failed", error = import_error_text(error)),
    }
}

fn import_error_text(error: &ImportError) -> String {
    match error {
        ImportError::Library(LibraryError::InvalidName) => fl!("import-invalid-name"),
        ImportError::Library(LibraryError::AlreadyExists(path)) => {
            let name = path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default();
            fl!("import-exists", name = name)
        }
        ImportError::Library(LibraryError::Theme(reason)) => read_error_text(reason),
        ImportError::Library(LibraryError::Io(error)) => {
            fl!("import-write-failed", error = error.as_str())
        }
        ImportError::NoLibrary => fl!("no-library"),
        ImportError::NotALocalFile => fl!("not-a-local-file"),
        ImportError::ChooserFailed => fl!("chooser-failed"),
        ImportError::Crashed => fl!("apply-crashed"),
    }
}

fn read_error_text(reason: &ReadError) -> String {
    match reason {
        ReadError::TooLarge { .. } => fl!("theme-too-large"),
        ReadError::NotAFile => fl!("not-a-file"),
        ReadError::Io(error) => fl!("theme-unreadable", error = error.as_str()),
        ReadError::Parse { line, column } => {
            fl!("theme-not-cosmic", line = line, column = column)
        }
        ReadError::NotFinite => fl!("theme-not-finite"),
        // Only a scan finds duplicates; an import refuses a taken name.
        ReadError::Duplicate { .. } => fl!("theme-duplicate"),
    }
}
