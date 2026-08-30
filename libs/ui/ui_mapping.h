/* Shared OpenEPL-component → RmlUi mapping.
 *
 * BOTH the runtime backend (`ui_rmlui.cpp`) and the designer canvas include
 * this. If either had its own copy, a component would render one way in the
 * designer and another in the built app — silent WYSIWYG drift, which is exactly
 * what D9 ("one component model, shared by designer and runtime") exists to
 * prevent. Designer-only styling layers on top of the seed below; it never
 * replaces it.
 *
 * Header-only: this is a handful of pure functions, and duplicating a build
 * rule to share them would be worse than the inlining.
 */
#ifndef OPENEPL_UI_MAPPING_H
#define OPENEPL_UI_MAPPING_H

#include <cstdio>
#include <cstring>
#include <string>

namespace openepl::ui {

/// The RmlUi tag backing an OpenEPL component type. The component vocabulary is
/// ours; this is the only place it meets the substrate's.
inline const char* tag_for(const char* type_name) {
    if (std::strcmp(type_name, "button") == 0) return "button";
    if (std::strcmp(type_name, "label") == 0) return "div";
    if (std::strcmp(type_name, "form") == 0) return "div";
    return "div";
}

/// OpenEPL property names use underscores (`background_color`) to match the rest
/// of the language and keep the lexer free of hyphen ambiguity; RCSS uses
/// hyphens. The substrate's spelling stops here.
inline std::string rcss_name(const char* property) {
    std::string s(property);
    for (char& c : s) {
        if (c == '_') c = '-';
    }
    return s;
}

/// Geometry properties are bare numbers in OpenEPL and pixel lengths in RCSS.
inline bool is_length_property(const char* p) {
    return std::strcmp(p, "left") == 0 || std::strcmp(p, "top") == 0 ||
           std::strcmp(p, "width") == 0 || std::strcmp(p, "height") == 0 ||
           std::strcmp(p, "border_radius") == 0;
}

/// `text` is an OpenEPL concept (element content), not an RCSS property.
inline bool is_text_property(const char* p) { return std::strcmp(p, "text") == 0; }

/// Convert an OpenEPL property value to what RCSS expects.
inline std::string rcss_value(const char* property, const char* value) {
    std::string v(value ? value : "");
    const bool numeric = !v.empty() && v.find_first_not_of("-0123456789") == std::string::npos;
    if (numeric && is_length_property(property)) v += "px";
    return v;
}

/// The seed stylesheet every OpenEPL document is built into.
///
/// D21: a document created bare drops decorators silently while `SetProperty`
/// still reports success. Always seed.
inline std::string seed_document(int width, int height, const std::string& font_family) {
    char buf[1024];
    std::snprintf(buf, sizeof buf,
        "<rml><head><style>"
        "body { width: %dpx; height: %dpx; font-family: '%s'; font-size: 16px; }"
        "button { display: block; position: absolute; text-align: center; padding-top: 8px; }"
        "div { display: block; position: absolute; }"
        "</style></head><body/></rml>",
        width, height, font_family.c_str());
    return std::string(buf);
}

/// Font files to try, with the family name each one registers as. RmlUi has no
/// CSS generic-family fallback, so the stylesheet must name a family that was
/// actually loaded.
struct FontCandidate {
    const char* path;
    const char* family;
};
inline const FontCandidate* font_candidates(int* count) {
    static const FontCandidate fonts[] = {
        {"/usr/share/fonts/dejavu-sans-fonts/DejaVuSans.ttf", "DejaVu Sans"},
        {"/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf", "DejaVu Sans"},
        {"/usr/share/fonts/adwaita-mono-fonts/AdwaitaMono-Regular.ttf", "Adwaita Mono"},
        {"/usr/share/fonts/liberation-sans-fonts/LiberationSans-Regular.ttf", "Liberation Sans"},
    };
    *count = (int)(sizeof(fonts) / sizeof(fonts[0]));
    return fonts;
}

} // namespace openepl::ui
#endif
