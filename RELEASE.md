# Making a release

Publishing a GitHub release does most of the work: the
[Release workflow](.github/workflows/release.yml) builds the `.deb` from the
release's tag and attaches it, with its checksum, to the release.

## 1. Prepare the release on `main`

1. **Version.** Set the new version (for example `0.2.0`) in the root
   `Cargo.toml`, under `[workspace.package]`. Every crate in the workspace
   takes it from there.
2. **Lock file.** Run `cargo check --workspace` once so `Cargo.lock` records
   the new version. CI builds with `--locked` and fails if it doesn't.
3. **Changelog.** Add a `## 0.2.0` section at the top of `CHANGELOG.md`: what
   changed, from a user's point of view.
4. **Check.** Run `./scripts/check.sh`. It runs what CI runs.
5. **Try it in the panel.** Install the build (`just build-release`, then
   `sudo ~/.cargo/bin/just install`) and go through
   [docs/manual-test.md](docs/manual-test.md).
6. **Commit and push**, then wait for the CI workflow on `main` to pass.

A version is never reused: once a version is published, a fix is a new
version (`0.2.1`), not a changed `0.2.0`.

## 2. Publish the release

The tag must be `v` followed by the version in `Cargo.toml`, such as `v0.2.0`;
the workflow refuses a mismatch.

From a terminal, with the notes in a file (the changelog section works well):

```sh
gh release create v0.2.0 --target main --title "COSMIC Theme Cycler 0.2.0" --notes-file notes.md
```

Or on GitHub: **Releases → Draft a new release**, create the tag `v0.2.0` on
`main`, fill in the title and notes, and **Publish release**.

Don't attach a `.deb` yourself: the workflow builds and attaches it.

## 3. Check that it worked

1. **Actions → Release:** both jobs, `build` and `publish`, should pass (about 5 minutes).
2. **The release page** should list `cosmic-theme-cycler_0.2.0-1_amd64.deb`
   and its `.sha256`.
3. Install it on a test machine with
   `sudo apt install ./cosmic-theme-cycler_0.2.0-1_amd64.deb`.

## If the workflow fails

Fix the cause on `main`, then rerun it for the tag:

```sh
gh workflow run release.yml -f tag=v0.2.0
```

The build uses the source at the tag. If the fix has to be in the source,
make a new version instead.
