# The applet: design

What the theme cycler does, and how.

## Using it

- **Left-click** the panel icon: the next toggled theme, in list order,
  wrapping round. With nothing toggled, the list opens. A click that comes
  while a theme is still being written (or a preview put back) runs as soon
  as that finishes; several such clicks count as one. The icon's tooltip
  names the theme shown.
- **Right-click** the panel icon: the list.
  - **Import theme…** at the top adds a `.ron` theme file (through the
    desktop's file chooser) to the user's library.
  - Every theme found, grouped **Light** then **Dark**, each group from the
    brightest background to the darkest. A toggle on each puts it in the
    cycle; the list never moves when one is toggled. The cycle follows
    this order.
  - **Your original theme**, the one COSMIC showed before the cycler first
    changed anything, is in the list too, so it can be cycled to.
  - **Resting the pointer** on a theme for a moment (200 ms) shows it on
    the desktop straight away, with no fade; moving to another shows that
    one; leaving the list, or closing it, puts back exactly what was there
    (both of COSMIC's slots and its light/dark setting). A preview is never
    recorded as the applied theme.
- The theme changes only when the user does one of these; nothing changes
  it by itself.
- All spacing in the popup comes from the COSMIC theme.

## Themes

COSMIC themes are exported `ThemeBuilder` files (`.ron`), the files COSMIC
Settings imports and exports. The applet changes **only the theme** and
COSMIC's light/dark setting to match, never the wallpaper. It lists:

- the themes built in (`cycler_core::builtin`): COSMIC's own default Dark
  and Light, read as COSMIC reads them (the user's data folder, then
  `/usr/share/cosmic`), checked for non-finite numbers, so they are what an
  untouched desktop shows; and ten more, built the way System76 builds its
  presets: Midnight, Frost and Overcast,
  Moonfly, Catppuccin Latte and Mocha, Tokyo Night and Tokyo Night Light,
  Nord, and Kanagawa Wave;
- the system presets, `/usr/share/cosmic-themes/*.ron`;
- the user's library, `~/.local/share/cosmic-theme-cycler/themes/`.

A theme is dark or light according to its palette. A file that doesn't
parse, or holds a non-finite number, is listed as unreadable and never
applied.

## Applying a theme

COSMIC keeps a dark and a light theme slot
(`com.system76.CosmicTheme.{Dark,Light}`, each with a `.Builder`) and shows
the one its light/dark setting picks. The applet writes the theme into the
slot of its own kind, builder then theme, then sets COSMIC's light/dark
setting to that kind (only that key, only if it differs), as COSMIC
Settings does when it imports a theme of the other kind. So a light theme
is shown as a light theme, and apps pick text colours for it. COSMIC's
automatic light/dark switching is left as it is, and if the user (or that
switching) flips light/dark, the applet doesn't fight it: COSMIC then shows
the last theme of the other kind.

## Previewing

Before the first theme it shows, a preview reads exactly what COSMIC shows
(both slots' theme and builder and the light/dark setting, holding the lock
so no write is half done). Each hovered theme is written as a click would,
but not recorded. When the pointer leaves or the list closes, that reading
is written back as it was, so a light theme only hovered doesn't stay in
COSMIC's light slot. Writes go one at a time; a hover that comes during a
write is shown when it finishes. The original is kept until putting it back
has worked, so a failed write can be retried by leaving or closing again. A
click during a preview puts the original back and applies its theme in one
step. If `--restore` runs during a preview, the preview stops rather than
undo it.

## When the applet is removed

cosmic-panel stops a removed applet with SIGTERM and waits for it to exit.
The applet catches it (`shutdown.rs`) and, holding the theme lock until it
exits, puts the original theme back if it had been in a panel and is now in
none. It also watches (read only) the applet lists of COSMIC's panels
(`cycler_core::panel`). Once it has been seen in a panel and then is in
none, it was taken out in COSMIC Settings, and it puts the original theme
back as `--restore` does. Logging out leaves the lists alone, so nothing
happens then. Moving the applet from one panel to another may briefly leave
it in neither, and then puts the original theme back too. Turning off the
only panel the applet is in counts as removing it. Uninstalling the
package doesn't run anything per user: remove the applet from the panel
first, or run `cosmic-theme-cycler --restore`.

## Saving and applying without waiting

Settings and themes are cosmic-config entries, one file per key, each
written atomically and, by cosmic-config, synced to disk. A synced write
takes a few milliseconds on a quiet disk but up to half a second while
something else writes heavily (a large build, say), and a theme is dozens of
keys. Settings changes go to a background writer that writes only the keys
that changed (`saver.rs`), so the popup never waits. A theme is applied by
writing only the keys that differ from what is shown, all at once
(`fastwrite.rs`, checked byte for byte against cosmic-config's own output),
along with COSMIC's light/dark setting and the applied theme's name.

These writes are atomic but not synced: each key is a new file renamed into
place, so COSMIC reads the old file or the new one, never half of either,
and a click never waits on a busy disk. Only a power cut or crash in the
seconds after a click could lose it, and then COSMIC shows the previous theme (or,
for a key that wasn't there before, its default), which `--restore` or the
next click puts right.
What must survive is synced: the record of your original theme, what
`--restore` writes back, and the settings the saver writes. COSMIC's apps
then redraw, which the applet doesn't control.

## Recovery

Before its first write, the applet records both theme slots as COSMIC
resolves them (including keys read from older `v1` folders and the system
defaults) and COSMIC's light/dark setting, in
`~/.local/state/cosmic-theme-cycler/original-theme.ron`.
`cosmic-theme-cycler --restore` writes them back so the desktop updates,
removes the keys that weren't there before (so COSMIC reads its older
settings and defaults again, exactly as before), forgets the applied theme,
and deletes the record so the next change starts a new one. If the
record can't be written, the applet doesn't change the theme.
