# Theme Cycler for COSMIC

A standalone panel applet for the Pop!_OS COSMIC desktop that cycles the
desktop theme through the COSMIC themes you choose (`cosmic-ext-theme-cycler`).
It changes only the theme's colours (and COSMIC's light/dark setting to
match). It keeps your Interface density, roundness, window gaps, window hint
and frosted glass as they are, never touches the wallpaper, and installs
beside stock COSMIC without replacing any of its components.

> **Not an official COSMIC project.** Theme Cycler for COSMIC is an independent,
> community-made applet. It is not made, endorsed, sponsored or supported by
> System76. COSMIC and Pop!_OS are trademarks of System76, used here only to
> say which desktop the applet works with. Please report problems
> [here](https://github.com/deepthinker2001/cosmic-ext-theme-cycler/issues),
> not to System76. The `cosmic-ext-` prefix follows the COSMIC community's
> naming for third-party extensions.

## Status

**0.2.0.** In daily use on Pop!_OS 24.04 with COSMIC
epoch-1.11.0, the release it is built against. The design is in
[docs/applet.md](docs/applet.md); the changes in each release are under
[Changelog](#changelog) below.

Applying a theme replaces the theme in COSMIC's slot for its kind and
switches COSMIC's light/dark setting to match. Your own theme and setting
are recorded first, and `cosmic-ext-theme-cycler --restore` puts them back (see
Troubleshooting).

## What it does

- **Left-click** the panel icon for the next theme you've toggled on; its
  tooltip names the theme shown.
- **Right-click** it for the list: **Import theme…** at the top, then every
  theme, light ones then dark ones, each brightest first, with a toggle to
  put it in the cycle, including **Your original theme** (what COSMIC showed
  before the cycler). Rest the pointer on a theme to try it on the desktop;
  move away and your theme comes back.
- **Auto** (in the right-click list, off by default) applies your light
  theme after sunup and your dark theme after sunset. Enter your latitude
  and longitude, or leave them empty for a rough estimate from your time
  zone; it never uses the network. It applies the light and dark theme you
  last picked by hand and learns the times you usually switch.
- Removing the applet from the panel puts your original theme back.
- Themes: COSMIC's own default Dark and Light, ten more built in
  (Midnight, Frost, Overcast, Moonfly, Catppuccin Latte and Mocha, Tokyo
  Night and Tokyo Night Light, Nord, and Kanagawa Wave), COSMIC's presets in
  `/usr/share/cosmic-themes`, and any you import.
- It changes the theme and COSMIC's light/dark setting to match, nothing
  else (not the wallpaper, not COSMIC's automatic light/dark switching).
- `cosmic-ext-theme-cycler --restore` puts back the theme you had before.

## Install

Upgrading from 0.1.x, when it was called `cosmic-theme-cycler`? See
[Upgrading from cosmic-theme-cycler](#upgrading-from-cosmic-theme-cycler)
first.

### With apt (recommended)

Add the signed repository once; updates then arrive with your normal system
updates:

```sh
curl -fsSL https://deepthinker2001.github.io/cosmic-ext-theme-cycler/key.gpg \
  | sudo gpg --dearmor -o /usr/share/keyrings/cosmic-ext-theme-cycler.gpg
echo "deb [signed-by=/usr/share/keyrings/cosmic-ext-theme-cycler.gpg] https://deepthinker2001.github.io/cosmic-ext-theme-cycler stable main" \
  | sudo tee /etc/apt/sources.list.d/cosmic-ext-theme-cycler.list
sudo apt update
sudo apt install cosmic-ext-theme-cycler
```

The signing key's fingerprint is
`1B33 3041 54C2 8AB6 0EC9 F8ED BEF9 D28E 08BD A02A`.

### From a release

Download the `.deb` from the
[latest release](https://github.com/deepthinker2001/cosmic-ext-theme-cycler/releases/latest),
check it against its `.sha256`, and install it:

```sh
sha256sum -c cosmic-ext-theme-cycler_*.deb.sha256
sudo apt install ./cosmic-ext-theme-cycler_*.deb
```

### From source

```sh
just build-release               # as yourself
sudo ~/.cargo/bin/just install   # binary, desktop entry, icon
```

Or build a .deb: `cargo install cargo-deb` once, then `just deb` and
`sudo apt install ./target/debian/cosmic-ext-theme-cycler_*.deb`.

Building needs Rust 1.93 or later and `libxkbcommon-dev` and
`libwayland-dev` (see [docs/workspace.md](docs/workspace.md)).

More detail, including every file it installs and writes, is in
[docs/install.md](docs/install.md).

### Upgrading from cosmic-theme-cycler

0.2.0 renamed the project from `cosmic-theme-cycler` to
`cosmic-ext-theme-cycler`, and the apt repository moved with it. Swap the
old repository for the new one, then install the new package. It takes
the old package's place:

```sh
sudo rm -f /etc/apt/sources.list.d/cosmic-theme-cycler.list \
  /usr/share/keyrings/cosmic-theme-cycler.gpg
# then add the repository as above, and:
sudo apt install cosmic-ext-theme-cycler
```

The signing key is the same. On its first start the applet moves your
settings, imported themes and the record of your original theme to their
new folders. The new applet has a new ID, though, so COSMIC drops the old
one from the panel. Log out and back in once (so the old 0.1.x applet
isn't still running from memory), then add **Theme Cycler** to the panel
again (below).

## Add it to the panel

COSMIC Settings → Desktop → Panel (or Dock) → **Configure panel applets** →
**Add applet** → **Theme Cycler**. If it isn't listed, close COSMIC Settings
completely and reopen it: it reads the applet list only when it starts.

## Troubleshooting

| Symptom | What to do |
| --- | --- |
| **Want your own theme back** | In a terminal: `cosmic-ext-theme-cycler --restore`. It puts back the theme and light/dark setting you had before the cycler first changed them. |
| Want COSMIC's original look | Toggle **COSMIC Dark** or **COSMIC Light** on and left-click until it shows, or rest the pointer on it in the list. |
| Left-click opens the list instead of changing theme | No theme is toggled on: turn some on. |
| A terminal's text area keeps its own colours | Expected: COSMIC Terminal's colour scheme (View → Color schemes) isn't part of the COSMIC theme. |
| Theme Cycler missing from "Add applet" | Close COSMIC Settings completely and reopen it; it reads the applet list only when it starts. |
| Editing a cycled theme in COSMIC Settings | Settings edits COSMIC's copy; the cycler's list keeps the original. Import an exported copy to cycle the edited one. |

## Uninstall

Remove it from the panel, then `sudo apt remove cosmic-ext-theme-cycler` or
`sudo ~/.cargo/bin/just uninstall`. Settings stay in
`~/.config/cosmic/io.github.deepthinker2001.CosmicExtThemeCycler/` until you
delete them.

## Changelog

What changed in each version. The full notes are in
[CHANGELOG.md](CHANGELOG.md).

### 0.2.0

- **New name: `cosmic-ext-theme-cycler`.** The package, the command
  (`cosmic-ext-theme-cycler --restore`), the GitHub repository and the apt
  repository all use the new name. The `cosmic-ext-` prefix is the COSMIC
  community's naming for third-party extensions. The project is
  independent and not endorsed by System76.
- Upgrading keeps your settings, imported themes and original theme: the
  applet moves them to the new folders on first start. Add the applet to
  the panel again after upgrading (see
  [Upgrading](#upgrading-from-cosmic-theme-cycler)).

### 0.1.1

- **Your settings stay yours.** Showing a theme now changes only its
  colours. Your Interface density, roundness, window gaps, window hint and
  frosted glass stay as you set them, and `--restore` keeps any you changed
  since. Fixes the theme list growing and shrinking under the pointer on a
  Compact desktop, which made it hard to click.
- **Install with apt** from a signed repository, so system updates bring
  new versions.

### 0.1.0

First release.

- Cycles the desktop theme through the themes you toggle on: left-click the
  panel icon for the next, right-click for the list. The tooltip names the
  theme shown.
- Rest the pointer on a theme to try it; move away and yours comes back.
- COSMIC's own Dark and Light, ten built-in themes, COSMIC's presets, and
  themes you import.
- Sets COSMIC's light/dark setting to match; never touches the wallpaper or
  automatic light/dark switching.
- Your original theme stays in the list, and removing the applet or
  `cosmic-theme-cycler --restore` puts it back.

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

Theme Cycler for COSMIC was built using AI. The code, tests and documentation
were written with AI assistance, and the code and security reviews before
each change were AI-assisted too. The author set the requirements, made the
design decisions, tested every behaviour on a real COSMIC desktop, and
decides what is released. Every change passes the same checks as any other
contribution: formatting, clippy with pedantic lints as errors, and the full
test suite in CI.

## Licence

GPL-3.0-only; see [LICENSE](LICENSE).
