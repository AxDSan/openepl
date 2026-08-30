#!/bin/bash
# Fetch the prebuilt accesskit-c into vendor/ (gitignored). Run once.
# Pinned per ADR 0007. Prebuilt avoids a cargo + cbindgen toolchain requirement.
set -eu
VER=0.22.3
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DEST="$ROOT/vendor/accesskit-c"

if [ -f "$DEST/include/accesskit.h" ]; then echo "accesskit-c $VER already present"; exit 0; fi
mkdir -p "$ROOT/vendor"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
echo "downloading accesskit-c $VER ..."
curl -fsSL -o "$TMP/ak.zip" \
  "https://github.com/AccessKit/accesskit-c/releases/download/$VER/accesskit-c-$VER.zip"
unzip -q "$TMP/ak.zip" -d "$TMP"
mv "$TMP/accesskit-c-$VER" "$DEST"
echo "accesskit-c $VER at $DEST"
