#!/bin/bash
# Build a relocatable OpenEPL bundle for Windows x86-64, cross-built from Linux.
#
#   tools/package-windows.sh            -> dist/openepl-<version>-windows-x86_64{,.zip}
#   tools/package-windows.sh --no-zip   -> leave the tree, skip the archive
#
# The Linux counterpart is tools/package.sh, and the tree has the same shape
# for the same reason: `openepl` finds its runtime by walking up from its own
# executable, so `bin\openepl.exe` resolving to `<root>\runtime` is what makes
# the tree relocatable.
#
# What goes in, and where it comes from:
#   bin/openepl-studio.exe   designer/build-windows.sh — mingw-w64 g++, the
#                            Windows RmlUi, and the sysroot's SDL2 / SDL2_image
#                            / freetype as DLLs, all copied beside it
#   bin/openepl.exe          cargo, for the x86_64-pc-windows-gnu target, IF that
#                            build succeeds. It is attempted and reported, never
#                            required: the bundle is still a bundle without it,
#                            and the README says what is missing.
#   assets/fonts/            DejaVu Sans and DejaVu Sans Mono, with their
#                            licence: Studio's text on a machine with no font
#                            it knows would otherwise render invisibly.
#
# What it needs (Fedora names): mingw64-gcc-c++ mingw64-sdl2-compat
# mingw64-SDL2_image mingw64-freetype, tools/build-rmlui-windows.sh run, and
# `rustup target add x86_64-pc-windows-gnu` for the compiler.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"
COMMIT="$(git rev-parse --short HEAD 2>/dev/null || echo unknown)"
TRIPLE="windows-x86_64"
NAME="openepl-${VERSION}-${TRIPLE}"
OUT="$ROOT/dist/$NAME"
MINGW=x86_64-w64-mingw32

echo "==> OpenEPL $VERSION -> dist/$NAME"

# --- prerequisites --------------------------------------------------------
for dep in "$MINGW-g++" "$MINGW-gcc" "$MINGW-pkg-config" "$MINGW-objdump" cargo; do
    command -v "$dep" >/dev/null || { echo "missing required tool: $dep" >&2; exit 1; }
done
if [ ! -f vendor/RmlUi/build-windows/librmlui.a ]; then
    echo "RmlUi is not built for Windows — run tools/build-rmlui-windows.sh" >&2
    exit 1
fi
"$MINGW-pkg-config" --exists sdl2 SDL2_image freetype2 || {
    echo "the mingw SDL2, SDL2_image and freetype packages are missing" >&2
    exit 1
}

# --- build ----------------------------------------------------------------
echo "==> studio (Windows)"
STUDIO_DIR="$ROOT/designer/windows"
OUT_DIR="$STUDIO_DIR" ./designer/build-windows.sh >/dev/null

echo "==> compiler (Windows)"
mkdir -p "$ROOT/dist"
CLI=""
if rustup target list --installed 2>/dev/null | grep -qx x86_64-pc-windows-gnu; then
    if cargo build --release --quiet --target x86_64-pc-windows-gnu -p openepl-cli 2>"$ROOT/dist/.cli-windows.log" \
        && [ -f target/x86_64-pc-windows-gnu/release/openepl.exe ]; then
        CLI="target/x86_64-pc-windows-gnu/release/openepl.exe"
    else
        echo "    the compiler did not build for Windows; the bundle ships Studio without it" >&2
        echo "    (its errors are in dist/.cli-windows.log)" >&2
    fi
else
    echo "    the Rust target x86_64-pc-windows-gnu is not installed (rustup target add x86_64-pc-windows-gnu);" >&2
    echo "    the bundle ships Studio without the compiler" >&2
fi

# --- assemble -------------------------------------------------------------
# A clean dist: every earlier version's tree and archives for THIS platform
# go, so dist/ holds only what this build produced — no stale bundle a
# release upload or a `ls dist` could pick up by mistake.
rm -rf "$ROOT"/dist/openepl-*-windows-x86_64 "$ROOT"/dist/openepl-*-windows-x86_64.tar.gz \
       "$ROOT"/dist/openepl-*-windows-x86_64.zip "$ROOT"/dist/openepl-*-windows-x86_64.*.sha256 \
       "$ROOT"/dist/openepl-*-windows-x86_64.tar.gz.sha256 "$ROOT"/dist/openepl-*-windows-x86_64.zip.sha256
mkdir -p "$OUT"/{bin,licenses}

