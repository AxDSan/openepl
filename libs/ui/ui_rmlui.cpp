/* OpenEPL UI backend — RmlUi implementation of the D10 interface (ADR 0005/D14).
 *
 * This is the ONLY file that knows RmlUi exists. Everything above it speaks
 * `abi/openepl_ui.h`, which contains no substrate types — that is what keeps the
 * substrate swappable (D10). Swapping backends means replacing this file.
 *
 * Verified mechanics come from spikes/q9-rmlui/.
 */
#include <RmlUi/Core.h>
#include <RmlUi/Core/ElementInstancer.h>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <string>
#include <unordered_map>
#include <vector>

#include "RmlUi_Backend.h"
#include "RmlUi_Renderer_GL3.h"
#include "RmlUi_Include_GL3.h"
#include "openepl_ui.h"

namespace {

struct UiState {
    Rml::Context* context = nullptr;
    std::unordered_map<uint64_t, bool> interactive;   /* handle -> wants hover states */
    Rml::ElementDocument* document = nullptr;
    std::vector<Rml::Element*> widgets;   // index+1 == OpenEPL_Widget handle
    std::string get_scratch;
    bool initialised = false;
};
UiState g;

Rml::Element* resolve(OpenEPL_Widget w) {
    if (w == 0 || w > g.widgets.size()) return nullptr;
    return g.widgets[(size_t)w - 1];
}

OpenEPL_Widget publish(Rml::Element* e) {
    g.widgets.push_back(e);
    return (OpenEPL_Widget)g.widgets.size();
}

/* Adapts an OpenEPL function-pointer handler to an RmlUi listener. Handlers are
 * bound by pointer, never by name, so no user identifier ships (G8). */
struct HandlerBridge : Rml::EventListener {
    OpenEPL_EventFn fn;
    explicit HandlerBridge(OpenEPL_EventFn f) : fn(f) {}
    void ProcessEvent(Rml::Event&) override { if (fn) fn(); }
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
    Rml::Element* element;
    Rml::String base;      /* colour as authored            */
    Rml::String hover;     /* lightened                     */
    Rml::String active;    /* darkened                      */

    StateStyler(Rml::Element* e, Rml::String b, Rml::String h, Rml::String a)
        : element(e), base(std::move(b)), hover(std::move(h)), active(std::move(a)) {}

