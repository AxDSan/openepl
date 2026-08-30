/* The OpenEPL designer — Phase 3 RAD vertical slice (PRD §7, M0).
 *
 * Dogfoods RmlUi (ADR 0005/D18): the chrome and the canvas are the same
 * substrate an OpenEPL app runs on, so what you draw is what you get (D9). The
 * canvas builds components through the SHARED mapping (libs/ui/ui_mapping.h),
 * never its own copy.
 *
 * It never parses .oir: `openepl inspect` is the only reader, and saving splices
 * the regenerated form over the original lines so hand-written code survives
 * (ADR 0011).
 */
#include <RmlUi/Core.h>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <string>
#include <vector>

#include "RmlUi_Backend.h"
#include "RmlUi_Renderer_GL3.h"
#include "RmlUi_Include_GL3.h"
#include "descriptors.h"
#include "model.h"
#include "ui_mapping.h"

using namespace openepl::designer;

namespace {

constexpr int WIN_W = 1100, WIN_H = 720;
constexpr int CANVAS_X = 240, CANVAS_Y = 48;   // canvas origin within the window

struct Designer {
    Rml::Context* context = nullptr;
    Rml::ElementDocument* doc = nullptr;
    Model model;
    std::string openepl_bin = "./target/debug/openepl";
    std::string selected;                      // component id, empty = none
    std::vector<std::string> pending_subs;     // handler stubs to append on save
    std::string status;

