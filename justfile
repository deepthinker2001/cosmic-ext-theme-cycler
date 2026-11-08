# Build, install and uninstall the theme cycler applet beside stock COSMIC.
# Same conventions as cosmic-applets' justfile: `rootdir` stages into another
# root (packaging), `prefix` picks /usr or /usr/local.
#
#   just build-release
#   sudo ~/.cargo/bin/just install          # or: just rootdir=/tmp/stage install
#   sudo ~/.cargo/bin/just uninstall
#   just deb                   # .deb in target/debian (cargo-deb)
#
# Only installing needs root, to write system folders. The applet never runs
# as root. Nothing here installs over, edits or removes a stock COSMIC file.

name := 'cosmic-ext-theme-cycler'
appid := 'io.github.deepthinker2001.CosmicExtThemeCycler'
crate := 'crates' / name

rootdir := ''
prefix := '/usr'

base-dir := absolute_path(clean(rootdir / prefix))
bin-dst := base-dir / 'bin' / name
desktop-dst := base-dir / 'share' / 'applications' / appid + '.desktop'
icons-dst := base-dir / 'share' / 'icons'
doc-dst := base-dir / 'share' / 'doc' / name

target-dir := env('CARGO_TARGET_DIR', justfile_directory() / 'target')
bin-src := target-dir / 'release' / name
desktop-src := crate / 'data' / appid + '.desktop'
icons-src := crate / 'data' / 'icons'

default: build-release

build-debug *args:
    cargo build --locked -p {{name}} {{args}}

build-release *args:
    cargo build --locked --release -p {{name}} {{args}}

# The checks CI runs.
check:
    ./scripts/check.sh

clean:
    cargo clean

# Build the .deb (needs cargo-deb: `cargo install cargo-deb`).
deb *args:
    cargo deb --locked -p {{name}} {{args}}

# Install the built applet, desktop entry and icon (after build-release).
install:
    #!/bin/sh
    set -eu
    if [ ! -x '{{bin-src}}' ]; then
        echo "{{bin-src}} not found: run 'just build-release' first (as yourself, not root)." >&2
        exit 1
    fi
    # Never replace a file a package owns (the .deb, or anything of COSMIC's).
    if [ -z '{{rootdir}}' ] && command -v dpkg-query >/dev/null 2>&1; then
        for f in '{{bin-dst}}' '{{desktop-dst}}'; do
            if owner=$(dpkg-query -S "$f" 2>/dev/null); then
                echo "Refusing to overwrite $f, which belongs to a package (${owner%%:*})." >&2
                echo "Remove that package first, or install with another prefix." >&2
                exit 1
            fi
        done
    fi
    install -Dm0755 '{{bin-src}}' '{{bin-dst}}'
    install -Dm0644 '{{desktop-src}}' '{{desktop-dst}}'
    install -Dm0644 README.md '{{doc-dst}}/README.md'
    install -Dm0644 docs/install.md '{{doc-dst}}/install.md'
    install -Dm0644 LICENSE '{{doc-dst}}/LICENSE'
    (cd '{{icons-src}}' && find . -type f) | while read -r icon; do
        install -Dm0644 "{{icons-src}}/$icon" "{{icons-dst}}/$icon"
    done
    if [ -z '{{rootdir}}' ]; then
        gtk-update-icon-cache -q -t '{{icons-dst}}/hicolor' 2>/dev/null || true
    fi

# Remove everything `install` placed. Settings in ~/.config stay (see README).
uninstall:
    #!/bin/sh
    set -eu
    if [ -z '{{rootdir}}' ] && command -v dpkg-query >/dev/null 2>&1; then
        if owner=$(dpkg-query -S '{{bin-dst}}' 2>/dev/null); then
            echo "{{bin-dst}} belongs to the package ${owner%%:*}: remove it with apt instead." >&2
            exit 1
        fi
    fi
    rm -f '{{bin-dst}}' '{{desktop-dst}}'
    rm -rf '{{doc-dst}}'
    (cd '{{icons-src}}' && find . -type f) | while read -r icon; do
        rm -f "{{icons-dst}}/$icon"
    done
    if [ -z '{{rootdir}}' ]; then
        gtk-update-icon-cache -q -t '{{icons-dst}}/hicolor' 2>/dev/null || true
    fi
