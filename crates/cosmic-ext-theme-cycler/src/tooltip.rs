//! The panel icon's tooltip, naming the theme shown, kept up to date while
//! it is open.
//!
//! libcosmic's `applet_tooltip` copies its text into the tooltip when the
//! pointer arrives, so after a click an open tooltip would keep the old
//! theme's name until the pointer left and came back. This is the same
//! tooltip (libcosmic 430f3d48, `src/applet/mod.rs`), but its view reads the
//! name from a shared value each time it is drawn, which the applet updates.
//! Values not taken from the COSMIC theme are copied from libcosmic's.

use std::sync::{Arc, LazyLock, RwLock};
use std::time::Duration;

use cosmic::applet::cosmic_panel_config::PanelAnchor;
use cosmic::cctk::sctk::reexports::protocols::xdg::shell::client::xdg_positioner::{
    Anchor, Gravity,
};
use cosmic::iced::runtime::platform_specific::wayland::popup::{SctkPopupSettings, SctkPositioner};
use cosmic::iced::{Limits, Point, Rectangle, Size, window};
use cosmic::widget::wayland::tooltip::widget::Tooltip;
use cosmic::widget::{autosize, layer_container};
use cosmic::{Element, surface};

/// The tooltip's popup, one per applet process.
static WINDOW_ID: LazyLock<window::Id> = LazyLock::new(window::Id::unique);
/// The tooltip's autosizing content.
static CONTENT_ID: LazyLock<cosmic::widget::Id> =
    LazyLock::new(|| cosmic::widget::Id::new("theme-cycler-tooltip"));

/// The text the tooltip shows, shared with its open popup.
pub(crate) type Name = Arc<RwLock<String>>;

/// `content` with a tooltip showing `name` as it is when drawn. No tooltip
/// while the applet's own popup is open, as with libcosmic's.
pub(crate) fn applet_tooltip<'a, Message: Clone + 'static>(
    anchor: PanelAnchor,
    content: impl Into<Element<'a, Message>>,
    name: &Name,
    has_popup: bool,
    on_surface_action: impl Fn(surface::Action<Message>) -> Message + Clone + 'static,
) -> Tooltip<'a, Message, Message> {
    let name = Arc::clone(name);
    Tooltip::new(
        content,
        (!has_popup).then_some(move |bounds: Rectangle| popup_settings(anchor, bounds)),
        move || {
            let text = name.read().map(|name| name.clone()).unwrap_or_default();
            Element::from(autosize::autosize(
                layer_container(cosmic::widget::text(text))
                    .layer(cosmic::cosmic_theme::Layer::Background)
                    .padding(cosmic::theme::spacing().space_xxxs),
                CONTENT_ID.clone(),
            ))
        },
        on_surface_action.clone()(surface::Action::DestroyPopup(*WINDOW_ID)),
        on_surface_action,
    )
    // libcosmic's delay.
    .delay(Duration::from_millis(100))
}

/// Where the tooltip opens: beside the icon, away from the panel's edge.
#[expect(
    clippy::cast_possible_truncation,
    reason = "panel button bounds are small, finite surface coordinates"
)]
fn popup_settings(anchor: PanelAnchor, bounds: Rectangle) -> SctkPopupSettings {
    let (popup_anchor, gravity) = match anchor {
        PanelAnchor::Left => (Anchor::Right, Gravity::Right),
        PanelAnchor::Right => (Anchor::Left, Gravity::Left),
        PanelAnchor::Top => (Anchor::Bottom, Gravity::Bottom),
        PanelAnchor::Bottom => (Anchor::Top, Gravity::Top),
    };
    SctkPopupSettings {
        parent: window::Id::RESERVED,
        id: *WINDOW_ID,
        grab: false,
        // As libcosmic's: an empty input zone off the surface, so the
        // tooltip never takes the pointer from the icon.
        input_zone: Some(vec![Rectangle::new(
            Point::new(-1000., -1000.),
            Size::default(),
        )]),
        positioner: SctkPositioner {
            size: None,
            size_limits: Limits::NONE.min_width(1.).min_height(1.),
            anchor_rect: Rectangle {
                x: bounds.x.round() as i32,
                y: bounds.y.round() as i32,
                width: bounds.width.round() as i32,
                height: bounds.height.round() as i32,
            },
            anchor: popup_anchor,
            gravity,
            // libcosmic's: slide and flip along both axes to stay on screen.
            constraint_adjustment: 15,
            offset: (0, 0),
            reactive: true,
        },
        parent_size: None,
        close_with_children: true,
    }
}
