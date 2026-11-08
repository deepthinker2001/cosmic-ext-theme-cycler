# Manual test in the panel

Run through this before each release. About ten minutes.

## Install

In your clone:

```sh
just build-release
sudo ~/.cargo/bin/just install
```

Then COSMIC Settings → Desktop → Panel → **Configure panel applets** →
**Add applet** → **Theme Cycler**. (Close and reopen COSMIC Settings if it
isn't listed.)

## Checks

Note anything that looks wrong, with the step number.

1. **The list.** Right-click the panel icon. The popup opens with
   **Import theme…** at the top, then **Light** themes and **Dark** themes,
   each from the brightest to the darkest, each with a toggle. Nothing
   overlaps, and it fits on screen.
2. **Preview.** Rest the pointer on a theme: after a moment the desktop
   switches to it, no fade. Move to others, light and dark: each shows.
   Move off the list: what you had comes back. Close the popup during a
   preview: the same. **Your original theme** is in the list.
3. **Toggle and click.** Toggle on two light and two dark themes (they stay
   in place). Close the popup and left-click the icon a few times: it steps
   through them in list order, light ones then dark, wrapping round, and
   COSMIC's Dark/Light setting follows the theme. Note how long each takes.
4. **Light themes.** With a light theme shown, text in COSMIC apps and the
   panel reads well.
5. **COSMIC's own look.** Toggle on **COSMIC Dark**, click to it: the desktop
   looks as it did out of the box.
6. **Import.** Right-click → **Import theme…**, pick a `.ron` (e.g. a copy
   of `/usr/share/cosmic-themes/mocha-dark.ron` renamed in your Downloads);
   it appears in the list.
7. **Your settings stay.** In COSMIC Settings → Appearance, set Interface
   density to Compact and change Style. Click through a few themes and rest
   the pointer on others: density and roundness stay as you set them, and
   the list keeps its size under the pointer. Set them back afterwards.
8. **Tooltip.** Hover the panel icon: it names the theme shown. Left-click
   while it is open: the name changes with the theme.
9. **Restore.** In a terminal: `cosmic-ext-theme-cycler --restore`. Your
   original theme and Dark/Light setting come back everywhere.
10. **Removal.** Click to some theme, then remove the applet from the panel
   (COSMIC Settings → Desktop → Panel → Configure panel applets): your
   original theme comes back. Add it again afterwards to keep testing.

Expected and not a fault: a terminal's text area keeps its own colours
(COSMIC Terminal's colour scheme, View → Color schemes, isn't part of the
COSMIC theme).

## If something goes badly

`cosmic-ext-theme-cycler --restore` puts your theme back. To remove the applet:
take it out of the panel, then `sudo ~/.cargo/bin/just uninstall`.
