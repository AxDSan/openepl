#!/bin/bash
# Build the OpenEPL designer. No CMake: this mirrors libs/ui/lib.json's flags.
#   designer/build.sh          -> build designer/openepl-designer
#   designer/build.sh test     -> build and run the headless model tests
set -eu
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

if [ ! -f vendor/RmlUi/build/librmlui.a ]; then
  echo "RmlUi is not vendored — run tools/fetch-rmlui.sh" >&2
  exit 1
fi

FLAGS=(-std=c++17 -O1 -Wall -Wextra -Wformat=2 -DRMLUI_SDL_VERSION_MAJOR=2 -DSDL_VIDEO_RENDER_OGL=1
       -I abi -I libs/ui -I designer -I vendor/RmlUi/Include -I vendor/RmlUi/Backends)
read -r -a PKG_CFLAGS <<< "$(pkg-config --cflags sdl2 SDL2_image freetype2)"
read -r -a PKG_LIBS <<< "$(pkg-config --libs sdl2 SDL2_image freetype2)"
LIBS=(-Lvendor/RmlUi/build -lrmlui -lGL -ldl "${PKG_LIBS[@]}")

if [ "${1:-}" = "test" ]; then
  clang++ -std=c++17 -O1 -I designer designer/model.cpp designer/test_model.cpp -o /tmp/openepl_designer_test
  exec /tmp/openepl_designer_test ./target/debug/openepl
fi

clang++ "${FLAGS[@]}" "${PKG_CFLAGS[@]}" \
  designer/main.cpp designer/model.cpp libs/ui/ui_libinfo.c \
  vendor/RmlUi/Backends/RmlUi_Backend_SDL_GL3.cpp \
  vendor/RmlUi/Backends/RmlUi_Platform_SDL.cpp \
  vendor/RmlUi/Backends/RmlUi_Renderer_GL3.cpp \
  "${LIBS[@]}" -o designer/openepl-designer
echo "built designer/openepl-designer"
