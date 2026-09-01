/* OpenEPL UI backend — RmlUi implementation of the D10 interface.
 *
 * This is the ONLY file that knows RmlUi exists. Everything above it speaks
 * `abi/openepl_ui.h`, which contains no substrate types — that is what keeps the
 * substrate swappable (D10). Swapping backends means replacing this file.
 *
 */
#include <RmlUi/Core.h>
#include <RmlUi/Core/ElementInstancer.h>
#include <RmlUi/Core/FileInterface.h>
#include <RmlUi/Core/Input.h>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <string>
#include <unordered_map>
#include <unordered_set>
#include <utility>
#include <vector>

#include <SDL.h>

#include "RmlUi_Backend.h"
#include "RmlUi_Renderer_GL3.h"
#include "RmlUi_Include_GL3.h"
#include "openepl_abi.h"   /* oe_malloc — runtime-owned allocation (D4) */
#include "a11y_bridge.h"
#include "ui_mapping.h"
#include "a11y_model.h"
#include "openepl_ui.h"

namespace {

struct UiState {
    Rml::Context* context = nullptr;
    std::unordered_map<uint64_t, bool> interactive;   /* handle -> wants hover states */
    std::unordered_map<uint64_t, uint64_t> parent_of;  /* handle -> parent handle */
    std::unordered_map<uint64_t, std::string> type_of;  /* handle -> component type */
    Rml::ElementDocument* document = nullptr;
    std::vector<Rml::Element*> widgets;   // index+1 == OpenEPL_Widget handle
    std::string get_scratch;
    bool initialised = false;
};
UiState g;

/* Frames drawn since oe_ui_run, for OPENEPL_UI_EXIT_AFTER_FRAMES. */
int g_frames = 0;

Rml::Element* resolve(OpenEPL_Widget w) {
    if (w == 0 || w > g.widgets.size()) return nullptr;
    return g.widgets[(size_t)w - 1];
}

OpenEPL_Widget publish(Rml::Element* e) {
    g.widgets.push_back(e);
    return (OpenEPL_Widget)g.widgets.size();
}

/* --- actions ----------------------------------------------------------
 *
 * An action is one command: its caption, its shortcut, whether it can be
 * invoked at all, and the subroutine behind it, held in a single place. Every
 * control that offers that command follows it, so disabling the action greys
 * the button and renaming it renames the button.
 *
 * Controls name the action they point at rather than referring to it: a
 * property value is a literal (backend/src/lib.rs) and a component identifier
 * deliberately never reaches the binary, so a name is the only channel there
 * is.
 *
 * Action handles share the widget numbering because the compiler numbers every
 * component of a library from ONE counter, in creation order — a module-level
 * action declared after a form's children is handle N+2, and that is the number
 * `save.enabled` in a subroutine passes back. Reserving a widget slot for each
 * action is what keeps the two sequences the same sequence.
 */
struct Action {
    std::string     name;
    std::string     text;
    std::string     shortcut;
    bool            enabled = true;
    OpenEPL_EventFn on_execute = nullptr;
};
std::unordered_map<uint64_t, Action>   g_actions;
std::unordered_map<uint64_t, uint64_t> g_action_of;   /* widget -> action handle */

/* A form is built before the module-level components it points at, so a
 * control almost always names an action that does not exist yet. Unresolved
 * bindings wait here and are claimed when an action takes that name. */
std::vector<std::pair<uint64_t, std::string>> g_pending_bindings;

/* Disabled is an OpenEPL concept, not an RCSS property, and RmlUi's `disabled`
 * attribute means nothing on a plain <button> element: the events still fire
 * and the hover shading still lights up. So the state is held here and every
 * listener asks — a control that answers the mouse while disabled is disabled
 * only in the source. */
std::unordered_set<uint64_t> g_disabled;

bool widget_disabled(OpenEPL_Widget w) { return g_disabled.count(w) > 0; }

void set_widget_enabled(OpenEPL_Widget w, bool on) {
    if (on) g_disabled.erase(w);
    else    g_disabled.insert(w);
    if (Rml::Element* e = resolve(w)) e->SetProperty("opacity", on ? "1.0" : "0.4");
}

/* Adapts an OpenEPL function-pointer handler to an RmlUi listener. Handlers are
 * bound by pointer, never by name, so no user identifier ships (G8). */
struct HandlerBridge : Rml::EventListener {
    OpenEPL_EventFn fn;
    OpenEPL_Widget  widget;
    HandlerBridge(OpenEPL_EventFn f, OpenEPL_Widget w) : fn(f), widget(w) {}
    void ProcessEvent(Rml::Event&) override {
        if (fn && !widget_disabled(widget)) fn();
    }
};
std::vector<HandlerBridge*> g_bridges;   // owned; freed at shutdown

/* Interactive visual states.
 *
 * Component properties are applied as INLINE properties, which outrank any
 * `:hover`/`:active` stylesheet rule — so a stylesheet cannot provide hover
 * feedback here. The backend therefore drives the states itself: it remembers
 * the base colour and swaps in derived shades on mouse over/down. Keeping this
 * in the backend also means every substrate gets to implement interaction the
 * way its own system prefers. */
struct StateStyler : Rml::EventListener {
    Rml::Element*  element;
    OpenEPL_Widget widget;
    Rml::String base;      /* colour as authored            */
    Rml::String hover;     /* lightened                     */
    Rml::String active;    /* darkened                      */

