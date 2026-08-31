/* Generates the designer canvas's dot-grid tile.
 *
 * RmlUi tiles an image decorator with `repeat`, so a 10x10 tile with a single
 * dot gives the spec's dot grid at any canvas size. Written as uncompressed TGA
 * because that needs no image library and SDL_image loads it.
 */
#ifndef OPENEPL_DESIGNER_DOTGRID_H
#define OPENEPL_DESIGNER_DOTGRID_H

#include <cstdio>
#include <sys/stat.h>
#include <string>

namespace openepl::designer {

/// Write a `spacing`x`spacing` BGRA tile with one dot; returns the path.
inline std::string write_dot_tile(const std::string& path, int spacing = 10) {
    FILE* f = std::fopen(path.c_str(), "wb");
    if (!f) return "";
    const unsigned char header[18] = {
        0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        (unsigned char)(spacing & 0xff), (unsigned char)(spacing >> 8),
        (unsigned char)(spacing & 0xff), (unsigned char)(spacing >> 8),
        32, 0x20 /* top-left origin */
    };
    std::fwrite(header, 1, sizeof header, f);
    for (int y = 0; y < spacing; y++) {
        for (int x = 0; x < spacing; x++) {
            // One soft dot per tile, in the border colour (#d0d7de).
            const bool dot = (x == 0 && y == 0);
            const unsigned char px[4] = {0xde, 0xd7, 0xd0, (unsigned char)(dot ? 0xff : 0x00)};
            std::fwrite(px, 1, 4, f);
        }
    }
    std::fclose(f);
    return path;
}

} // namespace openepl::designer
#endif