install -m755 "$STUDIO_DIR/openepl-studio.exe" "$OUT/bin/openepl-studio.exe"
for dll in "$STUDIO_DIR"/*.dll; do
    install -m644 "$dll" "$OUT/bin/$(basename "$dll")"
done
"$MINGW-strip" "$OUT/bin/openepl-studio.exe" 2>/dev/null || true
if [ -n "$CLI" ]; then
    install -m755 "$CLI" "$OUT/bin/openepl.exe"
    "$MINGW-strip" "$OUT/bin/openepl.exe" 2>/dev/null || true
    # Same check as the Linux bundle: the binary must agree with the archive's
    # name about what version it is. Under wine when wine is here.
    if command -v wine >/dev/null; then
        REPORTED="$(WINEDEBUG=-all WINEDLLOVERRIDES="winex11.drv,winewayland.drv=d" \
            env -u DISPLAY -u WAYLAND_DISPLAY wine "$OUT/bin/openepl.exe" version 2>/dev/null | tr -d '\r' | sed -n 's/^openepl //p')"
        if [ "$REPORTED" != "$VERSION" ]; then
            echo "bin/openepl.exe reports '$REPORTED' but Cargo.toml says $VERSION" >&2
            exit 1
        fi
    fi
fi

# The runtime, ABI and support libraries ship as SOURCE, as on Linux: the
# compiler on the Windows machine builds them into each program.
for d in runtime abi libs kits templates examples editors assets; do
    cp -r "$d" "$OUT/$d"
done

# Fonts. DejaVu is what Studio renders with on Linux; on Windows it is looked
# for beside Studio first (designer/main.cpp, windows_font_candidates), so a
# machine with none of the faces Studio knows still draws its text.
FONTS=""
for d in /usr/share/fonts/dejavu-sans-fonts /usr/share/fonts/dejavu-sans-mono-fonts /usr/share/fonts/truetype/dejavu; do
    [ -d "$d" ] && FONTS="$FONTS $d"
done
if [ -n "$FONTS" ]; then
    mkdir -p "$OUT/assets/fonts"
    for d in $FONTS; do
        for f in DejaVuSans.ttf DejaVuSans-Bold.ttf DejaVuSans-Oblique.ttf DejaVuSans-BoldOblique.ttf \
                 DejaVuSansMono.ttf DejaVuSansMono-Bold.ttf DejaVuSansMono-Oblique.ttf DejaVuSansMono-BoldOblique.ttf; do
            [ -f "$d/$f" ] && cp "$d/$f" "$OUT/assets/fonts/$f"
        done
    done
    for lic in /usr/share/licenses/dejavu-fonts-all/LICENSE /usr/share/licenses/dejavu-sans-fonts/LICENSE \
               /usr/share/fonts/dejavu-sans-fonts/LICENSE /usr/share/doc/fonts-dejavu-core/copyright; do
        if [ -f "$lic" ]; then cp "$lic" "$OUT/licenses/DejaVu-LICENSE.txt"; break; fi
    done
    [ -f "$OUT/assets/fonts/DejaVuSans.ttf" ] || echo "    no DejaVu Sans found; Studio will fall back to the system's Segoe UI" >&2
else
    echo "    DejaVu fonts are not installed here; Studio will fall back to the system's Segoe UI" >&2
fi

mkdir -p "$OUT/docs"
cp docs/editors.md "$OUT/docs/editors.md"

# GUI programs built ON the Windows machine link the same vendored UI stack the
# cross build does: the Windows RmlUi archive and its headers.
mkdir -p "$OUT/vendor/RmlUi/build-windows"
cp -r vendor/RmlUi/Include "$OUT/vendor/RmlUi/Include"
cp -r vendor/RmlUi/Backends "$OUT/vendor/RmlUi/Backends"
cp vendor/RmlUi/build-windows/librmlui.a "$OUT/vendor/RmlUi/build-windows/"

cp LICENSE "$OUT/LICENSE"
cp THIRD-PARTY.md "$OUT/THIRD-PARTY.md"
cp vendor/RmlUi/LICENSE.txt "$OUT/licenses/RmlUi-LICENSE.txt"
# SDL2 (sdl2-compat over SDL3), SDL2_image, freetype and what they pull in ship
# as DLLs; their notices travel with them, from the sysroot's packages.
SYSROOT="/usr/$MINGW/sys-root/mingw"
for pair in "share/licenses/mingw64-sdl2-compat:SDL2" "share/licenses/mingw64-SDL3:SDL3" \
            "share/licenses/mingw64-SDL2_image:SDL2_image" "share/licenses/mingw64-freetype:freetype"; do
    src="/usr/${pair%%:*}"; name="${pair##*:}"
    if [ -d "$src" ]; then
        for f in "$src"/*; do cp "$f" "$OUT/licenses/$name-$(basename "$f")"; done
    fi
done

find "$OUT/examples" "$OUT/libs" "$OUT/kits" \( -name '*.o' -o -name '*.so' -o -name '*.ll' -o -name '*.obj' \) -print0 2>/dev/null \
    | xargs -0 -r rm -f

sed -e "s/__VERSION__/$VERSION/g" tools/bundle-README.md > "$OUT/README.md"
printf '%s\ncommit %s\n' "$VERSION" "$COMMIT" > "$OUT/VERSION"
if [ -z "$CLI" ]; then
    printf '\nThis bundle has no bin\\openepl.exe: the compiler did not cross-build when it was made.\n' >> "$OUT/README.md"
fi

# --- archive --------------------------------------------------------------
if [ "${1:-}" != "--no-zip" ]; then
    echo "==> archive"
    rm -f "$ROOT/dist/$NAME.zip"
    if command -v zip >/dev/null; then
        ( cd "$ROOT/dist" && zip -qr "$NAME.zip" "$NAME" )
    else
        ( cd "$ROOT/dist" && python3 -c "import shutil,sys; shutil.make_archive(sys.argv[1], 'zip', '.', sys.argv[1])" "$NAME" )
    fi
    ( cd "$ROOT/dist" && sha256sum "$NAME.zip" > "$NAME.zip.sha256" )
fi

echo
echo "bundle:  dist/$NAME"
[ -n "$CLI" ] && echo "         with bin/openepl.exe" || echo "         WITHOUT bin/openepl.exe (see above)"
[ -f "$ROOT/dist/$NAME.zip" ] && \
    echo "archive: dist/$NAME.zip ($(du -h "$ROOT/dist/$NAME.zip" | cut -f1))"
