#!/usr/bin/env bash
# Builds the release artefacts for peak.
#
# Result (always under dist/, not committed):
#   dist/peak_<version>-1_amd64.deb
#   dist/peak-<version>-x86_64-linux.tar.gz
#
# Usage:  dist/make_release.sh [--skip-build]
#
# One-time prerequisite for the .deb:  cargo install cargo-deb --locked

set -euo pipefail

PROJECT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TARGET_DIR="${CARGO_TARGET_DIR:-$PROJECT_DIR/target}"
BIN="$TARGET_DIR/dist/peak"

if [[ "${1:-}" != "--skip-build" ]]; then
    echo "==> cargo build --profile dist"
    (cd "$PROJECT_DIR" && cargo build --locked --profile dist)
fi
[[ -x "$BIN" ]] || { echo "ERROR: $BIN missing"; exit 1; }
echo "==> binary: $BIN ($(du -h "$BIN" | cut -f1))"

VERSION=$("$BIN" --version | awk '{print $2}')
[[ -n "$VERSION" ]] || { echo "ERROR: could not read version"; exit 1; }
echo "==> version $VERSION"

STAGE=$(mktemp -d)
trap 'rm -rf "$STAGE"' EXIT
NAME="peak-$VERSION-x86_64-linux"
mkdir -p "$STAGE/$NAME"
cp "$BIN" "$PROJECT_DIR/README.md" "$PROJECT_DIR/LICENSE" "$PROJECT_DIR/dist/peak.desktop" "$STAGE/$NAME/"
TARBALL="$PROJECT_DIR/dist/$NAME.tar.gz"
rm -f "$PROJECT_DIR"/dist/peak-*-x86_64-linux.tar.gz
tar -C "$STAGE" -czf "$TARBALL" "$NAME"
echo "==> $TARBALL ($(du -h "$TARBALL" | cut -f1))"

if command -v cargo-deb >/dev/null; then
    rm -f "$PROJECT_DIR"/dist/peak_*.deb
    DEB=$(cd "$PROJECT_DIR" && cargo deb --profile dist --no-build -o "$PROJECT_DIR/dist/" | tail -1)
    [[ -f "$DEB" ]] || { echo "ERROR: cargo deb produced no package"; exit 1; }
    LISTING=$(dpkg-deb -c "$DEB")
    for f in \
        ./usr/bin/peak \
        ./usr/share/applications/peak.desktop \
        ./usr/share/doc/peak/README.md \
        ./usr/share/doc/peak/copyright
    do
        grep -q " $f\$" <<<"$LISTING" || { echo "ERROR: $f missing in deb"; exit 1; }
    done
    if grep -qE '\.git/|/target/|/src/' <<<"$LISTING"; then
        echo "ERROR: source tree leaked into deb"; exit 1
    fi
    echo "==> $DEB ($(du -h "$DEB" | cut -f1))"
else
    echo "NOTE: cargo-deb not installed, .deb skipped (cargo install cargo-deb --locked)"
fi

echo
echo "Release assets (do not commit):"
ls -lh "$PROJECT_DIR"/dist/peak_*.deb "$PROJECT_DIR"/dist/peak-*.tar.gz 2>/dev/null || true
