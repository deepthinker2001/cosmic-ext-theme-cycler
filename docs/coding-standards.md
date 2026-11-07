# Coding standards: COSMIC

All code here follows the conventions of COSMIC (System76's libcosmic and
cosmic-applets), at the epoch-1.11.0 baseline the workspace pins.

## Upstream policy

The Pop!_OS contributor guide forbids LLM-generated content in issues and
pull requests to Pop!_OS projects. This project is AI-assisted, so nothing
from it is posted upstream.

## Formatting and lints

- rustfmt defaults (cosmic-applets has no `rustfmt.toml`); `cargo fmt --all -- --check`.
- Clippy over all targets and features with the pedantic group, warnings as
  errors: `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings`.
  CI lints with Rust 1.93.1, as cosmic-applets does. A newer local clippy can
  find more; fix what it finds rather than allowing it.
- `unsafe_code = "deny"` across the workspace.
- A lint is silenced only with `#[expect(..., reason = "...")]` on the one
  item, never crate-wide.
- `./scripts/check.sh` runs all of it, as CI does.

## Applet structure (libcosmic idioms)

- **Popups the standard COSMIC way.** The panel button sends a message; the
  update handler opens the popup with
  `surface_task(app_popup::<Self>(live_settings, settings, None))` and closes
  it with `surface_task(destroy_popup(id))`. The `None` view means libcosmic
  renders the popup by calling `view_window(id)`, which wraps the content in
  `core.applet.popup_container(...)`. Surface actions from widgets
  (tooltips) come back as a `Message::Surface` and go out through
  `surface_task` too. Don't pass a view closure to `app_popup`.
- **Theme spacing, not pixels.** Padding and spacing come from
  `cosmic::theme::spacing()` (`space_xxs`, `space_xs`, `space_s`, ...), so the
  applet follows the user's density and theme. No literal pixel values in
  layout code. Sizes the panel decides (the icon button, the popup's anchor
  and size) come from `core.applet`, not from constants.
- Panel widgets come from `core.applet` (`icon_button`, `applet_tooltip`,
  `popup_container`) and `cosmic::applet::padded_control`, as in the stock
  applets, so the applet matches the panel's size and style.
- `type Executor = cosmic::SingleThreadExecutor`. Nothing blocks `update`:
  file and config I/O that can be slow runs as a `Task` whose result comes
  back as a message.
- Settings go through cosmic-config under the App ID, versioned (`v1`).

## Text

- Every user-visible string is a Fluent message in
  `crates/cosmic-theme-cycler/i18n/en/cosmic-theme-cycler.ftl`, looked up with
  `fl!` (checked at compile time). Variable parts are Fluent arguments.
- Fluent wraps arguments in U+2068/U+2069 isolation marks; tests that compare
  text with arguments strip them. Isolation stays on, as in the stock applets.
- Don't generate translations for other languages; COSMIC translations go
  through Weblate.

## Readability

Clear names, short functions, and comments that say why rather than what.
Pure rules (which theme comes next, theme discovery) live in
`cycler-core` with unit tests; the applet crate wires them to libcosmic.

## Commits

Small, single-purpose commits with an imperative subject and a body saying
why and how it was tested. Nearby COSMIC history uses varied styles
(`fix: ...`, `feat(desktop): ...`), so no style is enforced beyond that.