    StateStyler(Rml::Element* e, OpenEPL_Widget w, Rml::String b, Rml::String h, Rml::String a)
        : element(e), widget(w), base(std::move(b)), hover(std::move(h)), active(std::move(a)) {}

    void ProcessEvent(Rml::Event& ev) override {
        if (widget_disabled(widget)) return;
        const Rml::String& t = ev.GetType();
        if (t == "mouseover")      element->SetProperty("background-color", hover);
        else if (t == "mouseout")  element->SetProperty("background-color", base);
        else if (t == "mousedown") element->SetProperty("background-color", active);
        else if (t == "mouseup")   element->SetProperty("background-color", hover);
    }
};
std::vector<StateStyler*> g_stylers;   /* owned; freed at shutdown */

/* Scale an #rgb/#rrggbb colour by `factor`, clamped. */
Rml::String shade(const Rml::String& css, float factor) {
    if (css.size() < 4 || css[0] != '#') return css;
    Rml::String hex = css.substr(1);
    if (hex.size() == 3) hex = Rml::String{hex[0],hex[0],hex[1],hex[1],hex[2],hex[2]};
    if (hex.size() < 6) return css;
    char out[8];
    int v[3];
    for (int i = 0; i < 3; i++) {
        int c = (int)std::strtol(hex.substr((size_t)i * 2, 2).c_str(), nullptr, 16);
        c = (int)(c * factor);
        v[i] = c < 0 ? 0 : (c > 255 ? 255 : c);
    }
    std::snprintf(out, sizeof out, "#%02x%02x%02x", v[0], v[1], v[2]);
    return Rml::String(out);
}

/* Invokes an action on behalf of one of the controls that offer it. The action
 * decides, not the control: the same click does nothing while the action is
 * disabled, whichever button it arrived from. */
struct ActionBridge : Rml::EventListener {
    uint64_t action;
    explicit ActionBridge(uint64_t h) : action(h) {}
    void ProcessEvent(Rml::Event&) override {
        auto it = g_actions.find(action);
        if (it == g_actions.end() || !it->second.enabled || !it->second.on_execute) return;
        it->second.on_execute();
    }
};
std::vector<ActionBridge*> g_action_bridges;   /* owned; freed at shutdown */

void apply_action(uint64_t h);

uint64_t action_named(const std::string& name) {
    if (name.empty()) return 0;
    for (const auto& kv : g_actions) {
        if (kv.second.name == name) return kv.first;
    }
    return 0;
}

/* Point a control at an action: it takes the caption and the enabled state now,
 * and follows every later change to them. */
void bind_widget(OpenEPL_Widget w, uint64_t h) {
    g_action_of[w] = h;
    auto* b = new ActionBridge(h);
    g_action_bridges.push_back(b);
    if (Rml::Element* e = resolve(w)) e->AddEventListener("click", b);
    apply_action(h);
}

/* Push an action's state out to every control that offers it. An empty caption
 * leaves the control's own text alone, so a toolbar button showing an icon is
 * not blanked by an action that only carries a shortcut and a handler. */
void apply_action(uint64_t h) {
    auto it = g_actions.find(h);
    if (it == g_actions.end()) return;
    for (const auto& kv : g_action_of) {
        if (kv.second != h) continue;
        const OpenEPL_Widget w = (OpenEPL_Widget)kv.first;
        if (!it->second.text.empty()) oe_ui_set(w, "text", it->second.text.c_str());
        set_widget_enabled(w, it->second.enabled);
    }
}

/* An action has taken a name; adopt the controls that were waiting for it. */
void claim_pending(uint64_t h, const std::string& name) {
    if (name.empty()) return;
    for (size_t i = 0; i < g_pending_bindings.size();) {
        if (g_pending_bindings[i].second == name) {
            const OpenEPL_Widget w = (OpenEPL_Widget)g_pending_bindings[i].first;
            g_pending_bindings.erase(g_pending_bindings.begin() + (long)i);
            bind_widget(w, h);
        } else {
            i++;
        }
    }
}

/* --- shortcuts --------------------------------------------------------- */

/* `ctrl+shift+s`, `f5`, `escape` — spelled the way a menu prints it, lowercased
 * so the source can spell it either way. */
int key_for_name(const std::string& k) {
    using namespace Rml::Input;
    if (k.size() == 1) {
        const char c = k[0];
        if (c >= 'a' && c <= 'z') return KI_A + (c - 'a');
        if (c >= '0' && c <= '9') return KI_0 + (c - '0');
    }
    if (k.size() >= 2 && k[0] == 'f') {
        const int n = std::atoi(k.c_str() + 1);
        if (n >= 1 && n <= 12) return KI_F1 + (n - 1);
    }
    if (k == "escape" || k == "esc")   return KI_ESCAPE;
    if (k == "enter"  || k == "return") return KI_RETURN;
    if (k == "space")                  return KI_SPACE;
    if (k == "tab")                    return KI_TAB;
    if (k == "backspace")              return KI_BACK;
    if (k == "delete" || k == "del")   return KI_DELETE;
    if (k == "insert")                 return KI_INSERT;
    if (k == "home")                   return KI_HOME;
    if (k == "end")                    return KI_END;
    if (k == "up")                     return KI_UP;
    if (k == "down")                   return KI_DOWN;
    if (k == "left")                   return KI_LEFT;
    if (k == "right")                  return KI_RIGHT;
    return KI_UNKNOWN;
}

/* A shortcut matches only when the modifiers match exactly: ctrl+s must not
 * fire on ctrl+shift+s, which is routinely a different command. */
bool shortcut_matches(const std::string& spec, int key, bool ctrl, bool shift, bool alt) {
    std::string want_key;
    bool want_ctrl = false, want_shift = false, want_alt = false;
    std::string part;
    for (size_t i = 0; i <= spec.size(); i++) {
        if (i < spec.size() && spec[i] != '+') {
            part += (char)tolower((unsigned char)spec[i]);
            continue;
        }
        if (part == "ctrl" || part == "control")     want_ctrl = true;
        else if (part == "shift")                    want_shift = true;
        else if (part == "alt")                      want_alt = true;
        else if (!part.empty())                      want_key = part;
        part.clear();
    }
    if (want_ctrl != ctrl || want_shift != shift || want_alt != alt) return false;
    const int want = key_for_name(want_key);
    return want != Rml::Input::KI_UNKNOWN && want == key;
}

/* Shortcuts are matched on the DOCUMENT, not on the control that offers the
 * action: one that only fired while its button had focus would not be a
 * shortcut. */
struct ShortcutListener : Rml::EventListener {
    void ProcessEvent(Rml::Event& ev) override {
        const int  key   = ev.GetParameter<int>("key_identifier", 0);
        const bool ctrl  = ev.GetParameter<int>("ctrl_key", 0) != 0;
        const bool shift = ev.GetParameter<int>("shift_key", 0) != 0;
        const bool alt   = ev.GetParameter<int>("alt_key", 0) != 0;
        for (const auto& kv : g_actions) {
            const Action& a = kv.second;
            if (a.shortcut.empty() || !a.enabled || !a.on_execute) continue;
            if (!shortcut_matches(a.shortcut, key, ctrl, shift, alt)) continue;
            a.on_execute();
            ev.StopPropagation();
            return;
        }
    }
};
ShortcutListener g_shortcuts;

/* --- embedded resources ------------------------------------------------ */

/* Pictures a form names are compiled INTO the program (cli/src/main.rs), so a
 * built binary carries them instead of a promise about a path on the machine it
 * was built on. The table is weak: a program that names none defines neither
 * the symbol nor an empty stand-in, and a null table is the empty one. */
extern "C" {
struct OpenEPL_Resource {
    const char*          name;
    const unsigned char* data;
    long long            size;
};
/* Terminated by a null `name` rather than carrying a count, because a weak
 * COUNT would have to be read through a null pointer to discover it is absent. */
__attribute__((weak)) extern const OpenEPL_Resource oe_embedded_resources[];
}

std::string file_name_of(const std::string& path) {
    const size_t slash = path.find_last_of("/\\");
    return slash == std::string::npos ? path : path.substr(slash + 1);
}

const OpenEPL_Resource* find_resource(const std::string& path) {
    if (!oe_embedded_resources) return nullptr;
    for (const OpenEPL_Resource* r = oe_embedded_resources; r->name; r++) {
        if (path == r->name) return r;
    }
    /* The document is built from memory, so it has no URL for RmlUi to resolve
     * a source against and what arrives here may be joined, normalised, or
     * neither. The file name is what identifies a resource in every case. */
    const std::string base = file_name_of(path);
    for (const OpenEPL_Resource* r = oe_embedded_resources; r->name; r++) {
        if (base == file_name_of(r->name)) return r;
    }
    return nullptr;
}

/* Serves embedded resources, and everything else off the disk.
 *
 * The fallback is not optional: fonts are loaded through this same interface,
 * and an interface that only knew about resources would leave every OpenEPL
 * program with no text in it. */
class EmbeddedFiles : public Rml::FileInterface {
    struct Slot {
        const unsigned char* data = nullptr;
        size_t               size = 0;
        size_t               pos  = 0;
        FILE*                fp   = nullptr;
    };

public:
    Rml::FileHandle Open(const Rml::String& path) override {
        if (const OpenEPL_Resource* r = find_resource(path)) {
            auto* s = new Slot{r->data, (size_t)r->size, 0, nullptr};
            return (Rml::FileHandle)s;
        }
        FILE* fp = std::fopen(path.c_str(), "rb");
        if (!fp) return (Rml::FileHandle)0;
        auto* s = new Slot{nullptr, 0, 0, fp};
        return (Rml::FileHandle)s;
    }

