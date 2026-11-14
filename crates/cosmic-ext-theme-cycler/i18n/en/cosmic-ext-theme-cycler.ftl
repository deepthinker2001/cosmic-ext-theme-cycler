applet-name = Theme Cycler

looking = Looking for themes…
no-themes = No COSMIC themes found.
unreadable = { $count ->
    [one] One theme file could not be read.
   *[other] { $count } theme files could not be read.
}
original-theme = Your original theme
light-themes = Light
dark-themes = Dark

import = Import theme…
import-title = Import a COSMIC theme
theme-files = COSMIC themes

auto = Auto: light by day, dark by night
latitude = Latitude
longitude = Longitude
place-entered = Using the place you entered. Nothing is sent over the network.
place-estimated = No place entered: estimated from your time zone. Enter your latitude and longitude for exact sunrise and sunset.

none-selected = Turn on some themes to cycle through.
apply-failed = Theme not applied: { $error }
apply-crashed = the work stopped unexpectedly
imported = Imported { $name }.
import-failed = Not imported: { $error }
no-library = no folder for your themes (is HOME set?)
not-a-local-file = that file isn't on this computer
chooser-failed = the file chooser didn't open
import-invalid-name = the file name has no letters or digits to name the theme by
import-exists = a theme named { $name } is already in your library
import-write-failed = it couldn't be copied into your library ({ $error })
theme-too-large = the file is too large to be a COSMIC theme
theme-unreadable = the file couldn't be read ({ $error })
theme-not-cosmic = it isn't a COSMIC theme (unexpected content at line { $line }, column { $column })
theme-not-finite = a colour or size in it isn't a finite number
theme-duplicate = a theme of the same name is already listed
not-a-file = something other than a regular file is there
internal-error = something went wrong inside the theme cycler; the log has the details

snapshot-failed = your current theme couldn't be recorded first ({ $error })
snapshot-too-large = the saved theme file is too large to be the cycler's
snapshot-not-finite = the saved theme holds a number that isn't finite
snapshot-unreadable = the saved theme couldn't be read (unexpected content at line { $line }, column { $column })
lock-not-a-file = something other than a file is where the cycler's lock file goes
restore-as-root = Run --restore as yourself, not as root: it works on your own settings.
applet-as-root = Run the theme cycler as yourself, not as root: it works on your own settings.
restore-no-home = no folder to keep your original theme in (is HOME set?)
restore-settings-failed = The cycler's settings could not be updated: { $error }
restored = Your original theme is back.
nothing-to-restore = Nothing to restore: the cycler hasn't changed your theme.
restore-failed = Could not restore your theme: { $error }

usage =
    Usage: cosmic-ext-theme-cycler [--restore | --help | --version]

    A COSMIC panel applet that cycles through the COSMIC themes you choose.
    Add it from COSMIC Settings → Desktop → Panel → Configure panel applets.
    Left-click its icon for the next theme; right-click to choose themes.

    --restore   Put back the theme (and light/dark setting) you had before
                the cycler changed it.
migrate-failed = Couldn't move { $from } to { $to } ({ $error }). Move it by hand, then run this again.
