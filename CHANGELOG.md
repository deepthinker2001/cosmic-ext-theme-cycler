# Changelog

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
