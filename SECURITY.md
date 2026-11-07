# Security

## Reporting a problem

Please report security problems privately, through GitHub's
[private vulnerability reporting](https://github.com/deepthinker2001/cosmic-theme-cycler/security/advisories/new)
for this repository, rather than in a public issue. Include the version, what
you did, and what happened. Fixes go into a new release; see
[CHANGELOG.md](CHANGELOG.md).

Only the latest release is supported.

## What the applet can touch

The applet runs as you, inside the COSMIC panel. It has no network access,
starts no other programs, and refuses to run as root. It reads:

- theme files: COSMIC's presets in `/usr/share/cosmic-themes`, COSMIC's
  defaults, and the themes you import;
- COSMIC's theme settings and the panels' applet lists, in `~/.config/cosmic`.

It writes only:

- COSMIC's theme slots and its light/dark setting, in `~/.config/cosmic`;
- its own settings, in `~/.config/cosmic/io.github.deepthinker2001.CosmicThemeCycler/`;
- themes you import, in `~/.local/share/cosmic-theme-cycler/themes/`;
- the record of your original theme and a lock file, in
  `~/.local/state/cosmic-theme-cycler/`.

[docs/install.md](docs/install.md) lists every file.

## Security review

Every change was reviewed for security before it was merged, and the whole
codebase was audited again before the first public release (0.1.0). The
audit checked:

- **Untrusted theme files.** A theme you import may come from anyone. Files
  are opened without blocking (so a named pipe can't hang the applet),
  must be regular files, and are refused above 1 MiB before being read.
  Parsing is bounded in nesting depth (tested to 400,000 levels). Every
  number is checked after parsing, so a theme holding NaN, infinity or a
  value too large for COSMIC's colours is refused, wherever it is hidden.
  Parse errors report only a line and column, never the file's contents.
- **File names and paths.** An imported theme's file name is reduced to
  letters, digits and dashes, at most 100 bytes, so it can't leave the
  theme library. Theme keys are never used to build a path. The record of
  your original theme decides only which key files to keep, never which to
  delete.
- **Writing files.** Every write goes to a new temporary file created
  exclusively, then renamed into place, so a reader never sees half a file
  and a symbolic link in the way is replaced, not written through. The
  record of your original theme, what `--restore` writes back and the
  settings saved from the popup are synced to disk; showing a theme is not,
  so a click never waits on a busy disk. The library never overwrites an existing theme. Files read back
  for comparison must be regular files and are size-bounded. The lock and
  the record of your original theme are opened without following symbolic
  links.
- **Concurrent changes.** Applying, previewing, restoring and putting the
  original back on removal all hold one lock, so they never interleave.
- **Crashes.** Malformed input produces an error, not a panic.
- **Signals.** The panel stops the applet with SIGTERM. It is handled on an
  ordinary thread, not in signal context.
- **Code.** `unsafe` code is denied across the workspace. Clippy runs with
  pedantic lints as errors.
- **Build and release.** CI runs only on pushes and pull requests, with a
  read-only token. Actions are pinned by commit and containers by digest.
  Every build uses the locked dependency versions and pinned COSMIC
  commits. The release `.deb` is built in a job with no write access; a
  separate job, which runs no build code, attaches it and its SHA-256
  checksum to the release; a third, which also runs no build code, signs
  the apt repository. The signing key is a repository secret that only
  that job uses; its public half is `key.gpg` on the `gh-pages` branch.
- **The repository.** No secrets, tokens or keys.

### Known limits

- **Values in range but extreme.** A theme can still hold very large gaps,
  radii or spacing, which COSMIC may draw badly. COSMIC Settings' own import
  accepts the same values. If a theme breaks your desktop, run
  `cosmic-theme-cycler --restore`.
- **Release downloads.** The `.sha256` beside each `.deb` on a release page
  shows the download is intact, not who built it. The apt repository is
  signed (key `1B33 3041 54C2 8AB6 0EC9 F8ED BEF9 D28E 08BD A02A`), so apt
  checks both.
- **Dependency advisories** are not yet checked automatically in CI.
- **Installing from source** with `sudo just install` runs this repository's
  `justfile` as root. Read it first, or install the `.deb`.
