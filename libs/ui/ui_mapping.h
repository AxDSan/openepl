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
    // Same shape as a checkbox: an <input type='radio'> plus its caption.
    if (std::strcmp(type_name, "radiobutton") == 0) return "div";
    if (std::strcmp(type_name, "combobox") == 0) return "select";
    if (std::strcmp(type_name, "memo") == 0) return "textarea";
    if (std::strcmp(type_name, "slider") == 0) return "input";
    if (std::strcmp(type_name, "image") == 0) return "img";
    if (std::strcmp(type_name, "progressbar") == 0) return "progress";
    // A listbox and a spinner are assembled from plain elements: RmlUi has no
    // always-visible list, and no <input> that carries its own step buttons.
    return "div";   // label, groupbox, form, listbox, spinner
}

/// The class an OpenEPL component wears, or nullptr when it needs none.
///
/// Several components share one substrate tag — a checkbox, a radio button, a
/// listbox and a spinner are all `div` — so the class is the only thing telling
/// the stylesheet which is which. It lives here beside the tag rather than in
/// each consumer, because a control that gets its class in the runtime and not
/// in the designer is the WYSIWYG drift this header exists to prevent.
inline const char* class_for(const char* type_name) {
    if (std::strcmp(type_name, "groupbox") == 0)    return "oe-groupbox";
    if (std::strcmp(type_name, "checkbox") == 0)    return "oe-checkbox";
    if (std::strcmp(type_name, "radiobutton") == 0) return "oe-radio";
    if (std::strcmp(type_name, "listbox") == 0)     return "oe-listbox";
    if (std::strcmp(type_name, "spinner") == 0)     return "oe-spinner";
    return nullptr;
}

/// Some components need an attribute at creation for the substrate element to
/// behave correctly (an `<input>` needs its type). Returns nullptr when none.
inline const char* creation_attribute(const char* type_name, const char** value) {
    if (std::strcmp(type_name, "editbox") == 0) { *value = "text"; return "type"; }
    if (std::strcmp(type_name, "slider") == 0) { *value = "range"; return "type"; }
    return nullptr;
}

/// Components built from more than one substrate element. Returns the inner
/// markup to install on creation, or nullptr for simple components.
inline const char* inner_markup(const char* type_name) {
    if (std::strcmp(type_name, "checkbox") == 0)
        return "<input type='checkbox' class='oe-box'/><span class='oe-caption'></span>";
    if (std::strcmp(type_name, "radiobutton") == 0)
        return "<input type='radio' class='oe-box'/><span class='oe-caption'></span>";
    // The two arrows are ordinary buttons, so they light up on hover and take
    // a click through the same path every other button does.
    if (std::strcmp(type_name, "spinner") == 0)
        return "<input type='text' class='oe-value'/>"
               "<button class='oe-step oe-up'>+</button>"
               "<button class='oe-step oe-down'>-</button>";
    return nullptr;
}

/// Whether `checked` / `text` must be routed to a child element.
inline bool is_composite(const char* type_name) {
    return std::strcmp(type_name, "checkbox") == 0 ||
           std::strcmp(type_name, "radiobutton") == 0;
}

/// Properties carried as element ATTRIBUTES rather than RCSS styling.
/// Returns the attribute name, or nullptr if the property is styling.
inline const char* attribute_for(const char* type_name, const char* property) {
    if (std::strcmp(property, "text") == 0 && std::strcmp(type_name, "editbox") == 0)
        return "value";
    if (std::strcmp(property, "checked") == 0) return "checked";
    // RmlUi enforces radio exclusion itself, across the whole document, by the
    // `name` attribute — so the group name IS the mechanism, not a label for
    // one this library would otherwise have to write.
    if (std::strcmp(property, "group") == 0 && std::strcmp(type_name, "radiobutton") == 0)
        return "name";
    // A slider's bounds and position are attributes for the DESIGNER canvas,
    // which has no running control to ask. The runtime never reaches here for
    // them: it goes through the typed element interface first, because an
    // attribute stops being the answer the moment the user drags the handle.
    if (std::strcmp(property, "min") == 0) return "min";
    if (std::strcmp(property, "max") == 0) return "max";
    if (std::strcmp(property, "value") == 0 && std::strcmp(type_name, "slider") == 0)
        return "value";
    if (std::strcmp(property, "source") == 0) return "src";
    if (std::strcmp(property, "value") == 0 && std::strcmp(type_name, "progressbar") == 0)
        return "value";
    return nullptr;
}

/// Whether a component renders its `text` property as element content.
/// Stated as the types that DO, not as the ones that do not: the list of
/// controls whose `text` is something other than their content — a value, a
/// caption on a child, a picture — is now the longer of the two and grows with
/// every control added.
inline bool text_is_content(const char* type_name) {
    return std::strcmp(type_name, "form") == 0 || std::strcmp(type_name, "button") == 0 ||
           std::strcmp(type_name, "label") == 0 || std::strcmp(type_name, "checkbox") == 0 ||
           std::strcmp(type_name, "radiobutton") == 0 ||
           std::strcmp(type_name, "groupbox") == 0 || std::strcmp(type_name, "memo") == 0;
}