    void Close(Rml::FileHandle file) override {
        auto* s = (Slot*)file;
        if (!s) return;
        if (s->fp) std::fclose(s->fp);
        delete s;
    }

    size_t Read(void* buffer, size_t size, Rml::FileHandle file) override {
        auto* s = (Slot*)file;
        if (!s) return 0;
        if (s->fp) return std::fread(buffer, 1, size, s->fp);
        const size_t left = s->pos < s->size ? s->size - s->pos : 0;
        const size_t n = size < left ? size : left;
        std::memcpy(buffer, s->data + s->pos, n);
        s->pos += n;
        return n;
    }

    bool Seek(Rml::FileHandle file, long offset, int origin) override {
        auto* s = (Slot*)file;
        if (!s) return false;
        if (s->fp) return std::fseek(s->fp, offset, origin) == 0;
        long base = 0;
        if (origin == SEEK_CUR)      base = (long)s->pos;
        else if (origin == SEEK_END) base = (long)s->size;
        const long want = base + offset;
        if (want < 0 || (size_t)want > s->size) return false;
        s->pos = (size_t)want;
        return true;
    }

    size_t Tell(Rml::FileHandle file) override {
        auto* s = (Slot*)file;
        if (!s) return 0;
        if (s->fp) return (size_t)std::ftell(s->fp);
        return s->pos;
    }
};
EmbeddedFiles g_files;

/* Properties that are OpenEPL concepts rather than RCSS properties. */

int env_int(const char* name, int fallback) {
    const char* v = std::getenv(name);
    return v ? std::atoi(v) : fallback;
}

} // namespace

