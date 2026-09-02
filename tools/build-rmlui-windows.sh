#!/bin/bash
# Cross-build the vendored RmlUi for Windows x86-64 with mingw-w64, into
# vendor/RmlUi/build-windows (gitignored). Run once, after tools/fetch-rmlui.sh
# — this reuses the same pinned checkout and builds it a second time.
#
# What it needs on the build machine (Fedora names):
#   mingw64-gcc-c++ mingw64-sdl2-compat mingw64-SDL2_image mingw64-freetype
# Debian/Ubuntu ship the compiler (g++-mingw-w64-x86-64) but not the SDL2 and
# freetype cross packages; point SYSROOT at a sysroot that has them.
set -eu
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SRC="$ROOT/vendor/RmlUi"
DEST="$SRC/build-windows"
TRIPLE=x86_64-w64-mingw32
SYSROOT="${SYSROOT:-/usr/$TRIPLE/sys-root/mingw}"

if [ -f "$DEST/librmlui.a" ]; then echo "RmlUi (Windows) already built at $DEST"; exit 0; fi
[ -d "$SRC" ] || { echo "run tools/fetch-rmlui.sh first: $SRC is not there" >&2; exit 1; }
command -v "$TRIPLE-g++" >/dev/null || { echo "missing $TRIPLE-g++ (mingw64-gcc-c++ on Fedora)" >&2; exit 1; }
[ -f "$SYSROOT/include/freetype2/ft2build.h" ] || { echo "missing mingw freetype under $SYSROOT (mingw64-freetype on Fedora)" >&2; exit 1; }
[ -f "$SYSROOT/include/SDL2/SDL.h" ] || { echo "missing mingw SDL2 under $SYSROOT (mingw64-sdl2-compat on Fedora)" >&2; exit 1; }

mkdir -p "$DEST"
# A toolchain file of our own rather than a distribution's wrapper script, so
# the same command works wherever the cross compiler and sysroot are.
cat > "$DEST/toolchain.cmake" <<CMAKE
set(CMAKE_SYSTEM_NAME Windows)
set(CMAKE_SYSTEM_PROCESSOR x86_64)
set(CMAKE_C_COMPILER $TRIPLE-gcc)
set(CMAKE_CXX_COMPILER $TRIPLE-g++)
set(CMAKE_RC_COMPILER $TRIPLE-windres)
set(CMAKE_FIND_ROOT_PATH "$SYSROOT")
set(CMAKE_FIND_ROOT_PATH_MODE_PROGRAM NEVER)
set(CMAKE_FIND_ROOT_PATH_MODE_LIBRARY ONLY)
set(CMAKE_FIND_ROOT_PATH_MODE_INCLUDE ONLY)
set(CMAKE_FIND_ROOT_PATH_MODE_PACKAGE ONLY)
CMAKE
cmake -S "$SRC" -B "$DEST" -G Ninja -DCMAKE_BUILD_TYPE=Release \
      -DCMAKE_TOOLCHAIN_FILE="$DEST/toolchain.cmake" \
      -DRMLUI_BACKEND=SDL_GL3 -DRMLUI_FONT_ENGINE=freetype \
      -DRMLUI_SAMPLES=OFF -DBUILD_SHARED_LIBS=OFF
cmake --build "$DEST"
echo "RmlUi (Windows) built at $DEST"
