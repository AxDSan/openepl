#!/bin/bash
# Build a relocatable OpenEPL release bundle.
#
#   tools/package.sh            -> dist/openepl-<version>-linux-x86_64{,.tar.gz}
#   tools/package.sh --no-tar   -> leave the tree, skip the archive
#
# The bundle mirrors the repository layout on purpose: `openepl` finds its
# runtime by walking up from its own executable, so `bin/openepl` resolving to
# `<root>/runtime` is what makes the tree relocatable with no extra code and no
# environment variables.
#
# NOTE ON THE WORD "RELEASE": this packages the toolchain and the IDE. It is not
# the G8 hardened release profile for programs *users* build — that is Phase 4
# and does not exist yet.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

# The workspace version is the product version. A git description would put
# a commit hash in the artefact name, which reads like a mistake on a download
# page; the commit is recorded in VERSION.commit instead.
VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"
COMMIT="$(git rev-parse --short HEAD 2>/dev/null || echo unknown)"
TRIPLE="linux-x86_64"
NAME="openepl-${VERSION}-${TRIPLE}"
OUT="$ROOT/dist/$NAME"

echo "==> OpenEPL $VERSION -> dist/$NAME"

# --- prerequisites --------------------------------------------------------
# Fail early and by name. A half-built bundle that fails at the link step is
# far more confusing than a missing-dependency message here.
for dep in cargo clang ar pkg-config; do
    command -v "$dep" >/dev/null || { echo "missing required tool: $dep" >&2; exit 1; }
done
if [ ! -f vendor/RmlUi/build/librmlui.a ]; then
    echo "RmlUi is not vendored — run tools/fetch-rmlui.sh" >&2
    exit 1
fi

# --- build ----------------------------------------------------------------
echo "==> compiler (release)"
cargo build --release --quiet

echo "==> studio"
./designer/build.sh >/dev/null

# --- assemble -------------------------------------------------------------
rm -rf "$OUT"
mkdir -p "$OUT"/{bin,licenses}

install -m755 target/release/openepl "$OUT/bin/openepl"
install -m755 designer/openepl-designer "$OUT/bin/openepl-studio"
strip "$OUT/bin/openepl" "$OUT/bin/openepl-studio" 2>/dev/null || true

# The runtime, ABI and support libraries ship as SOURCE: `openepl build`
# compiles and links them into each program, which is what makes dead-stripping
# per-command possible.
for d in runtime abi libs templates examples editors assets; do
    cp -r "$d" "$OUT/$d"
done

# Only user-facing documentation ships.
mkdir -p "$OUT/docs"
cp docs/editors.md "$OUT/docs/editors.md"

# GUI programs link the vendored UI stack, so a user building outside this
# repository needs its headers and static libraries too.
mkdir -p "$OUT/vendor/RmlUi/build" "$OUT/vendor/accesskit-c"
cp -r vendor/RmlUi/Include "$OUT/vendor/RmlUi/Include"
cp -r vendor/RmlUi/Backends "$OUT/vendor/RmlUi/Backends"
cp vendor/RmlUi/build/librmlui.a "$OUT/vendor/RmlUi/build/"
if [ -d vendor/accesskit-c/include ]; then
    cp -r vendor/accesskit-c/include "$OUT/vendor/accesskit-c/include"
    # Only the archive this platform actually links. accesskit-c vendors every
    # target it supports — Windows, Android, arm64 — which is 274 MB of code
    # this bundle can never use, and would quadruple the download.
    AK="lib/linux/x86_64/static"
    mkdir -p "$OUT/vendor/accesskit-c/$AK"
    cp vendor/accesskit-c/$AK/* "$OUT/vendor/accesskit-c/$AK/"
fi

# Licences: RmlUi and AccessKit are statically linked into what we ship, so
# their notices travel with it.
cp LICENSE "$OUT/LICENSE"
cp THIRD-PARTY.md "$OUT/THIRD-PARTY.md"
cp vendor/RmlUi/LICENSE.txt "$OUT/licenses/RmlUi-LICENSE.txt"
for f in vendor/accesskit-c/LICENSE-MIT vendor/accesskit-c/LICENSE-APACHE; do
    [ -f "$f" ] && cp "$f" "$OUT/licenses/accesskit-$(basename "$f").txt"
done

# Drop build artefacts that must not ship.
find "$OUT/examples" "$OUT/libs" -name '*.o' -o -name '*.so' -o -name '*.ll' 2>/dev/null \
    | xargs -r rm -f

sed -e "s/__VERSION__/$VERSION/g" tools/bundle-README.md > "$OUT/README.md"
printf '%s\ncommit %s\n' "$VERSION" "$COMMIT" > "$OUT/VERSION"

# --- archive --------------------------------------------------------------
if [ "${1:-}" != "--no-tar" ]; then
    echo "==> archive"
    ( cd "$ROOT/dist" && tar czf "$NAME.tar.gz" "$NAME" )
    ( cd "$ROOT/dist" && sha256sum "$NAME.tar.gz" > "$NAME.tar.gz.sha256" )
fi

echo
echo "bundle:  dist/$NAME"
[ -f "$ROOT/dist/$NAME.tar.gz" ] && \
    echo "archive: dist/$NAME.tar.gz ($(du -h "$ROOT/dist/$NAME.tar.gz" | cut -f1))"
echo
echo "Verify it with: tools/verify-bundle.sh dist/$NAME"