    void ProcessEvent(Rml::Event& ev) override {
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

/* Map an OpenEPL component type to the RmlUi tag that backs it. Keeping this
 * mapping here (rather than in the descriptors) means the component vocabulary
 * is ours, not the substrate's. */
const char* tag_for(const char* type_name) {
    if (std::strcmp(type_name, "button") == 0) return "button";
    if (std::strcmp(type_name, "label") == 0)  return "div";
    if (std::strcmp(type_name, "form") == 0)   return "div";
    return "div";
}

/* Properties that are OpenEPL concepts rather than RCSS properties. */
bool is_text_property(const char* p) { return std::strcmp(p, "text") == 0; }

/* OpenEPL property names use underscores (`background_color`) to match the rest
 * of the language and keep the lexer free of hyphen ambiguity; RCSS uses
 * hyphens. Translate at this boundary — the substrate's spelling stops here. */
std::string rcss_name(const char* p) {
    std::string s(p);
    for (char& c : s) if (c == '_') c = '-';
    return s;
}

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
    Rml::Initialise();

    /* Fonts: try a few common paths so the spike works on a bare system. */
    /* RmlUi has no CSS generic-family fallback: the stylesheet must name a
     * family that was actually loaded, so we record the one we got. */
    const char* fonts[][2] = {
        { "/usr/share/fonts/dejavu-sans-fonts/DejaVuSans.ttf", "DejaVu Sans" },
        { "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf", "DejaVu Sans" },
        { "/usr/share/fonts/adwaita-mono-fonts/AdwaitaMono-Regular.ttf", "Adwaita Mono" },
        { "/usr/share/fonts/liberation-sans-fonts/LiberationSans-Regular.ttf", "Liberation Sans" },
    };
    std::string family = "sans-serif";
    for (auto& f : fonts) {
        if (Rml::LoadFontFace(f[0])) { family = f[1]; break; }
    }

    g.context = Rml::CreateContext("main", Rml::Vector2i(width, height));
    if (!g.context) return 1;

    /* D21: forms are ALWAYS instantiated into a stylesheet-seeded document.
     * A bare CreateDocument() silently drops decorators while SetProperty still
     * returns true — the spike's most expensive finding. */
    char seed[1024];
    std::snprintf(seed, sizeof seed,
        "<rml><head><style>"
        "body { width: %dpx; height: %dpx; font-family: '%s'; font-size: 16px; }"
        "button { display: block; position: absolute; text-align: center; padding-top: 8px; }"
        "div { display: block; position: absolute; }"
        "</style></head><body/></rml>", width, height, family.c_str());
    g.document = g.context->LoadDocumentFromMemory(seed);
    if (!g.document) return 1;
    g.document->Show();

    g.widgets.clear();
    publish(g.document);          // handle 1 == the form root
    g.initialised = true;
    return 0;
}

OpenEPL_Widget oe_ui_root(void) { return g.initialised ? 1 : 0; }

OpenEPL_Widget oe_ui_create(OpenEPL_Widget parent, const char* type_name) {
    if (!g.initialised || !type_name) return 0;
    Rml::Element* p = parent ? resolve(parent) : g.document;
    if (!p) return 0;
    Rml::ElementPtr child = g.document->CreateElement(tag_for(type_name));
    if (!child) return 0;
    Rml::Element* raw = p->AppendChild(std::move(child));
    if (!raw) return 0;
    OpenEPL_Widget h = publish(raw);
    /* Buttons get interaction feedback; a control that does not visibly respond
     * to the mouse reads as broken regardless of whether its event fires. */
    if (std::strcmp(type_name, "button") == 0) g.interactive[h] = true;
    return h;
}

int oe_ui_set(OpenEPL_Widget w, const char* property, const char* value) {
    Rml::Element* e = resolve(w);
    if (!e || !property || !value) return 1;

    if (is_text_property(property)) { e->SetInnerRML(value); return 0; }

    /* Bare integers are pixel lengths for geometry properties. */
    std::string v = value;
    bool numeric = !v.empty() && v.find_first_not_of("-0123456789") == std::string::npos;
    if (numeric && (std::strcmp(property, "left") == 0 || std::strcmp(property, "top") == 0 ||
                    std::strcmp(property, "width") == 0 || std::strcmp(property, "height") == 0 ||
                    std::strcmp(property, "border_radius") == 0))
        v += "px";

    if (w == 1 && std::strcmp(property, "title") == 0) return 0;  /* window title: set at init */

    bool ok = e->SetProperty(rcss_name(property), v);

    if (ok && std::strcmp(property, "background_color") == 0 && g.interactive.count(w)) {
        auto* st = new StateStyler(e, v, shade(v, 1.18f), shade(v, 0.82f));
        g_stylers.push_back(st);
        for (const char* ev : {"mouseover", "mouseout", "mousedown", "mouseup"})
            e->AddEventListener(ev, st);
    }
    return ok ? 0 : 1;
}

const char* oe_ui_get(OpenEPL_Widget w, const char* property) {
    Rml::Element* e = resolve(w);
    if (!e || !property) return nullptr;
    if (is_text_property(property)) { g.get_scratch = e->GetInnerRML(); return g.get_scratch.c_str(); }
    const Rml::Property* p = e->GetProperty(rcss_name(property));
    if (!p) return nullptr;
    g.get_scratch = p->ToString();
    return g.get_scratch.c_str();
}

int oe_ui_on(OpenEPL_Widget w, const char* event, OpenEPL_EventFn handler) {
    Rml::Element* e = resolve(w);
    if (!e || !event || !handler) return 1;
    auto* bridge = new HandlerBridge(handler);
    g_bridges.push_back(bridge);
    e->AddEventListener(event, bridge);
    return 0;
}

int oe_ui_set_a11y(OpenEPL_Widget w, int32_t role, const char* name) {
    Rml::Element* e = resolve(w);
    if (!e) return 1;
    /* Recorded as attributes now; the AccessKit bridge (Phase 3) walks the tree
     * and reads these. Capturing them at construction is the point of D16 —
     * the information exists before the bridge does. */
    e->SetAttribute("oe-role", (int)role);
    if (name) e->SetAttribute("oe-a11y-name", Rml::String(name));
    return 0;
}

int oe_ui_run(void) {
    if (!g.initialised) return 1;

    const int max_frames = env_int("OPENEPL_UI_EXIT_AFTER_FRAMES", 0);
    const char* synth_click = std::getenv("OPENEPL_UI_SYNTH_CLICK");
    const char* dump_path = std::getenv("OPENEPL_UI_DUMP");
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

    if (mouse_at) {
        int mx = 0, my = 0;
        if (std::sscanf(mouse_at, "%d,%d", &mx, &my) == 2)
            g.context->ProcessMouseMove(mx, my, 0);
    }

    int frames = 0;
    bool running = true;
    while (running) {
        if (max_frames == 0) running = Backend::ProcessEvents(g.context, nullptr, true);
        g.context->Update();
        Backend::BeginFrame();
        g.context->Render();

        if (max_frames > 0 && ++frames >= max_frames) {
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
            break;
        }
        Backend::PresentFrame();
    }
    return 0;
}

void oe_ui_shutdown(void) {
    if (!g.initialised) return;
    Rml::Shutdown();
    Backend::Shutdown();
    for (auto* b : g_bridges) delete b;
    g_bridges.clear();
    for (auto* s : g_stylers) delete s;
    g_stylers.clear();
    g.interactive.clear();
    g.widgets.clear();
    g.initialised = false;
}

} // extern "C"