    // drag state
    bool dragging = false;
    int drag_dx = 0, drag_dy = 0;
};
Designer g;

Rml::Element* canvas() { return g.doc->GetElementById("canvas"); }

std::string esc(const std::string& s) {
    std::string o;
    for (char c : s) {
        if (c == '<') o += "&lt;";
        else if (c == '>') o += "&gt;";
        else if (c == '&') o += "&amp;";
        else o += c;
    }
    return o;
}

/* --- canvas: build the live component tree from the model ---------------- */

void rebuild_canvas() {
    Rml::Element* c = canvas();
    if (!c) return;
    while (c->GetNumChildren() > 0) c->RemoveChild(c->GetChild(0));

    // The form's own background, so the canvas looks like the running app.
    if (const std::string* bg = g.model.form.property("background_color")) {
        c->SetProperty("background-color", *bg);
    }
    if (const std::string* w = g.model.form.property("width")) {
        c->SetProperty("width", *w + "px");
    }
    if (const std::string* h = g.model.form.property("height")) {
        c->SetProperty("height", *h + "px");
    }

    for (const auto& comp : g.model.children) {
        Rml::ElementPtr child = g.doc->CreateElement(openepl::ui::tag_for(comp.type_name.c_str()));
        Rml::Element* e = c->AppendChild(std::move(child));
        e->SetId(comp.id);
        e->SetAttribute("oe-id", comp.id);
        for (const auto& p : comp.properties) {
            if (openepl::ui::is_text_property(p.first.c_str())) {
                e->SetInnerRML(esc(p.second));
            } else {
                e->SetProperty(openepl::ui::rcss_name(p.first.c_str()),
                               openepl::ui::rcss_value(p.first.c_str(), p.second.c_str()));
            }
        }
        // Designer-only affordance, layered ON TOP of the shared mapping.
        if (comp.id == g.selected) {
            e->SetProperty("border-width", "2px");
            e->SetProperty("border-color", "#ffcc00");
        }
    }
}

/* --- properties inspector, driven by the component descriptors ----------- */

void rebuild_inspector() {
    Rml::Element* panel = g.doc->GetElementById("inspector");
    if (!panel) return;
    if (g.selected.empty()) {
        panel->SetInnerRML("<p class='hint'>Select a component on the canvas.</p>");
        return;
    }
    Component* comp = g.model.find(g.selected);
    if (!comp) { panel->SetInnerRML("<p class='hint'>—</p>"); return; }
    const OpenEPL_ComponentDesc* desc = describe(comp->type_name.c_str());
    if (!desc) { panel->SetInnerRML("<p class='hint'>unknown type</p>"); return; }

    std::string html = "<h2>" + esc(comp->id) + "</h2><p class='type'>" +
                       esc(comp->type_name) + "</p>";
    for (int i = 0; i < desc->property_count; i++) {
        const char* name = desc->properties[i].name;
        const std::string* v = comp->property(name);
        const std::string value = v ? *v : (desc->properties[i].default_value
                                                ? desc->properties[i].default_value
                                                : "");
        html += "<div class='row'><label>" + std::string(name) + "</label>";
        html += "<input type='text' class='pv' name='" + std::string(name) + "' value='" +
                esc(value) + "'/></div>";
    }
    for (int i = 0; i < desc->event_count; i++) {
        const char* ev = desc->events[i].name;
        const std::string* h = comp->handler(ev);
        html += "<div class='row'><label>on " + std::string(ev) + "</label>";
        html += "<input type='text' class='ev' name='" + std::string(ev) + "' value='" +
                esc(h ? *h : "") + "'/></div>";
    }
    panel->SetInnerRML(html);
}

void set_status(const std::string& s) {
    g.status = s;
    if (Rml::Element* e = g.doc->GetElementById("status")) e->SetInnerRML(esc(s));
    std::printf("designer: %s\n", s.c_str());
    std::fflush(stdout);
}

void select(const std::string& id) {
    g.selected = id;
    rebuild_canvas();
    rebuild_inspector();
}

/* --- actions -------------------------------------------------------------- */

void add_component(const std::string& type_name) {
    const OpenEPL_ComponentDesc* desc = describe(type_name.c_str());
    if (!desc) { set_status("unknown component type " + type_name); return; }
    Component c;
    c.id = g.model.fresh_id(type_name);
    c.type_name = type_name;
    for (int i = 0; i < desc->property_count; i++) {
        if (desc->properties[i].default_value) {
            c.set_property(desc->properties[i].name, desc->properties[i].default_value);
        }
    }
    // Stagger new components so they do not stack invisibly.
    c.set_property("left", std::to_string(20 + 12 * (int)g.model.children.size()));
    c.set_property("top", std::to_string(20 + 34 * (int)g.model.children.size()));
    g.model.children.push_back(c);
    set_status("added " + c.id);
    select(c.id);
}

void save() {
    std::string err;
    if (!save_model(g.model, g.pending_subs, err)) { set_status("save failed: " + err); return; }
    g.pending_subs.clear();
    set_status("saved " + g.model.path);
}

void run_app() {
    save();
    const std::string cmd = g.openepl_bin + " run " + g.model.path + " &";
    set_status("running…");
    if (std::system(cmd.c_str()) != 0) set_status("run failed");
}

/* --- event handling ------------------------------------------------------- */

struct Listener : Rml::EventListener {
    void ProcessEvent(Rml::Event& ev) override {
        Rml::Element* el = ev.GetTargetElement();
        const Rml::String type = ev.GetType();

        if (type == "click") {
            // Toolbox / toolbar buttons carry an action attribute.
            for (Rml::Element* e = el; e; e = e->GetParentNode()) {
                if (e->HasAttribute("oe-add")) {
                    add_component(e->GetAttribute<Rml::String>("oe-add", ""));
                    return;
                }
                if (e->HasAttribute("oe-action")) {
                    const Rml::String a = e->GetAttribute<Rml::String>("oe-action", "");
                    if (a == "save") save();
                    else if (a == "run") run_app();
                    return;
                }
                if (e->HasAttribute("oe-id")) {   // a component on the canvas
                    select(e->GetAttribute<Rml::String>("oe-id", ""));
                    return;
                }
            }
            return;
        }

        if (type == "change") {   // an inspector field committed
            if (g.selected.empty()) return;
            Component* comp = g.model.find(g.selected);
            if (!comp) return;
            const Rml::String name = el->GetAttribute<Rml::String>("name", "");
            const Rml::String value = el->GetAttribute<Rml::String>("value", "");
            const Rml::String cls = el->GetAttribute<Rml::String>("class", "");
            if (name.empty()) return;
            if (cls.find("ev") != Rml::String::npos) {
                comp->set_handler(name, value);
                if (!value.empty() && !g.model.has_sub(value)) {
                    g.pending_subs.push_back(value);   // stub written on save
                    set_status("will create sub " + value);
                }
            } else {
                comp->set_property(name, value);
                rebuild_canvas();
            }
            return;
        }

        // Drag-to-move on the canvas.
        if (type == "mousedown" && el->HasAttribute("oe-id")) {
            select(el->GetAttribute<Rml::String>("oe-id", ""));
            g.dragging = true;
            const auto off = el->GetAbsoluteOffset();
            g.drag_dx = ev.GetParameter<int>("mouse_x", 0) - (int)off.x;
            g.drag_dy = ev.GetParameter<int>("mouse_y", 0) - (int)off.y;
        } else if (type == "mousemove" && g.dragging && !g.selected.empty()) {
            Component* comp = g.model.find(g.selected);
            if (!comp) return;
            const int x = ev.GetParameter<int>("mouse_x", 0) - g.drag_dx - CANVAS_X;
            const int y = ev.GetParameter<int>("mouse_y", 0) - g.drag_dy - CANVAS_Y;
            comp->set_property("left", std::to_string(x < 0 ? 0 : x));
            comp->set_property("top", std::to_string(y < 0 ? 0 : y));
            rebuild_canvas();
        } else if (type == "mouseup") {
            if (g.dragging) rebuild_inspector();
            g.dragging = false;
        }
    }
};
Listener g_listener;

/* --- scripted sessions, for headless end-to-end testing ------------------- */

void run_script(const char* script) {
    std::string s(script);
    size_t i = 0;
    while (i <= s.size()) {
        size_t semi = s.find(';', i);
        std::string cmd = s.substr(i, semi == std::string::npos ? std::string::npos : semi - i);
        if (!cmd.empty()) {
            const size_t colon = cmd.find(':');
            const std::string verb = cmd.substr(0, colon);
            const std::string arg = colon == std::string::npos ? "" : cmd.substr(colon + 1);
            if (verb == "add") add_component(arg);
            else if (verb == "select") select(arg);
            else if (verb == "set") {
                // set:<property>=<value>  (on the current selection)
                const size_t eq = arg.find('=');
                if (eq != std::string::npos && !g.selected.empty()) {
                    if (Component* c = g.model.find(g.selected)) {
                        c->set_property(arg.substr(0, eq), arg.substr(eq + 1));
                        rebuild_canvas();
                        rebuild_inspector();
                    }
                }
            } else if (verb == "wire") {
                // wire:<event>=<sub>
                const size_t eq = arg.find('=');
                if (eq != std::string::npos && !g.selected.empty()) {
                    if (Component* c = g.model.find(g.selected)) {
                        const std::string ev = arg.substr(0, eq), sub = arg.substr(eq + 1);
                        c->set_handler(ev, sub);
                        if (!g.model.has_sub(sub)) g.pending_subs.push_back(sub);
                    }
                }
            } else if (verb == "save") save();
        }
        if (semi == std::string::npos) break;
        i = semi + 1;
    }
}

} // namespace

