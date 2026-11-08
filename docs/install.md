# Installing COSMIC Theme Cycler

> **Not an official COSMIC project.** COSMIC Theme Cycler is an independent,
> community-made applet. It is not made, endorsed, sponsored or supported by
> System76. COSMIC and Pop!_OS are trademarks of System76, used here only to
> say which desktop the applet works with. Please report problems
> [on GitHub](https://github.com/deepthinker2001/cosmic-ext-theme-cycler/issues),
> not to System76. The `cosmic-ext-` prefix follows the COSMIC community's
> naming for third-party extensions.

The applet installs beside the stock COSMIC applets and replaces, edits or
removes none of their files. Only installing needs root, to write system
folders; the applet itself never runs as root.

## With apt

The signed apt repository on GitHub Pages carries every release:

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

To stop receiving updates, remove
`/etc/apt/sources.list.d/cosmic-ext-theme-cycler.list` and
`/usr/share/keyrings/cosmic-ext-theme-cycler.gpg`.

## Upgrading from cosmic-theme-cycler (0.1.x)

Until 0.2.0 the project was called `cosmic-theme-cycler`, and so was its apt
repository. Remove the old repository's two files (the `.list` and the
keyring, as above but with the old name), add the new one, and run
`sudo apt install cosmic-ext-theme-cycler`. The new package takes the old
one's place (it breaks and replaces it). The signing key
is the same.

The first time it starts, the applet renames these folders, unless the new
ones already exist:

| Old | New |
| --- | --- |
| `~/.config/cosmic/io.github.deepthinker2001.CosmicThemeCycler/` | `~/.config/cosmic/io.github.deepthinker2001.CosmicExtThemeCycler/` |
| `~/.local/share/cosmic-theme-cycler/` | `~/.local/share/cosmic-ext-theme-cycler/` |
| `~/.local/state/cosmic-theme-cycler/` | `~/.local/state/cosmic-ext-theme-cycler/` |

Log out and back in once, so the old 0.1.x applet isn't still running from
memory and writing to the old folders. Then add **Theme Cycler** to the
panel again: the panel knows applets by App ID, and the old entry no longer
has an applet behind it.

## Building

Rust 1.93 or later, plus the Wayland development headers:

```sh
sudo apt install libxkbcommon-dev libwayland-dev
just build-release               # as yourself, not root
```

## From the .deb

```sh
cargo install cargo-deb          # once
just deb                         # or: cargo deb --locked -p cosmic-ext-theme-cycler
sudo apt install ./target/debian/cosmic-ext-theme-cycler_*.deb
```

It installs:

| File | Purpose |
| --- | --- |
| `/usr/bin/cosmic-ext-theme-cycler` | the applet, and `--restore` |
| `/usr/share/applications/io.github.deepthinker2001.CosmicExtThemeCycler.desktop` | how COSMIC finds the applet (`X-CosmicApplet=true`) |
| `/usr/share/icons/hicolor/scalable/apps/io.github.deepthinker2001.CosmicExtThemeCycler-symbolic.svg` | the panel icon |
| `/usr/share/doc/cosmic-ext-theme-cycler/` | README, this file, licence |

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
| `~/.config/cosmic/io.github.deepthinker2001.CosmicExtThemeCycler/v1` | the applet's settings |
| `~/.local/share/cosmic-ext-theme-cycler/themes/` | themes you import |
| `~/.local/state/cosmic-ext-theme-cycler/original-theme.ron` | your theme before the first change, for `--restore` |

It never writes COSMIC's automatic light/dark switching, the wallpaper, or
any other COSMIC setting.

## Getting your theme back

```sh
cosmic-ext-theme-cycler --restore
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
   `cosmic-ext-theme-cycler --restore` instead.
2. `sudo apt remove cosmic-ext-theme-cycler`, or `sudo ~/.cargo/bin/just uninstall` if you
   installed from source.

Your settings, imported themes and the restore record stay in the folders
above until you delete them.