/// Whether a property names what a control HOLDS rather than how it looks.
///
/// The runtime answers these from the typed element itself (`control_set` in
/// ui_rmlui.cpp) and never reaches the styling path. A consumer that has no
/// running control — the designer canvas — must skip them rather than fall
/// through, because `value: 3` and `step: 1` are not RCSS: handing them to the
/// stylesheet yields a parse error on stderr and no rendering either way.
inline bool is_control_value(const char* type_name, const char* property) {
    // What the control holds is never something a form declares.
    if (std::strcmp(property, "count") == 0) return true;
    if (std::strcmp(type_name, "combobox") == 0 || std::strcmp(type_name, "listbox") == 0)
        return std::strcmp(property, "items") == 0 || std::strcmp(property, "selected") == 0;
    if (std::strcmp(type_name, "spinner") == 0)
        return std::strcmp(property, "value") == 0 || std::strcmp(property, "step") == 0;
    return false;
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
        p + "img { display: block; position: absolute; }" +

        /* RmlUi builds a <select> out of three named sub-elements and styles
         * none of them: unstyled, the value is invisible, the arrow has no
         * size, and the drop-down opens as a zero-height box. */
        p + "select { display: block; position: absolute; background-color: #ffffff;"
            " border: 1px #d0d7de; border-radius: 4px; color: #1f2328; }" +
        p + "select:focus { border: 1px #1e60d5; }" +
        p + "select selectvalue { width: auto; margin-right: 22px; padding: 4px 8px 0 8px;"
            " height: 100%; }" +
        /* RmlUi's arrow element takes no content, so the affordance has to be the
         * band itself: a shaded, bordered strip reads as a control, an unshaded
         * one reads as a gap in the border. */
        p + "select selectarrow { width: 22px; height: 100%; background-color: #dfe4ea;"
            " border-left: 1px #d0d7de; }" +
        p + "select selectarrow:hover { background-color: #cdd4dd; }" +
        /* The drop-down is drawn OUTSIDE the select's own box, so it needs its
         * own background and border or it renders over whatever is behind it. */
        p + "select selectbox { background-color: #ffffff; border: 1px #d0d7de;"
            " border-radius: 4px; margin-top: 2px; padding: 2px; }" +
        p + "select selectbox option { display: block; padding: 4px 8px 4px 8px;"
            " color: #1f2328; }" +
        p + "select selectbox option:hover { background-color: #dbe6f7; }" +
        p + "select selectbox option:checked { background-color: #1e60d5; color: #ffffff; }" +

        /* A listbox is assembled here rather than by the substrate, so the rows
         * and the selected row are this stylesheet's job entirely. */
        /* `overflow: hidden`, never `auto`: giving RmlUi a scrolling container
         * here leaves the rows inside it with no definite width to resolve
         * against, and every one of them collapses to its padding. Clipping is
         * what the box needs anyway — a list longer than its rectangle must not
         * draw over the rest of the form. */
        p + "div.oe-listbox { background-color: #ffffff; border: 1px #d0d7de;"
            " border-radius: 4px; color: #1f2328; overflow: hidden; }" +
        p + "div.oe-listbox div.oe-item { display: block; position: relative; width: 100%;"
            " box-sizing: border-box; padding: 4px 8px 4px 8px; }" +
        p + "div.oe-listbox div.oe-item:hover { background-color: #dbe6f7; }" +
        p + "div.oe-listbox div.oe-selected { background-color: #1e60d5; color: #ffffff; }" +

        /* Round, so a radio button is distinguishable from a checkbox at a
         * glance — which is the only thing that tells a reader the choice is
         * exclusive. */
        p + "div.oe-radio { background-color: #00000000; }" +
        p + "div.oe-radio input { display: inline-block; position: relative; width: 16px;"
            " height: 16px; background-color: #ffffff; border: 1px #8c959f;"
            " border-radius: 8px; vertical-align: -3px; }" +
        /* A filled dot rather than a ring: a thick inset border would have to be
         * drawn inside the box, and the radius is the only thing distinguishing
         * this from a checkbox — a border wide enough to read as a ring squares
         * the corners off and takes that away. */
        p + "div.oe-radio input:checked { background-color: #1e60d5; border: 1px #1e60d5;"
            " border-radius: 8px; }" +
        p + "div.oe-radio span.oe-caption { display: inline-block; padding-left: 8px; }" +

        p + "textarea { display: block; position: absolute; background-color: #ffffff;"
            " border: 1px #d0d7de; border-radius: 4px; padding: 4px 6px 4px 6px;"
            " color: #1f2328; }" +
        p + "textarea:focus { border: 1px #1e60d5; }" +

        /* A range input is a track plus a bar and nothing else; with no size on
         * either, the control lays out as a zero-pixel line. */
        p + "input.range { display: block; position: absolute; }" +
        /* The bar is positioned by the widget, and for a horizontal slider its
         * vertical place is its own TOP MARGIN (WidgetSlider::PositionBar) — not
         * a flow offset from the track. So the two margins are what line the
         * handle up with the groove, and a negative one throws it clear of the
         * control entirely. */
        p + "input.range slidertrack { width: 100%; height: 6px; margin-top: 7px;"
            " background-color: #d0d7de; border-radius: 3px; }" +
        p + "input.range sliderbar { width: 18px; height: 18px; margin-top: 1px;"
            " background-color: #1e60d5; border-radius: 9px; }" +
        p + "input.range sliderbar:hover { background-color: #3a7ae8; }" +
        p + "input.range sliderarrowdec, input.range sliderarrowinc { width: 0; height: 0; }" +

        p + "div.oe-spinner { background-color: #00000000; }" +
        /* Everything is placed from the LEFT and TOP in percentages: a box
         * sized by opposing `right`/`bottom` edges lays out at its content
         * width here, which for a text box is the whole rest of the window. */
        p + "div.oe-spinner input.oe-value { display: block; position: absolute; left: 0;"
            " top: 0; width: 78%; height: 100%; box-sizing: border-box;"
            " background-color: #ffffff; border: 1px #d0d7de; border-radius: 4px 0 0 4px;"
            " padding: 4px 6px 0 6px; color: #1f2328; }" +
        p + "div.oe-spinner button.oe-step { display: block; position: absolute; left: 78%;"
            " width: 22%; height: 50%; box-sizing: border-box; padding: 0;"
            " background-color: #eaeef2; border: 1px #d0d7de; color: #1f2328;"
            " text-align: center; font-size: 12px; line-height: 12px; }" +
        p + "div.oe-spinner button.oe-step:hover { background-color: #dbe1e8; }" +
        p + "div.oe-spinner button.oe-up { top: 0; border-radius: 0 4px 0 0; }" +
        p + "div.oe-spinner button.oe-down { top: 50%; border-radius: 0 0 4px 0; }";
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
    /// Fixed-width. The code editor needs one: its syntax-highlight layer sits
    /// behind the text and only aligns if every glyph is the same width.
    bool is_mono;
};
inline const FontCandidate* font_candidates(int* count) {
    static const FontCandidate fonts[] = {
        {"/usr/share/fonts/dejavu-sans-fonts/DejaVuSans.ttf", "DejaVu Sans",
         "/usr/share/fonts/dejavu-sans-fonts/DejaVuSans-Bold.ttf",
         "/usr/share/fonts/dejavu-sans-fonts/DejaVuSans-Oblique.ttf",
         "/usr/share/fonts/dejavu-sans-fonts/DejaVuSans-BoldOblique.ttf", false},
        {"/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf", "DejaVu Sans",
         "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf",
         "/usr/share/fonts/truetype/dejavu/DejaVuSans-Oblique.ttf",
         "/usr/share/fonts/truetype/dejavu/DejaVuSans-BoldOblique.ttf", false},
        {"/usr/share/fonts/adwaita-mono-fonts/AdwaitaMono-Regular.ttf", "Adwaita Mono",
         "/usr/share/fonts/adwaita-mono-fonts/AdwaitaMono-Bold.ttf",
         "/usr/share/fonts/adwaita-mono-fonts/AdwaitaMono-Italic.ttf",
         "/usr/share/fonts/adwaita-mono-fonts/AdwaitaMono-BoldItalic.ttf", true},
        {"/usr/share/fonts/liberation-sans-fonts/LiberationSans-Regular.ttf", "Liberation Sans",
         "/usr/share/fonts/liberation-sans-fonts/LiberationSans-Bold.ttf",
         "/usr/share/fonts/liberation-sans-fonts/LiberationSans-Italic.ttf",
         "/usr/share/fonts/liberation-sans-fonts/LiberationSans-BoldItalic.ttf", false},
        {"/usr/share/fonts/liberation-mono-fonts/LiberationMono-Regular.ttf", "Liberation Mono",
         "/usr/share/fonts/liberation-mono-fonts/LiberationMono-Bold.ttf",
         "/usr/share/fonts/liberation-mono-fonts/LiberationMono-Italic.ttf",
         "/usr/share/fonts/liberation-mono-fonts/LiberationMono-BoldItalic.ttf", true},
        {"/usr/share/fonts/dejavu-sans-mono-fonts/DejaVuSansMono.ttf", "DejaVu Sans Mono",
         "/usr/share/fonts/dejavu-sans-mono-fonts/DejaVuSansMono-Bold.ttf",
         "/usr/share/fonts/dejavu-sans-mono-fonts/DejaVuSansMono-Oblique.ttf",
         "/usr/share/fonts/dejavu-sans-mono-fonts/DejaVuSansMono-BoldOblique.ttf", true},
    };
    *count = (int)(sizeof(fonts) / sizeof(fonts[0]));
    return fonts;
}

} // namespace openepl::ui
#endif
