# Installing COSMIC Theme Cycler

The applet installs beside the stock COSMIC applets and replaces, edits or
removes none of their files. Only installing needs root, to write system
folders; the applet itself never runs as root.

## Building

Rust 1.93 or later, plus the Wayland development headers:

```sh
sudo apt install libxkbcommon-dev libwayland-dev
just build-release               # as yourself, not root
```

## From the .deb

```sh
cargo install cargo-deb          # once
just deb                         # or: cargo deb --locked -p cosmic-theme-cycler
sudo apt install ./target/debian/cosmic-theme-cycler_*.deb
```

It installs:

| File | Purpose |
| --- | --- |
| `/usr/bin/cosmic-theme-cycler` | the applet, and `--restore` |
| `/usr/share/applications/io.github.deepthinker2001.CosmicThemeCycler.desktop` | how COSMIC finds the applet (`X-CosmicApplet=true`) |
| `/usr/share/icons/hicolor/scalable/apps/io.github.deepthinker2001.CosmicThemeCycler-symbolic.svg` | the panel icon |
| `/usr/share/doc/cosmic-theme-cycler/` | README, this file, licence |

## From source with just

```sh
sudo ~/.cargo/bin/just install                # same files under /usr
sudo ~/.cargo/bin/just prefix=/usr/local install
just rootdir=/tmp/stage install  # stage into a folder, e.g. for packaging
```

`sudo` doesn't search `~/.cargo/bin`, where `cargo install just` puts
`just`, hence the full path (or install `just` from apt).

`install` refuses to overwrite a file another package owns (the .deb, or
anything of COSMIC's): remove that package first, or use another prefix.

## Adding it to the panel

COSMIC Settings → Desktop → Panel (or Dock) → **Configure panel applets** →
**Add applet** → **Theme Cycler**. If it isn't listed, close COSMIC
Settings completely and reopen it: it reads the applet list only when it
starts, so there's no need to log out.

## What it writes, and where

| Where | What |
| --- | --- |
| `~/.config/cosmic/com.system76.CosmicTheme.{Dark,Light}[.Builder]/v2` | the theme being shown: COSMIC's own theme config, the slot of the theme's kind (light or dark) |
| `~/.config/cosmic/com.system76.CosmicTheme.Mode/v1/is_dark` | COSMIC's light/dark setting, set to match the theme, and only when it differs |
| `~/.config/cosmic/io.github.deepthinker2001.CosmicThemeCycler/v1` | the applet's settings |
| `~/.local/share/cosmic-theme-cycler/themes/` | themes you import |
| `~/.local/state/cosmic-theme-cycler/original-theme.ron` | your theme before the first change, for `--restore` |

It never writes COSMIC's automatic light/dark switching, the wallpaper, or
any other COSMIC setting.

## Getting your theme back

```sh
cosmic-theme-cycler --restore
```

puts back the theme and light/dark setting you had before the cycler first
changed them, and works whether the applet is running or not. It writes both
of COSMIC's theme slots back as they were recorded, then removes the key
files the cycler added to their `v2` folders, so COSMIC reads your older
`v1` settings and its defaults again as before. Run it as yourself, not
with `sudo`. If you change your theme in COSMIC Settings after the cycler
first ran, `--restore` still goes back to the theme from before that first
change.

## Uninstalling

1. Remove it from the panel (Configure panel applets). That puts your
   original theme back; if the applet wasn't running, run
   `cosmic-theme-cycler --restore` instead.
2. `sudo apt remove cosmic-theme-cycler`, or `sudo ~/.cargo/bin/just uninstall` if you
   installed from source.

Your settings, imported themes and the restore record stay in the folders
above until you delete them.
