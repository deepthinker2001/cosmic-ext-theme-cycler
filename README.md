# COSMIC Theme Cycler

A standalone panel applet for the Pop!_OS COSMIC desktop that cycles the
desktop theme through the COSMIC themes you choose (`cosmic-theme-cycler`).
It changes only the theme (and COSMIC's light/dark setting to match), never
the wallpaper, and installs beside stock COSMIC without replacing any of its
components.

## Status

**0.1.0, first release.** In daily use on Pop!_OS 24.04 with COSMIC
epoch-1.11.0, the release it is built against. The design is in
[docs/applet.md](docs/applet.md); the changes in each release are in
[CHANGELOG.md](CHANGELOG.md).

Applying a theme replaces the theme in COSMIC's slot for its kind and
switches COSMIC's light/dark setting to match. Your own theme and setting
are recorded first, and `cosmic-theme-cycler --restore` puts them back (see
Troubleshooting).

## What it does

- **Left-click** the panel icon for the next theme you've toggled on; its
  tooltip names the theme shown.
- **Right-click** it for the list: **Import theme…** at the top, then every
  theme, light ones then dark ones, each brightest first, with a toggle to
  put it in the cycle, including **Your original theme** (what COSMIC showed
  before the cycler). Rest the pointer on a theme to try it on the desktop;
  move away and your theme comes back.
- Removing the applet from the panel puts your original theme back.
- Themes: COSMIC's own default Dark and Light, ten more built in
  (Midnight, Frost, Overcast, Moonfly, Catppuccin Latte and Mocha, Tokyo
  Night and Tokyo Night Light, Nord, and Kanagawa Wave), COSMIC's presets in
  `/usr/share/cosmic-themes`, and any you import.
- It changes the theme and COSMIC's light/dark setting to match, nothing
  else (not the wallpaper, not COSMIC's automatic light/dark switching).
- `cosmic-theme-cycler --restore` puts back the theme you had before.

## Install

### From a release

Download the `.deb` from the
[latest release](https://github.com/deepthinker2001/cosmic-theme-cycler/releases/latest),
check it against its `.sha256`, and install it:

```sh
sha256sum -c cosmic-theme-cycler_*.deb.sha256
sudo apt install ./cosmic-theme-cycler_*.deb
```

### From source

```sh
just build-release               # as yourself
sudo ~/.cargo/bin/just install   # binary, desktop entry, icon
```

Or build a .deb: `cargo install cargo-deb` once, then `just deb` and
`sudo apt install ./target/debian/cosmic-theme-cycler_*.deb`.

Building needs Rust 1.93 or later and `libxkbcommon-dev` and
`libwayland-dev` (see [docs/workspace.md](docs/workspace.md)).

More detail, including every file it installs and writes, is in
[docs/install.md](docs/install.md).

## Add it to the panel

COSMIC Settings → Desktop → Panel (or Dock) → **Configure panel applets** →
**Add applet** → **Theme Cycler**. If it isn't listed, close COSMIC Settings
completely and reopen it: it reads the applet list only when it starts.

## Troubleshooting

| Symptom | What to do |
| --- | --- |
| **Want your own theme back** | In a terminal: `cosmic-theme-cycler --restore`. It puts back the theme and light/dark setting you had before the cycler first changed them. |
| Want COSMIC's original look | Toggle **COSMIC Dark** or **COSMIC Light** on and left-click until it shows, or rest the pointer on it in the list. |
| Left-click opens the list instead of changing theme | No theme is toggled on: turn some on. |
| A terminal's text area keeps its own colours | Expected: COSMIC Terminal's colour scheme (View → Color schemes) isn't part of the COSMIC theme. |
| Theme Cycler missing from "Add applet" | Close COSMIC Settings completely and reopen it; it reads the applet list only when it starts. |
| Editing a cycled theme in COSMIC Settings | Settings edits COSMIC's copy; the cycler's list keeps the original. Import an exported copy to cycle the edited one. |

## Uninstall

Remove it from the panel, then `sudo apt remove cosmic-theme-cycler` or
`sudo ~/.cargo/bin/just uninstall`. Settings stay in
`~/.config/cosmic/io.github.deepthinker2001.CosmicThemeCycler/` until you
delete them.

## Development

```sh
./scripts/check.sh   # what CI runs: fmt, clippy -D warnings, tests, desktop entry
```

Code follows COSMIC's conventions: see
[docs/coding-standards.md](docs/coding-standards.md). The layout of the
workspace is in [docs/workspace.md](docs/workspace.md), the checks to run by
hand in the panel before a release in [docs/manual-test.md](docs/manual-test.md),
and how to make a release in [RELEASE.md](RELEASE.md).

## Security

See [SECURITY.md](SECURITY.md) for what the applet reads and writes, the
results of its security review, and how to report a problem.

## How it was made

COSMIC Theme Cycler was built using AI. The code, tests and documentation
were written with AI assistance, and the code and security reviews before
each change were AI-assisted too. The author set the requirements, made the
design decisions, tested every behaviour on a real COSMIC desktop, and
decides what is released. Every change passes the same checks as any other
contribution: formatting, clippy with pedantic lints as errors, and the full
test suite in CI.

## Licence

GPL-3.0-only; see [LICENSE](LICENSE).
