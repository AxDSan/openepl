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
    if (std::strcmp(type_name, "editbox") == 0) return "input";
    // A checkbox is a container holding the box and its caption.
    if (std::strcmp(type_name, "checkbox") == 0) return "div";
    if (std::strcmp(type_name, "image") == 0) return "img";
    if (std::strcmp(type_name, "progressbar") == 0) return "progress";
    return "div";   // label, groupbox, form
}

/// Some components need an attribute at creation for the substrate element to
/// behave correctly (an `<input>` needs its type). Returns nullptr when none.
inline const char* creation_attribute(const char* type_name, const char** value) {
    if (std::strcmp(type_name, "editbox") == 0) { *value = "text"; return "type"; }
    return nullptr;
}

/// Components built from more than one substrate element. Returns the inner
/// markup to install on creation, or nullptr for simple components.
inline const char* inner_markup(const char* type_name) {
    if (std::strcmp(type_name, "checkbox") == 0)
        return "<input type='checkbox' class='oe-box'/><span class='oe-caption'></span>";
    return nullptr;
}

/// Whether `checked` / `text` must be routed to a child element.
inline bool is_composite(const char* type_name) {
    return std::strcmp(type_name, "checkbox") == 0;
}

/// Properties carried as element ATTRIBUTES rather than RCSS styling.
/// Returns the attribute name, or nullptr if the property is styling.
inline const char* attribute_for(const char* type_name, const char* property) {
    if (std::strcmp(property, "text") == 0 && std::strcmp(type_name, "editbox") == 0)
        return "value";
    if (std::strcmp(property, "checked") == 0) return "checked";
    if (std::strcmp(property, "max") == 0) return "max";
    if (std::strcmp(property, "source") == 0) return "src";
    if (std::strcmp(property, "value") == 0 && std::strcmp(type_name, "progressbar") == 0)
        return "value";
    return nullptr;
}

/// Whether a component renders its `text` property as element content.
inline bool text_is_content(const char* type_name) {
    return std::strcmp(type_name, "editbox") != 0 && std::strcmp(type_name, "image") != 0 &&
           std::strcmp(type_name, "progressbar") != 0;
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
/// Default appearance for every component type.
///
/// RmlUi ships form controls with NO default styling: an `<input>` or
/// `<progress>` renders as *nothing* until styled. These rules are what make
/// editbox/checkbox/progressbar real controls rather than invisible ones.
///
/// Shared by the runtime seed AND the designer canvas — with two copies, a
/// control would look different in the designer than in the built app, which is
/// the WYSIWYG drift D9 exists to prevent.
inline std::string control_styles(const std::string& scope = "") {
    // `scope` prefixes every selector so these rules can be confined to a
    // subtree. The app document contains only the form, so it uses no scope;
    // the DESIGNER must scope them to its canvas, or `div{position:absolute}`
    // lands on the whole IDE and every panel collapses onto the same point.
    const std::string p = scope.empty() ? std::string() : scope + " ";
    return
        p + "div { display: block; position: absolute; }" +
        p + "button { display: block; position: absolute; text-align: center; padding-top: 8px; }" +
        p + "input.text { display: block; position: absolute; background-color: #ffffff;"
            " border: 1px #d0d7de; border-radius: 4px; padding: 4px 6px 0 6px; color: #1f2328; }" +
        p + "input.text:focus { border: 1px #1e60d5; }" +
        p + "div.oe-checkbox { background-color: #00000000; }" +
        p + "div.oe-checkbox input { display: inline-block; position: relative; width: 16px;"
            " height: 16px; background-color: #ffffff; border: 1px #8c959f; border-radius: 3px;"
            " vertical-align: -3px; }" +
        p + "div.oe-checkbox input:checked { background-color: #1e60d5; border: 1px #1e60d5; }" +
        p + "div.oe-checkbox span.oe-caption { display: inline-block; padding-left: 8px; }" +
        p + "div.oe-groupbox { border: 1px #d0d7de; border-radius: 6px; padding: 8px; }" +
        p + "progress { display: block; position: absolute; background-color: #e1e4e8;"
            " border-radius: 8px; }" +
        p + "progress fill { background-color: #1e60d5; border-radius: 8px; }" +
        p + "img { display: block; position: absolute; }";
}

/// The seed document every OpenEPL form is built into.
///
/// D21: a document created bare drops decorators silently while `SetProperty`
/// still reports success. Always seed.
inline std::string seed_document(int width, int height, const std::string& font_family) {
    std::string out = "<rml><head><style>";
    out += "body { width: " + std::to_string(width) + "px; height: " + std::to_string(height) +
           "px; font-family: '" + font_family + "'; font-size: 16px; color: #1f2328; }";
    out += control_styles();
    out += "</style></head><body/></rml>";
    return out;
}

/// Font files to try, with the family name each one registers as.
///
/// Two RmlUi facts drive this shape. It has no CSS generic-family fallback, so
/// the stylesheet must name a family that was actually loaded. And it does not
/// synthesise bold or italic: every style is a separate face file, and text
/// styled with a face that was never loaded renders with **no font at all** —
/// silently invisible, not merely unstyled. So each candidate carries its
/// companion faces.
struct FontCandidate {
    const char* path;   ///< regular face; its success selects the family
    const char* family;
    const char* bold;        ///< may be null
    const char* italic;      ///< may be null
    const char* bold_italic; ///< may be null
};
inline const FontCandidate* font_candidates(int* count) {
    static const FontCandidate fonts[] = {
        {"/usr/share/fonts/dejavu-sans-fonts/DejaVuSans.ttf", "DejaVu Sans",
         "/usr/share/fonts/dejavu-sans-fonts/DejaVuSans-Bold.ttf",
         "/usr/share/fonts/dejavu-sans-fonts/DejaVuSans-Oblique.ttf",
         "/usr/share/fonts/dejavu-sans-fonts/DejaVuSans-BoldOblique.ttf"},
        {"/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf", "DejaVu Sans",
         "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf",
         "/usr/share/fonts/truetype/dejavu/DejaVuSans-Oblique.ttf",
         "/usr/share/fonts/truetype/dejavu/DejaVuSans-BoldOblique.ttf"},
        {"/usr/share/fonts/adwaita-mono-fonts/AdwaitaMono-Regular.ttf", "Adwaita Mono",
         "/usr/share/fonts/adwaita-mono-fonts/AdwaitaMono-Bold.ttf",
         "/usr/share/fonts/adwaita-mono-fonts/AdwaitaMono-Italic.ttf",
         "/usr/share/fonts/adwaita-mono-fonts/AdwaitaMono-BoldItalic.ttf"},
        {"/usr/share/fonts/liberation-sans-fonts/LiberationSans-Regular.ttf", "Liberation Sans",
         "/usr/share/fonts/liberation-sans-fonts/LiberationSans-Bold.ttf",
         "/usr/share/fonts/liberation-sans-fonts/LiberationSans-Italic.ttf",
         "/usr/share/fonts/liberation-sans-fonts/LiberationSans-BoldItalic.ttf"},
    };
    *count = (int)(sizeof(fonts) / sizeof(fonts[0]));
    return fonts;
}

} // namespace openepl::ui
#endif
