#!/bin/bash
# Prove a release bundle works away from the repository that built it.
#
#   tools/verify-bundle.sh dist/openepl-<version>-linux-x86_64
#   tools/verify-bundle.sh dist/openepl-<version>-linux-x86_64.tar.gz
#
# Everything runs from a scratch directory with OPENEPL_RUNTIME_DIR unset. That
# variable is set by every test in the repo; leaving it set here would mask the
# exact relocation bugs this script exists to find.
set -uo pipefail

BUNDLE="${1:-}"
[ -n "$BUNDLE" ] || { echo "usage: verify-bundle.sh <bundle-dir|tarball>" >&2; exit 2; }
BUNDLE="$(cd "$(dirname "$BUNDLE")" && pwd)/$(basename "$BUNDLE")"

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

# A relocated bundle, not the one in place: unpack or copy it somewhere else
# entirely, so nothing can resolve back to the source tree by accident.
if [ -d "$BUNDLE" ]; then
    cp -r "$BUNDLE" "$WORK/openepl"
else
    mkdir -p "$WORK/x" && tar xzf "$BUNDLE" -C "$WORK/x"
    mv "$WORK/x"/* "$WORK/openepl"
fi
BIN="$WORK/openepl/bin"

unset OPENEPL_RUNTIME_DIR
mkdir -p "$WORK/project"
cd "$WORK/project"

pass=0
fail=0
check() {
    local name="$1"; shift
    if "$@" >/tmp/openepl_verify.log 2>&1; then
        printf '  %-52s PASS\n' "$name"
        pass=$((pass + 1))
    else
        printf '  %-52s FAIL\n' "$name"
        sed 's/^/      /' /tmp/openepl_verify.log | head -12
        fail=$((fail + 1))
    fi
}

echo "verifying $(basename "$BUNDLE") from $WORK/project"
echo

check "binaries are present and executable" test -x "$BIN/openepl" -a -x "$BIN/openepl-studio"
check "brand assets ship with the bundle" test -f "$WORK/openepl/assets/openepl-wordmark.png" \
    -a -f "$WORK/openepl/assets/openepl-icon.png"
check "licences ship with the bundle" test -f "$WORK/openepl/LICENSE" \
    -a -f "$WORK/openepl/licenses/RmlUi-LICENSE.txt"

# Finds its own runtime with no environment variable — the whole point of the
# layout.
check "openepl locates its runtime unaided" bash -c \
    "'$BIN/openepl' templates | grep -q '^template: console-app'"

check "creates a console project" "$BIN/openepl" new console-app hello
check "builds it" "$BIN/openepl" build hello/main.oir -o hello/app
check "the built program runs and prints" bash -c \
    "cd '$WORK/project' && ./hello/app | grep -q 'Hello from OpenEPL'"

check "creates a GUI project" "$BIN/openepl" new gui-app winapp
check "builds a GUI program (links the vendored UI stack)" \
    "$BIN/openepl" build winapp/main.oir -o winapp/app

check "creates a shared library" "$BIN/openepl" new shared-library slib
check "builds a .so" "$BIN/openepl" build slib/main.oir -o slib/libslib.so
check "the .so exports its subroutines" bash -c \
    "nm -D --defined-only '$WORK/project/slib/libslib.so' | grep -q ' T greet'"

check "creates a static library" "$BIN/openepl" new static-library alib
check "builds a .a" "$BIN/openepl" build alib/main.oir -o alib/libalib.a

# The IDE, headless: it must find `openepl` beside itself and open a project.
check "studio opens a project" env OPENEPL_DESIGNER_SCRIPT='view:code' \
    "$BIN/openepl-studio" "$WORK/project/hello/main.oir"
check "studio's welcome screen creates and opens a project" env \
    OPENEPL_DESIGNER_WELCOME_PICK=console-app OPENEPL_DESIGNER_SCRIPT='view:code' \
    "$BIN/openepl-studio"

# The logo must be found from the unpacked location, not from a build tree.
check "studio finds its assets after relocation" bash -c \
    "OPENEPL_DESIGNER_SCRIPT='view:code' '$BIN/openepl-studio' '$WORK/project/hello/main.oir' 2>&1 \
     | grep -qi 'could not load texture' && exit 1 || exit 0"

# Studio must not litter the directory it was launched from.
check "studio leaves no scratch files behind" bash -c \
    "! test -f '$WORK/project/openepl_dotgrid.tga'"

# The frame length is computed, not written by hand: a wrong Content-Length
# makes the server hang rather than answer, and the test would be reporting its
# own arithmetic error as a product failure.
frame() { printf 'Content-Length: %d\r\n\r\n%s' "${#1}" "$1"; }

lsp_probe() {
    # A full lifecycle, not just initialize: closing stdin without a shutdown
    # makes the server exit non-zero, which `pipefail` would report as a
    # failure of the thing being tested rather than of how it was asked.
    {
        frame '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"capabilities":{}}}'
        frame '{"jsonrpc":"2.0","method":"initialized","params":{}}'
        frame '{"jsonrpc":"2.0","id":2,"method":"shutdown","params":null}'
        frame '{"jsonrpc":"2.0","method":"exit"}'
    } | timeout 20 "$BIN/openepl" lsp 2>/dev/null > "$WORK/lsp.out"
    grep -q capabilities "$WORK/lsp.out"
}
check "the language server answers" lsp_probe

echo
echo "  $pass passed, $fail failed"
[ "$fail" -eq 0 ] || exit 1