extern "C" {

int oe_ui_init(const char* title, int width, int height) {
    if (g.initialised) return 0;
    if (!Backend::Initialize(title ? title : "OpenEPL", width, height, true)) return 1;
    Rml::SetSystemInterface(Backend::GetSystemInterface());
    Rml::SetRenderInterface(Backend::GetRenderInterface());
    /* Before Initialise, which installs the stdio interface itself if nothing
     * else has claimed the slot — and after that a resource would be looked for
     * on disk, which is the whole thing embedding exists to stop. */
    Rml::SetFileInterface(&g_files);
    Rml::Initialise();

    /* Fonts: try a few common paths so the spike works on a bare system. */
    /* Font list and family names come from the shared mapping, so the designer
     * and the built app resolve the same font. */
    int font_count = 0;
    const auto* fonts = openepl::ui::font_candidates(&font_count);
    std::string family = "sans-serif";
    for (int i = 0; i < font_count; i++) {
        if (!Rml::LoadFontFace(fonts[i].path)) continue;
        family = fonts[i].family;
        // Load the companion styles too. RmlUi does not synthesise bold or
        // italic: text asking for a face that was never loaded renders with no
        // font at all, i.e. invisibly. Missing companions are not fatal —
        // that text just falls back to regular.
        for (const char* extra : {fonts[i].bold, fonts[i].italic, fonts[i].bold_italic}) {
            if (extra) Rml::LoadFontFace(extra);
        }
        break;
    }

    g.context = Rml::CreateContext("main", Rml::Vector2i(width, height));
    if (!g.context) return 1;

    /* D21: forms are ALWAYS instantiated into a stylesheet-seeded document.
     * A bare CreateDocument() silently drops decorators while SetProperty still
     * returns true — the spike's most expensive finding. */
    const std::string seed = openepl::ui::seed_document(width, height, family);
    g.document = g.context->LoadDocumentFromMemory(seed);
    if (!g.document) return 1;
    g.document->Show();

    g.widgets.clear();
    publish(g.document);          // handle 1 == the form root
    g.document->AddEventListener("keydown", &g_shortcuts);
    g.initialised = true;
    return 0;
}

OpenEPL_Widget oe_ui_root(void) { return g.initialised ? 1 : 0; }

OpenEPL_Widget oe_ui_create(OpenEPL_Widget parent, const char* type_name) {
    if (!g.initialised || !type_name) return 0;
    Rml::Element* p = parent ? resolve(parent) : g.document;
    if (!p) return 0;
    Rml::ElementPtr child = g.document->CreateElement(openepl::ui::tag_for(type_name));
    if (!child) return 0;
    // Some substrate elements need an attribute at creation (an <input> needs
    // its type before it will behave as a text box or a checkbox).
    const char* attr_value = nullptr;
    if (const char* attr = openepl::ui::creation_attribute(type_name, &attr_value)) {
        child->SetAttribute(attr, Rml::String(attr_value));
    }
    Rml::Element* raw = p->AppendChild(std::move(child));
    if (!raw) return 0;
    OpenEPL_Widget h = publish(raw);
    /* Buttons get interaction feedback; a control that does not visibly respond
     * to the mouse reads as broken regardless of whether its event fires. */
    if (std::strcmp(type_name, "button") == 0) g.interactive[h] = true;
    g.parent_of[h] = parent ? parent : 1;
    g.type_of[h] = type_name;
    if (std::strcmp(type_name, "groupbox") == 0) raw->SetAttribute("class", Rml::String("oe-groupbox"));
    if (std::strcmp(type_name, "checkbox") == 0) raw->SetAttribute("class", Rml::String("oe-checkbox"));
    if (const char* markup = openepl::ui::inner_markup(type_name)) raw->SetInnerRML(markup);
    // RmlUi's <progress> defaults to max=1, which makes any percentage look
    // full; OpenEPL's `value` is a percentage.
    if (std::strcmp(type_name, "progressbar") == 0) raw->SetAttribute("max", Rml::String("100"));
    return h;
}

int oe_ui_set(OpenEPL_Widget w, const char* property, const char* value) {
    Rml::Element* e = resolve(w);
    if (!e || !property || !value) return 1;

    /* Pointing a control at an action, by the action's name. The form is built
     * before the module-level components, so the action named here usually does
     * not exist yet and the binding waits for it. */
    if (std::strcmp(property, "action") == 0) {
        if (const uint64_t h = action_named(value)) bind_widget(w, h);
        else if (*value) g_pending_bindings.emplace_back((uint64_t)w, value);
        return 0;
    }
    if (std::strcmp(property, "enabled") == 0) {
        set_widget_enabled(w, std::strcmp(value, "true") == 0 || std::strcmp(value, "1") == 0);
        return 0;
    }

    const std::string type = g.type_of.count(w) ? g.type_of[w] : std::string("form");

    // Attribute-backed properties (an editbox's value, a checkbox's checked
    // state, an image's source) are not RCSS styling.
    if (const char* attr = openepl::ui::attribute_for(type.c_str(), property)) {
        // Composite components route to the child that actually carries it.
        Rml::Element* target = e;
        if (openepl::ui::is_composite(type.c_str()) && std::strcmp(property, "checked") == 0) {
            if (Rml::Element* box = e->GetElementById("")) target = box;
            for (int i = 0; i < e->GetNumChildren(); i++) {
                if (e->GetChild(i)->GetTagName() == "input") { target = e->GetChild(i); break; }
            }
        }
        e = target;
        if (std::strcmp(property, "checked") == 0) {
            const bool on = std::strcmp(value, "true") == 0 || std::strcmp(value, "1") == 0;
            if (on) e->SetAttribute(attr, Rml::String("checked"));
            else e->RemoveAttribute(attr);
        } else {
            e->SetAttribute(attr, Rml::String(value));
        }
        return 0;
    }

    if (openepl::ui::is_text_property(property) && openepl::ui::text_is_content(type.c_str())) {
        if (openepl::ui::is_composite(type.c_str())) {
            for (int i = 0; i < e->GetNumChildren(); i++) {
                if (e->GetChild(i)->GetTagName() == "span") {
                    e->GetChild(i)->SetInnerRML(value);
                    return 0;
                }
            }
        }
        e->SetInnerRML(value);
        /* Keep the accessible name in step: a screen reader must announce the
         * current text, not whatever it was at construction. */
        openepl::a11y::set_label(w, value);
        return 0;
    }

    const std::string v = openepl::ui::rcss_value(property, value);

    if (w == 1 && std::strcmp(property, "title") == 0) return 0;  /* window title: set at init */

    bool ok = e->SetProperty(openepl::ui::rcss_name(property), v);

    if (ok && std::strcmp(property, "background_color") == 0 && g.interactive.count(w)) {
        auto* st = new StateStyler(e, w, v, shade(v, 1.18f), shade(v, 0.82f));
        g_stylers.push_back(st);
        for (const char* ev : {"mouseover", "mouseout", "mousedown", "mouseup"})
            e->AddEventListener(ev, st);
    }
    return ok ? 0 : 1;
}

const char* oe_ui_get(OpenEPL_Widget w, const char* property) {
    Rml::Element* e = resolve(w);
    if (!e || !property) return nullptr;

    if (std::strcmp(property, "enabled") == 0) {
        const char* lit = widget_disabled(w) ? "false" : "true";
        char* out = (char*)oe_malloc((long)std::strlen(lit) + 1);
        if (out) std::strcpy(out, lit);
        return out;
    }

    const std::string type = g.type_of.count(w) ? g.type_of[w] : std::string("form");

    Rml::String value;
    if (const char* attr = openepl::ui::attribute_for(type.c_str(), property)) {
        // Attribute-backed properties must be READ from the attribute too, or
        // an editbox reports its markup instead of what the user typed.
        value = e->GetAttribute<Rml::String>(attr, "");
    } else if (openepl::ui::is_text_property(property) &&
               openepl::ui::text_is_content(type.c_str())) {
        value = e->GetInnerRML();
    } else {
        const Rml::Property* p = e->GetProperty(openepl::ui::rcss_name(property));
        if (!p) return nullptr;
        value = p->ToString();
    }

    /* Return a runtime-owned copy rather than a shared scratch buffer, so two
     * reads in one expression — concat(a.text, b.text) — cannot alias. Freed
     * with all other runtime data at shutdown (D4). */
    char* out = (char*)oe_malloc((long)value.size() + 1);
    if (!out) return nullptr;
    std::memcpy(out, value.c_str(), value.size() + 1);
    return out;
}

int32_t oe_ui_get_int(OpenEPL_Widget w, const char* property) {
    /* A truth value does not read as a number: `strtol("true")` is 0, which is
     * the wrong answer rather than a missing one. */
    if (std::strcmp(property, "enabled") == 0) return widget_disabled(w) ? 0 : 1;
    const char* text = oe_ui_get(w, property);
    return text ? (int32_t)std::strtol(text, nullptr, 10) : 0;
}

int oe_ui_on(OpenEPL_Widget w, const char* event, OpenEPL_EventFn handler) {
    Rml::Element* e = resolve(w);
    if (!e || !event || !handler) return 1;
    auto* bridge = new HandlerBridge(handler, w);
    g_bridges.push_back(bridge);
    e->AddEventListener(event, bridge);
    return 0;
}

/* --- the library's non-visual components (abi/openepl_abi.h) ------------
 *
 * `action` is the only one, and it is addressed through these rather than
 * through the widget interface because it has no rectangle. The five entry
 * points are the same five `timer` implements in runtime/oe_component.c.
 */
int64_t oe_ui_component_create(const char* type_name) {
    if (!type_name || std::strcmp(type_name, "action") != 0) {
        oe_error_set(OE_ERR_INVALID_ARG, "ui declares no such non-visual component");
        return 0;
    }
    /* A widget slot is reserved for the action so the two kinds of component
     * keep ONE numbering, which is what the compiler assumed when it handed a
     * subroutine the handle to write `save.enabled` through. */
    const OpenEPL_Widget h = publish(nullptr);
    g_actions[h] = Action{};
    oe_error_clear();
    return (int64_t)h;
}

int32_t oe_ui_component_set(int64_t h, const char* prop, const char* value) {
    auto it = g_actions.find((uint64_t)h);
    if (it == g_actions.end() || !prop || !value) return 1;
    Action& a = it->second;
    if (std::strcmp(prop, "name") == 0) {
        a.name = value;
        claim_pending((uint64_t)h, a.name);
        return 0;
    }
    if (std::strcmp(prop, "text") == 0)          a.text = value;
    else if (std::strcmp(prop, "shortcut") == 0) a.shortcut = value;
    else if (std::strcmp(prop, "enabled") == 0)
        a.enabled = std::strcmp(value, "true") == 0 || std::strcmp(value, "1") == 0;
    else return 1;
    apply_action((uint64_t)h);
    return 0;
}

const char* oe_ui_component_get(int64_t h, const char* prop) {
    auto it = g_actions.find((uint64_t)h);
    if (it == g_actions.end() || !prop) return nullptr;
    const Action& a = it->second;
    std::string v;
    if (std::strcmp(prop, "name") == 0)          v = a.name;
    else if (std::strcmp(prop, "text") == 0)     v = a.text;
    else if (std::strcmp(prop, "shortcut") == 0) v = a.shortcut;
    else if (std::strcmp(prop, "enabled") == 0)  v = a.enabled ? "true" : "false";
    else return nullptr;
    /* Runtime-owned, like every other text result, so a caller may hold it. */
    char* out = (char*)oe_malloc((long)v.size() + 1);
    if (out) std::memcpy(out, v.c_str(), v.size() + 1);
    return out;
}

int32_t oe_ui_component_get_int(int64_t h, const char* prop) {
    auto it = g_actions.find((uint64_t)h);
    if (it == g_actions.end() || !prop) return 0;
    if (std::strcmp(prop, "enabled") == 0) return it->second.enabled ? 1 : 0;
    return 0;
}

int32_t oe_ui_component_on(int64_t h, const char* event, OpenEPL_EventFn handler) {
    auto it = g_actions.find((uint64_t)h);
    if (it == g_actions.end() || !event || !handler) return 1;
    if (std::strcmp(event, "execute") != 0) return 1;
    it->second.on_execute = handler;
    return 0;
}

int oe_ui_set_a11y(OpenEPL_Widget w, int32_t role, const char* name) {
    Rml::Element* e = resolve(w);
    if (!e) return 1;
    /* Publish into the substrate-free model the AccessKit bridge serves. The
     * model is the thread boundary: adapter callbacks read it, never widgets. */
    openepl::a11y::Node n;
    n.id = w;
    n.parent = (w == 1) ? 0 : (g.parent_of.count(w) ? g.parent_of[w] : 1);
    n.role = role;
    n.label = name ? name : "";
    n.clickable = g.interactive.count(w) > 0;
    openepl::a11y::put_node(n);
    return 0;
}

/* One turn of the window: pump input, lay out, publish accessibility, draw.
 *
 * This is a runtime event SOURCE, not a loop of its own. The loop belongs to
 * the runtime (abi/openepl_abi.h), so a window and a timer can be alive in the
 * same program — which they cannot be when whichever library is linked owns the
 * only loop there is. Registered with a period of 0: a window wants every turn
 * it can get, and deliberately does not power-save, or an app whose window is
 * not focused stops updating and its timers and animation stop with it.
 */
static int32_t ui_pump(void *) {
    const int max_frames = env_int("OPENEPL_UI_EXIT_AFTER_FRAMES", 0);
    const char* dump_path = std::getenv("OPENEPL_UI_DUMP");

    if (max_frames == 0 && !Backend::ProcessEvents(g.context, nullptr, false)) {
        /* The window closed. That ends the PROGRAM, not just this source: a
         * windowless process still running its timers is not what closing a
         * window has ever meant. */
        oe_loop_quit(0);
        return 1;
    }
    g.context->Update();

    /* Refresh accessible bounds from the laid-out widgets, then publish.
     * Cheap: update_if_active does nothing until an AT connects. */
    for (size_t i = 0; i < g.widgets.size(); i++) {
        if (Rml::Element* e = g.widgets[i]) {
            auto off = e->GetAbsoluteOffset();
            auto size = e->GetBox().GetSize();
            openepl::a11y::set_bounds((uint64_t)i + 1, off.x, off.y, size.x, size.y);
        }
    }
    openepl::a11y::bridge_publish();

    /* An assistive technology may have asked to activate a control. The
     * request arrived on the adapter thread and was queued; dispatch it
     * here, on the main thread, where touching widgets is safe. */
    for (uint64_t id : openepl::a11y::take_actions()) {
        if (Rml::Element* target = resolve(id)) {
            if (std::getenv("OPENEPL_A11Y_TRACE"))
                std::fprintf(stderr, "openepl-a11y: dispatching click to widget %llu\n",
                             (unsigned long long)id);
            openepl::a11y::set_focus(id);
            target->DispatchEvent("click", Rml::Dictionary());
        }
    }

    Backend::BeginFrame();
    g.context->Render();

    if (max_frames > 0 && ++g_frames >= max_frames) {
        /* Test hook: print the accessibility tree. Substrate-independent
         * and needs no accessibility bus, so it works in CI. */
        if (const char* d = std::getenv("OPENEPL_UI_DUMP_A11Y")) {
            if (*d && d[0] != '0') {
                for (const auto& n : openepl::a11y::snapshot()) {
                    std::printf("a11y: id=%llu parent=%llu role=%d bounds=%.0f,%.0f,%.0fx%.0f name=\"%s\"%s\n",
                                (unsigned long long)n.id, (unsigned long long)n.parent,
                                n.role, n.x, n.y, n.w, n.h, n.label.c_str(),
                                n.clickable ? " clickable" : "");
                }
                std::printf("a11y: adapter_active=%d\n", (int)openepl::a11y::bridge_active());
            }
        }
        auto* gl3 = static_cast<RenderInterface_GL3*>(Backend::GetRenderInterface());
        gl3->EndFrame();
        if (dump_path) {
            int W = g.context->GetDimensions().x, H = g.context->GetDimensions().y;
            std::vector<unsigned char> px((size_t)W * H * 3);
            glReadPixels(0, 0, W, H, GL_RGB, GL_UNSIGNED_BYTE, px.data());
            if (FILE* f = std::fopen(dump_path, "wb")) {
                std::fprintf(f, "P6\n%d %d\n255\n", W, H);
                for (int y = H - 1; y >= 0; y--) std::fwrite(&px[(size_t)y * W * 3], 1, (size_t)W * 3, f);
                std::fclose(f);
            }
        }
        oe_loop_quit(0);
        return 1;
    }
    Backend::PresentFrame();
    return 0;
}

int oe_ui_run(void) {
    if (!g.initialised) return 1;

    const char* synth_click = std::getenv("OPENEPL_UI_SYNTH_CLICK");
    const char* mouse_at = std::getenv("OPENEPL_UI_MOUSE");   /* "x,y" — drives hover */

    if (synth_click) {
        /* Targets a widget HANDLE, not an element id — component ids are
         * compile-time only and never reach the binary (G8), so the test hook
         * must not depend on them. */
        g.context->Update();
        OpenEPL_Widget target = (OpenEPL_Widget)std::strtoull(synth_click, nullptr, 10);
        if (Rml::Element* e = resolve(target))
            e->DispatchEvent("click", Rml::Dictionary());
        else
            std::fprintf(stderr, "openepl-ui: no widget handle %s to click\n", synth_click);
    }

    /* A shortcut cannot be verified by looking at a frame, so there is a hook
     * to press one — dispatched as a real keydown, through the same listener a
     * keyboard reaches. */
    if (const char* key = std::getenv("OPENEPL_UI_SYNTH_KEY")) {
        std::string spec;
        for (const char* c = key; *c; c++) spec += (char)tolower((unsigned char)*c);
        Rml::Dictionary p;
        std::string name;
        for (size_t i = 0; i <= spec.size(); i++) {
            if (i < spec.size() && spec[i] != '+') { name += spec[i]; continue; }
            if (name == "ctrl" || name == "control") p["ctrl_key"] = Rml::Variant(1);
            else if (name == "shift")                p["shift_key"] = Rml::Variant(1);
            else if (name == "alt")                  p["alt_key"] = Rml::Variant(1);
            else if (!name.empty())                  p["key_identifier"] = Rml::Variant(key_for_name(name));
            name.clear();
        }
        g.document->DispatchEvent("keydown", p);
    }

    if (mouse_at) {
        int mx = 0, my = 0;
        if (std::sscanf(mouse_at, "%d,%d", &mx, &my) == 2)
            g.context->ProcessMouseMove(mx, my, 0);
    }

    openepl::a11y::bridge_init();
    {
        /* Report the window's screen position so ATs can map node bounds to
         * screen coordinates. Returns 0,0 under Wayland, where a client cannot
         * know its own position — accessible bounds stay window-relative there. */
        int wx = 0, wy = 0, ww = 0, wh = 0;
        /* The backend owns the SDL window privately; recover it from the
         * current GL context rather than reaching into backend internals. */
        if (SDL_Window* win = SDL_GL_GetCurrentWindow()) {
            SDL_GetWindowPosition(win, &wx, &wy);
            SDL_GetWindowSize(win, &ww, &wh);
        }
        openepl::a11y::bridge_set_window_bounds((float)wx, (float)wy, (float)ww, (float)wh);
    }

    g_frames = 0;
    if (!oe_loop_add(ui_pump, nullptr, 0)) return 1;
    return oe_loop_run();
}

void oe_ui_shutdown(void) {
    if (!g.initialised) return;
    openepl::a11y::bridge_shutdown();
    openepl::a11y::clear();
    Rml::Shutdown();
    Backend::Shutdown();
    for (auto* b : g_bridges) delete b;
    g_bridges.clear();
    for (auto* s : g_stylers) delete s;
    g_stylers.clear();
    for (auto* a : g_action_bridges) delete a;
    g_action_bridges.clear();
    g_actions.clear();
    g_action_of.clear();
    g_pending_bindings.clear();
    g_disabled.clear();
    g.interactive.clear();
    g.widgets.clear();
    g.type_of.clear();
    g.parent_of.clear();
    g.initialised = false;
}

} // extern "C"