int main(int argc, char** argv) {
    if (argc < 2) {
        std::fprintf(stderr,
                     "usage: openepl-designer <project.oir> [path/to/openepl]\n"
                     "\nEnvironment:\n"
                     "  OPENEPL_DESIGNER_SCRIPT   run a scripted session headlessly, then exit\n");
        return 2;
    }
    const std::string path = argv[1];
    if (argc > 2) g.openepl_bin = argv[2];

    std::string err;
    if (!load_model(g.openepl_bin, path, g.model, err)) {
        std::fprintf(stderr, "designer: cannot load %s\n%s\n", path.c_str(), err.c_str());
        return 1;
    }

    if (!Backend::Initialize("OpenEPL Designer", WIN_W, WIN_H, true)) return 1;
    Rml::SetSystemInterface(Backend::GetSystemInterface());
    Rml::SetRenderInterface(Backend::GetRenderInterface());
    Rml::Initialise();

    int font_count = 0;
    const auto* fonts = openepl::ui::font_candidates(&font_count);
    std::string family = "sans-serif";
    for (int i = 0; i < font_count; i++) {
        if (Rml::LoadFontFace(fonts[i].path)) { family = fonts[i].family; break; }
    }

    g.context = Rml::CreateContext("designer", Rml::Vector2i(WIN_W, WIN_H));

    // The chrome is authored in the substrate's own format — dogfooding (D18).
    std::string toolbox;
    const OpenEPL_LibInfo* lib = ui_library();
    for (int i = 0; i < lib->component_count; i++) {
        const char* n = lib->components[i].name;
        if (std::strcmp(n, "form") == 0) continue;   // the form is the canvas itself
        toolbox += "<button class='tool' oe-add='" + std::string(n) + "'>" + n + "</button>";
    }

    char chrome[8192];
    std::snprintf(chrome, sizeof chrome,
        "<rml><head><style>"
        "body{width:%dpx;height:%dpx;font-family:'%s';font-size:14px;background-color:#15171f;color:#e8e8ee}"
        "#bar{position:absolute;left:0;top:0;width:%dpx;height:40px;background-color:#20232e;padding:8px}"
        "#bar button{display:inline-block;width:90px;height:26px;margin-right:8px;background-color:#4a86e8;color:#fff;text-align:center;padding-top:5px;border-radius:5px}"
        "#side{position:absolute;left:0;top:48px;width:224px;height:%dpx;background-color:#1b1e28;padding:10px}"
        "#side h2{font-size:15px;color:#fff}"
        ".tool{display:block;width:200px;height:28px;margin-bottom:6px;background-color:#2f3446;color:#fff;text-align:center;padding-top:6px;border-radius:5px}"
        "#canvaswrap{position:absolute;left:%dpx;top:%dpx}"
        "#canvas{position:relative;width:420px;height:260px;background-color:#1e2233}"
        "#inspector{position:absolute;right:0;top:48px;width:260px;height:%dpx;background-color:#1b1e28;padding:10px}"
        "#inspector .row{display:block;margin-bottom:6px}"
        "#inspector label{display:block;color:#9aa3b5;font-size:12px}"
        "#inspector input{display:block;width:230px;height:22px;background-color:#0f1117;color:#fff;padding-left:4px}"
        "#status{position:absolute;left:0;bottom:0;width:%dpx;height:24px;background-color:#20232e;color:#9aa3b5;padding:5px}"
        "</style></head><body>"
        "<div id='bar'><button oe-action='save'>Save</button><button oe-action='run'>Run</button></div>"
        "<div id='side'><h2>Toolbox</h2>%s</div>"
        "<div id='canvaswrap'><div id='canvas'/></div>"
        "<div id='inspector'/>"
        "<div id='status'>ready</div>"
        "</body></rml>",
        WIN_W, WIN_H, family.c_str(), WIN_W, WIN_H - 72, CANVAS_X, CANVAS_Y, WIN_H - 72, WIN_W);

    g.doc = g.context->LoadDocumentFromMemory(chrome);
    if (!g.doc) { std::fprintf(stderr, "designer: chrome failed to load\n"); return 1; }
    g.doc->Show();

    for (const char* ev : {"click", "change", "mousedown", "mousemove", "mouseup"}) {
        g.doc->AddEventListener(ev, &g_listener);
    }

    rebuild_canvas();
    rebuild_inspector();
    set_status("editing " + path);

    if (const char* script = std::getenv("OPENEPL_DESIGNER_SCRIPT")) {
        g.context->Update();
        run_script(script);
        std::printf("designer: script complete\n");
        Rml::Shutdown();
        Backend::Shutdown();
        return 0;
    }

    while (Backend::ProcessEvents(g.context, nullptr, true)) {
        g.context->Update();
        Backend::BeginFrame();
        g.context->Render();
        Backend::PresentFrame();
    }
    Rml::Shutdown();
    Backend::Shutdown();
    return 0;
}
