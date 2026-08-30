#!/bin/bash
# Reproduce the Q9 RmlUi spike (ADR 0004 §8). Requires: clang++, cmake, ninja,
# SDL2 + SDL2_image + freetype2 + GL dev packages.
set -eu
HERE="$(cd "$(dirname "$0")" && pwd)"
WORK="${1:-/tmp/openepl-q9-spike}"
mkdir -p "$WORK" && cd "$WORK"

[ -d RmlUi ] || git clone --depth 1 --branch 6.3 https://github.com/mikke89/RmlUi.git
cmake -S RmlUi -B RmlUi/build -G Ninja -DCMAKE_BUILD_TYPE=Release \
      -DRMLUI_BACKEND=SDL_GL3 -DRMLUI_SAMPLES=OFF -DBUILD_SHARED_LIBS=OFF
cmake --build RmlUi/build

build() { # <source> <output> [extra flags...]
  local src="$1" out="$2"; shift 2
  clang++ -std=c++17 -DRMLUI_SDL_VERSION_MAJOR=2 -DSDL_VIDEO_RENDER_OGL=1 "$@" \
    "$HERE/$src" \
    RmlUi/Backends/RmlUi_Backend_SDL_GL3.cpp RmlUi/Backends/RmlUi_Platform_SDL.cpp \
    RmlUi/Backends/RmlUi_Renderer_GL3.cpp \
    -I RmlUi/Include -I RmlUi/Backends $(pkg-config --cflags sdl2 SDL2_image freetype2) \
    -L RmlUi/build -lrmlui $(pkg-config --libs sdl2 SDL2_image) -lGL -ldl -lfreetype -o "$out"
}

build spike.cpp spike -O2          # steps 1+2: effects + component model
build paint.cpp paint -O2          # isolates the decorator/stylesheet finding
build sheet.cpp sheet -O2          # confirms the stylesheet fix
build diag.cpp  diag  -O2          # which properties parse
build hello.cpp hello_rmlui -Os -flto -ffunction-sections -fdata-sections -Wl,--gc-sections -s

echo "--- running spike ---"; ./spike
echo "--- hello-world size ---"; ls -l hello_rmlui | awk '{printf "%d KiB\n", $5/1024}'
