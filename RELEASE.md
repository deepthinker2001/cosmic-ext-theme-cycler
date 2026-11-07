# Making a release

Publishing a GitHub release does most of the work: the
[Release workflow](.github/workflows/release.yml) builds the `.deb` from the
release's tag, attaches it with its checksum to the release, and adds it to
the signed apt repository that GitHub Pages serves from the `gh-pages`
branch. apt users get the new version with their next system update.

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

Don't attach a `.deb` yourself: the workflow builds and attaches it. A
pre-release gets its `.deb` but is not added to the apt repository. Publish
releases one at a time: if a third run queues behind a running one, GitHub
drops the waiting one, and it has to be rerun by hand.

## 3. Check that it worked

1. **Actions → Release:** all three jobs, `build`, `publish` and `apt`,
   should pass (about 5 minutes).
2. **The release page** should list `cosmic-theme-cycler_0.2.0-1_amd64.deb`
   and its `.sha256`.
3. **apt:** on a machine with the repository added, `sudo apt update` then
   `apt policy cosmic-theme-cycler` should show the new version as the
   candidate.

## If the workflow fails

- **Rerun it for the tag**, after fixing the cause on `main`:

  ```sh
  gh workflow run release.yml -f tag=v0.2.0
  ```

  The build uses the source at the tag; the publishing script comes from
  `main`. If the fix has to be in the source, make a new version instead.

- **Publish by hand.** With the signing key's keyring on your machine and a
  checkout of the `gh-pages` branch:

  ```sh
  GNUPGHOME=<keyring folder> scripts/publish-apt.sh <gh-pages checkout> \
      1B33304154C28AB60EC9F8EDBEF9D28E08BDA02A cosmic-theme-cycler_0.2.0-1_amd64.deb
  ```

  using the `.deb` from the release page, then commit and push the
  `gh-pages` checkout.

## The signing key

The apt repository is signed with the key
`1B33 3041 54C2 8AB6 0EC9 F8ED BEF9 D28E 08BD A02A`
("deepthinker2001 (COSMIC Theme Cycler apt repository)"). The workflow reads
it from the repository secret `APT_SIGNING_KEY`, and its fingerprint from the
repository variable `APT_SIGNING_KEY_ID`. Its public half is served as
`key.gpg` on GitHub Pages.

**It expires on 2031-11-06.** Extend it well before then, or apt will refuse
the repository:

```sh
GNUPGHOME=<keyring folder> gpg --quick-set-expire 1B33304154C28AB60EC9F8EDBEF9D28E08BDA02A 5y
GNUPGHOME=<keyring folder> gpg --armor --export-secret-keys 1B33304154C28AB60EC9F8EDBEF9D28E08BDA02A \
  | gh secret set APT_SIGNING_KEY
```

Then rerun the workflow for the latest tag, which publishes the extended
public key as `key.gpg` and re-signs the repository. Users need to fetch the
new `key.gpg` again (the first install step in the README); nothing else
changes for them.

If the key is ever lost or exposed, make a new one, replace the secret and
the variable, publish, and tell users to fetch `key.gpg` again: the old key
can no longer be trusted, so apt would reject the repository until they do.
