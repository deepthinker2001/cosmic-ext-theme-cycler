# Changelog

## 0.3.0

- New **Auto** toggle in the right-click list: the light theme you last
  picked is applied after sunup and the dark one after sunset. Set your
  latitude and longitude there, or leave them empty to use an estimate from
  your time zone; the applet never uses the network for this. Themes you
  switch by hand while Auto is on teach it your usual switch times. The
  settings are saved with the others.

## 0.2.0

- Renamed from `cosmic-theme-cycler` to `cosmic-ext-theme-cycler`, following
  the COSMIC community's `cosmic-ext-` naming for third-party extensions.
  The package, binary, GitHub repository
  (`deepthinker2001/cosmic-ext-theme-cycler`) and apt repository
  (`https://deepthinker2001.github.io/cosmic-ext-theme-cycler`) all use the
  new name. The App ID is now `io.github.deepthinker2001.CosmicExtThemeCycler`.
- The new package breaks and replaces
  `cosmic-theme-cycler`, so installing it removes the old one.
- On first start (and on `--restore`) the applet moves 0.1.x's settings,
  imported themes and original-theme record to the new folders. A folder is
  moved only if the new one doesn't exist yet; nothing is overwritten.
- After upgrading, add the applet to the panel again: COSMIC knows it by
  its App ID, which changed.
- The README and install guide now say that the project is independent and
  not endorsed by System76.

## 0.1.1

- Themes change colours only. COSMIC stores some of your settings beside a
  theme's colours (Interface density, Style roundness, window gaps, the
  active window hint and frosted glass), and theme files carry their own.
  The applet now keeps yours, both when showing a theme and on `--restore`.
  Before, a theme could switch a Compact desktop to Standard spacing, and
  while previewing, the theme list grew and shrank under the pointer, which
  made it hard to click.
- Installable with apt from a signed repository, so system updates bring
  new versions (set up just after the 0.1.0 release; see the README).

## 0.1.0

First release.

- A standalone COSMIC panel applet that cycles the desktop theme through the
  COSMIC themes you toggle on. Left-click the panel icon for the next theme;
  right-click for the list. The icon's tooltip names the theme shown.
- Themes grouped light then dark, brightest first. Rest the pointer on one to
  try it on the desktop; move away and your theme comes back.
- COSMIC's own Dark and Light, ten built-in themes (Midnight, Frost,
  Overcast, Moonfly, Catppuccin Latte and Mocha, Tokyo Night and Tokyo Night
  Light, Nord, Kanagawa Wave), COSMIC's presets, and themes you import.
- Sets COSMIC's light/dark setting to match each theme; never touches the
  wallpaper or COSMIC's automatic light/dark switching.
- **Your original theme** stays in the list; removing the applet from the
  panel puts it back, and so does `cosmic-theme-cycler --restore`.
