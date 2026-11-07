# Workspace

The root `Cargo.toml` is the workspace for the standalone COSMIC applet:
a separately installable panel applet, built against the libcosmic that the
installed Pop!_OS COSMIC (epoch-1.11.0) uses, replacing no stock component.

## Layout

| Path | What it is |
| --- | --- |
| `crates/cycler-core` | The rules and files, with no GUI: theme discovery and import (`library`), the built-in themes (`builtin`), the cycle order (`cycle`), the applet's settings (`settings`), and whether the applet is in a panel (`panel`). |
| `crates/cosmic-theme-cycler` | The libcosmic applet: the panel button and its live tooltip, the theme list with hover preview, applying themes (`apply`, `fastwrite`), `--restore` and the snapshot (`restore`), the settings saver, and putting the original back when removed (`shutdown`). |
| `scripts/check.sh` | The CI checks, runnable locally. |
| `.github/workflows/ci.yml` | CI: formatting, clippy, tests, desktop entry validation. |
| `.github/workflows/release.yml` | Builds the `.deb` for a published release, attaches it, and adds it to the signed apt repository (see [RELEASE.md](../RELEASE.md)). |
| `scripts/publish-apt.sh` | Rebuilds and signs the apt repository on the `gh-pages` branch. |

## Pinned COSMIC baseline

Pinned through `[patch]` in the root manifest, so every COSMIC crate
resolves to the commits the installed panel was built with (`[workspace.metadata.cosmic-baseline]` lists them):

- release epoch-1.11.0, cosmic-applets `328d142f`
- libcosmic `430f3d48`
- cosmic-protocols `c0cff4db`

## Building

Rust 1.93 or later, plus `libxkbcommon-dev` and `libwayland-dev` (probed by
the Wayland crates' build scripts):

```sh
cargo build --locked -p cosmic-theme-cycler
./scripts/check.sh
```

Adding a dependency that is already in `Cargo.lock` still fails under
`--locked`; run `cargo metadata --offline` once and check that `git diff
Cargo.lock` shows only new edges.
