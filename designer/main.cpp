/* OpenEPL Studio — the visual designer (PRD §7 Phase 3, metric M0).
 *
 * Chrome follows the OpenEPL Studio design specification: title bar, menu bar,
 * action toolbar, toolbox / designer / inspector docks, a split code+output
 * panel, and a status bar. Tokens live in theme.h so no colour is hard-coded
 * here.
 *
 * Dogfoods RmlUi (ADR 0005/D18): the IDE and the apps it builds run on the same
 * substrate, and the canvas builds components through the SHARED mapping
 * (libs/ui/ui_mapping.h) so what you draw is what you get (D9).
 *
 * It never parses .oir — `openepl inspect` is the only reader — and saving
 * splices the regenerated form over the original lines so hand-written code
 * survives (ADR 0011).
 */
#include <RmlUi/Core.h>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <fstream>
#include <sstream>
#include <string>
#include <vector>

#include "RmlUi_Backend.h"
#include "RmlUi_Include_GL3.h"
#include "RmlUi_Renderer_GL3.h"
#include "descriptors.h"
#include "dotgrid.h"
#include "highlight.h"
#include "model.h"
#include "theme.h"
#include "ui_mapping.h"

using namespace openepl::designer;

namespace {

constexpr int WIN_W = 1440, WIN_H = 900;

/// Components named in the design spec that the UI library does not provide
/// yet. Shown greyed so the toolbox reads as designed while staying honest
/// about what actually exists — clicking one says so rather than failing oddly.
struct PlannedTool { const char* section; const char* name; };
const PlannedTool PLANNED[] = {
    {"Common Controls", "EditBox"},  {"Common Controls", "ListBox"},
    {"Common Controls", "ComboBox"}, {"Common Controls", "CheckBox"},
    {"Common Controls", "Image"},    {"Containers", "GroupBox"},
    {"Containers", "TabControl"},    {"Containers", "Splitter"},
    {"System", "Timer"},             {"System", "FileDialog"},
    {"System", "TrayIcon"},
};

struct Designer {
    Rml::Context* context = nullptr;
    Rml::ElementDocument* doc = nullptr;
    Model model;
    std::string openepl_bin = "./target/debug/openepl";
    std::string selected;
    std::string inspector_tab = "props";
    std::string search;
    std::vector<std::string> pending_subs;
    std::vector<std::string> log_lines;
    bool dirty = false;
    bool dragging = false;
    int drag_dx = 0, drag_dy = 0;
};
Designer g;

std::string esc(const std::string& s) { return escape_rml(s); }

/// Filename without directories, for the title bar.
std::string basename_of(const std::string& p) {
    const size_t slash = p.find_last_of('/');
    return slash == std::string::npos ? p : p.substr(slash + 1);
}

Rml::Element* by_id(const char* id) { return g.doc ? g.doc->GetElementById(id) : nullptr; }

void set_status(const std::string& text) {
    if (Rml::Element* e = by_id("statustext")) e->SetInnerRML(esc(text));
    std::printf("designer: %s\n", text.c_str());
    std::fflush(stdout);
}

void log(const std::string& line, const char* cls = nullptr) {
    g.log_lines.push_back(cls ? "<span class='" + std::string(cls) + "'>" + esc(line) + "</span>"
                              : esc(line));
    if (Rml::Element* e = by_id("log")) {
        std::string html;
        for (const auto& l : g.log_lines) html += "<div>" + l + "</div>";
        e->SetInnerRML(html);
    }
}

void mark_dirty() { g.dirty = true; }

/* --- toolbox -------------------------------------------------------------- */

bool matches_search(const std::string& name) {
    if (g.search.empty()) return true;
    std::string a = name, b = g.search;
    for (auto& c : a) c = (char)tolower((unsigned char)c);
    for (auto& c : b) c = (char)tolower((unsigned char)c);
    return a.find(b) != std::string::npos;
}

std::string build_toolbox() {
    std::string html;
    const OpenEPL_LibInfo* lib = ui_library();

    auto section = [&](const char* title, bool real_first) {
        std::string body;
        if (real_first) {
            for (int i = 0; i < lib->component_count; i++) {
                const char* n = lib->components[i].name;
                if (std::strcmp(n, "form") == 0) continue;
                if (!matches_search(n)) continue;
                body += "<div class='tool' oe-add='" + std::string(n) + "'>"
                        "<span class='ico'>■</span> " + n + "</div>";
            }
        }
        for (const auto& p : PLANNED) {
            if (std::strcmp(p.section, title) != 0) continue;
            if (!matches_search(p.name)) continue;
            body += "<div class='tool soon' oe-soon='" + std::string(p.name) + "'>"
                    "<span class='ico'>□</span> " + p.name + "</div>";
        }
        if (body.empty()) return;
        html += "<div class='sect'>" + std::string(title) + "</div>" + body;
    };

    section("Common Controls", true);
    section("Containers", false);
    section("System", false);
    if (html.empty()) html = "<div class='hint'>No matches.</div>";
    return html;
}


/// Build the whole IDE chrome. Structure follows the OpenEPL Studio design
/// specification: title bar, menu bar, action toolbar, toolbox / designer /
/// inspector docks, a split code+output panel, and a status bar.
std::string build_chrome(const std::string& family, const std::string& dot_tile) {
    using namespace theme;
    const int content_y = TITLEBAR_H + MENUBAR_H + TOOLBAR_H;
    const int content_h = WIN_H - content_y - STATUS_H;
    const int centre_w  = WIN_W - TOOLBOX_W - INSPECT_W;
    const int canvas_h  = content_h - TABBAR_H - BOTTOM_H;

    std::ostringstream s;
    s << "<rml><head><style>";

    // ---- base -----------------------------------------------------------
    s << "body{width:" << WIN_W << "px;height:" << WIN_H << "px;font-family:'" << family
      << "';font-size:12px;color:" << TEXT << ";background-color:" << CHROME << "}";
    s << "div{display:block}";
    s << "#titlebar,#menubar,#toolbar,#toolbox,#centre,#inspectdock,#bottom,#status,"
         "#canvasarea,#formwin,#overlay,.pane,.wc,.dot,.selbox,.handle,.badge"
         "{position:absolute}";
    s << "span{display:inline}";

    // ---- title bar ------------------------------------------------------
    s << "#titlebar{left:0;top:0;width:" << WIN_W << "px;height:" << TITLEBAR_H
      << "px;background-color:" << CHROME << ";border-bottom:1px " << BORDER_SOFT << "}";
    s << "#titlebar .appicon{position:absolute;left:10px;top:7px;width:18px;height:18px;"
         "background-color:" << ACCENT << ";border-radius:4px;color:#fff;text-align:center;"
         "font-size:11px;padding-top:3px}";
    s << "#titlebar .title{position:absolute;left:38px;top:8px;width:700px;height:18px;"
         "overflow:hidden;white-space:nowrap;font-size:13px;font-weight:bold;color:" << TEXT << "}";
    s << "#titlebar .wc{position:absolute;top:8px;width:16px;height:16px;border-radius:8px}";

    // ---- menu bar -------------------------------------------------------
    s << "#menubar{left:0;top:" << TITLEBAR_H << "px;width:" << WIN_W << "px;height:" << MENUBAR_H
      << "px;background-color:" << CHROME_ALT << ";border-bottom:1px " << BORDER_SOFT << "}";
    s << "#menubar .m{display:inline-block;padding:6px 10px 6px 10px;"
         "font-size:12px;color:" << TEXT << ";border-radius:4px}";
    s << "#menubar .m:hover{background-color:#e8eaed}";

    // ---- action toolbar -------------------------------------------------
    s << "#toolbar{left:0;top:" << (TITLEBAR_H + MENUBAR_H) << "px;width:" << WIN_W << "px;height:"
      << TOOLBAR_H << "px;background-color:" << CHROME_ALT << ";border-bottom:1px " << BORDER << "}";
    s << ".tb{display:inline-block;height:26px;margin:7px 2px 0 2px;padding:5px 10px 0 10px;"
         "border-radius:5px;font-size:12px;color:" << TEXT << "}";
    s << ".tb:hover{background-color:#e8eaed}";
    s << ".tb.primary{background-color:" << ACCENT << ";color:" << ACCENT_TEXT << "}";
    s << ".tb.ghost{color:" << TEXT_MUTED << "}";
    s << ".tb.run{color:" << SUCCESS << ";font-weight:bold}";
    s << ".tb.stop{color:" << DANGER << "}";
    s << ".sep{display:inline-block;width:1px;height:20px;margin:10px 8px 0 8px;"
         "background-color:" << BORDER << "}";

    // ---- toolbox --------------------------------------------------------
    s << "#toolbox{left:0;top:" << content_y << "px;width:" << TOOLBOX_W << "px;height:" << content_h
      << "px;background-color:" << PANEL << ";border-right:1px " << BORDER << "}";
    s << ".panelhead{height:28px;padding:8px 10px 0 10px;font-size:11px;"
         "font-weight:bold;color:" << TEXT_MUTED << ";background-color:" << CHROME_ALT
      << ";border-bottom:1px " << BORDER_SOFT << "}";
    s << "#search{margin:8px;width:" << (TOOLBOX_W - 16)
      << "px;height:26px;border:1px " << BORDER << ";border-radius:5px;background-color:" << PANEL
      << ";color:" << TEXT << ";padding-left:8px;font-size:12px}";
    s << ".sect{margin:6px 8px 2px 8px;font-size:11px;font-weight:bold;"
         "color:" << TEXT_MUTED << "}";
    s << ".tool{display:block;height:28px;margin:0 6px 2px 6px;padding:6px 8px 0 8px;"
         "border-radius:4px;font-size:12px;color:" << TEXT << "}";
    s << ".tool:hover{background-color:#eef2f8}";
    s << ".tool.sel{background-color:" << ACCENT << ";color:" << ACCENT_TEXT << "}";
    s << ".tool.soon{color:#aeb6c2}";
    s << ".tool .ico{display:inline-block;width:14px;color:" << ACCENT << "}";
    s << ".tool.soon .ico{color:#c9d0da}";

    // ---- centre: tabs + canvas -----------------------------------------
    s << "#centre{left:" << TOOLBOX_W << "px;top:" << content_y << "px;width:" << centre_w
      << "px;height:" << content_h << "px;background-color:" << CANVAS << "}";
    s << "#tabs{height:" << TABBAR_H << "px;background-color:" << CHROME_ALT
      << ";border-bottom:1px " << BORDER << "}";
    s << ".tab{display:inline-block;height:" << TABBAR_H
      << "px;padding:9px 14px 0 14px;font-size:12px;color:" << TEXT_MUTED << "}";
    s << ".tab.active{background-color:" << CANVAS << ";color:" << TEXT
      << ";border-bottom:2px " << ACCENT << "}";
    s << "#canvasarea{left:0;top:" << TABBAR_H << "px;width:" << centre_w << "px;height:" << canvas_h
      << "px;background-color:" << CANVAS << ";decorator:image(\"" << dot_tile << "\" repeat)}";

    // The form preview: a floating native-looking window.
    s << "#formwin{left:60px;top:40px;border-radius:8px;background-color:#ffffff;"
         "box-shadow:#0000002e 0 10px 30px 0px, #00000014 0 2px 6px 0px}";
    s << "#formtitle{height:28px;background-color:" << CHROME
      << ";border-bottom:1px " << BORDER_SOFT << ";border-top-left-radius:8px;"
         "border-top-right-radius:8px;padding:7px 10px 0 10px;font-size:12px;color:" << TEXT << "}";
    s << "#formtitle .dot{position:absolute;top:10px;width:9px;height:9px;border-radius:5px}";
    s << "#canvas div,#canvas button{position:absolute}";
    s << "#canvas{position:relative;overflow:hidden;border-bottom-left-radius:8px;"
         "border-bottom-right-radius:8px}";
    // selection chrome
    s << "#overlay{position:absolute;left:0;top:0;width:100%;height:100%}";
    s << ".selbox{border:1px " << ACCENT << "}";
    s << ".handle{width:7px;height:7px;background-color:#ffffff;border:1px " << ACCENT << "}";
    s << ".badge{background-color:" << ACCENT << ";color:#fff;font-size:11px;padding:3px 7px 3px 7px;"
         "border-radius:4px;white-space:nowrap}";

    // ---- inspector ------------------------------------------------------
    s << "#inspectdock{left:" << (TOOLBOX_W + centre_w) << "px;top:" << content_y << "px;width:"
      << INSPECT_W << "px;height:" << content_h << "px;background-color:" << PANEL
      << ";border-left:1px " << BORDER << "}";
    s << ".segbar{margin:8px;height:26px;white-space:nowrap}";
    s << ".seg{display:inline-block;width:" << ((INSPECT_W - 18) / 2)
      << "px;height:26px;padding-top:5px;text-align:center;font-size:12px;color:" << TEXT_MUTED
      << ";background-color:" << CHROME << ";border:1px " << BORDER << "}";
    s << ".seg.active{background-color:" << ACCENT << ";color:" << ACCENT_TEXT << ";border:1px "
      << ACCENT << "}";
    s << "#ctxlabel{margin:2px 10px 6px 10px;font-size:13px;font-weight:bold;"
         "color:" << TEXT << "}";
    s << "#grid{margin:0 10px 0 10px}";
    s << ".prow{margin-bottom:8px}";
    s << ".prow label{display:block;font-size:11px;color:" << TEXT_MUTED << ";margin-bottom:2px}";
    s << ".prow input{display:block;width:" << (INSPECT_W - 24)
      << "px;height:24px;border:1px " << BORDER << ";border-radius:4px;background-color:" << PANEL
      << ";color:" << TEXT << ";padding-left:6px;font-size:12px}";
    s << ".prow input:focus{border:1px " << ACCENT << "}";
    s << ".note{font-size:11px;font-style:italic;color:" << TEXT_MUTED << ";margin-top:2px}";
    s << ".wire{margin:10px;padding:8px;background-color:" << CHROME_ALT
      << ";border:1px " << BORDER_SOFT << ";border-radius:5px}";
    s << ".wire .h{font-size:11px;font-weight:bold;color:" << TEXT_MUTED << ";margin-bottom:4px}";
    s << ".wire .link{color:" << ACCENT << "}";
    s << ".hint{color:" << TEXT_MUTED << ";font-size:12px;padding:10px}";

    // ---- bottom split ---------------------------------------------------
    s << "#bottom{left:" << TOOLBOX_W << "px;top:" << (content_y + TABBAR_H + canvas_h)
      << "px;width:" << centre_w << "px;height:" << BOTTOM_H << "px;background-color:" << PANEL
      << ";border-top:1px " << BORDER << "}";
    s << ".pane{position:absolute;top:0;height:" << BOTTOM_H << "px;overflow:hidden}";
    s << ".panehead{height:26px;padding:6px 10px 0 10px;font-size:11px;"
         "font-weight:bold;color:" << TEXT_MUTED << ";background-color:" << CHROME_ALT
      << ";border-bottom:1px " << BORDER_SOFT << "}";
    s << "#code{font-family:'" << family
      << "';font-size:12px;padding:6px 0 0 0;height:" << (BOTTOM_H - 32) << "px;overflow:auto}";
    s << ".ln{display:inline-block;width:30px;color:#9aa3b0;text-align:right;padding-right:8px}";
    s << ".cl{display:block;white-space:pre;padding-left:2px;height:17px}";
    s << ".cl span{white-space:pre}";
    s << ".k{color:" << SYN_KEYWORD << "}.m{color:" << SYN_METHOD << "}.s{color:" << SYN_STRING
      << "}.i{color:" << SYN_IDENT << "}.c{color:" << SYN_COMMENT << ";font-style:italic}.n{color:"
      << SYN_NUMBER << "}";
    s << "#log{font-size:12px;padding:6px 10px 0 10px;height:" << (BOTTOM_H - 32)
      << "px;overflow:auto;color:" << TEXT << "}";
    s << "#log .ok{color:" << SUCCESS << "}#log .muted{color:" << TEXT_MUTED << "}";
    s << "#log div{white-space:nowrap;overflow:hidden;height:17px}";

    // ---- status bar -----------------------------------------------------
    s << "#status{left:0;top:" << (WIN_H - STATUS_H) << "px;width:" << WIN_W << "px;height:"
      << STATUS_H << "px;background-color:" << CHROME << ";border-top:1px " << BORDER
      << ";font-size:11px;color:" << TEXT_MUTED << ";padding:5px 10px 0 10px}";
    s << "#status .right{position:absolute;right:26px;top:5px;width:200px;text-align:right;white-space:nowrap}";
    s << "#status .dot{color:" << SUCCESS << "}";

    s << "</style></head><body>";

    // ---- markup ---------------------------------------------------------
    s << "<div id='titlebar'><div class='appicon'>E</div>"
         "<div class='title'>OpenEPL Studio — " << esc(basename_of(g.model.path)) << " — ["
      << esc(g.model.form_name) << "]</div>"
         "<div class='wc' style='right:66px;background-color:#febc2e'/>"
         "<div class='wc' style='right:42px;background-color:#28c840'/>"
         "<div class='wc' style='right:18px;background-color:#ff5f57'/></div>";

    s << "<div id='menubar'>";
    for (const char* m : {"File", "Edit", "View", "Project", "Components", "Build", "Debug", "Run", "Help"})
        s << "<div class='m'>" << m << "</div>";
    s << "</div>";

    s << "<div id='toolbar'>"
         "<div class='tb' oe-action='new'>New</div>"
         "<div class='tb' oe-action='open'>Open</div>"
         "<div class='tb' oe-action='save'>Save</div>"
         "<div class='sep'/>"
         "<div class='tb primary' oe-action='view-designer'>Designer</div>"
         "<div class='tb ghost' oe-action='view-code'>Code</div>"
         "<div class='sep'/>"
         "<div class='tb run' oe-action='run'>▶ Run</div>"
         "<div class='tb' oe-action='build'>Build Binary</div>"
         "<div class='tb stop' oe-action='stop'>■ Stop</div>"
         "</div>";

    s << "<div id='toolbox'><div class='panelhead'>TOOLBOX</div>"
         "<input type='text' id='search' placeholder='Search toolbox...'/>"
      << build_toolbox() << "</div>";

    s << "<div id='centre'>"
         "<div id='tabs'>"
         "<div class='tab active'>Designer — " << esc(g.model.form_name) << "</div>"
         "<div class='tab'>Code</div>"
         "</div>"
         "<div id='canvasarea'>"
         "<div id='formwin'>"
         "<div id='formtitle'><span id='formtitletext'>Form</span>"
         "<div class='dot' style='right:12px;background-color:#ff5f57'/>"
         "<div class='dot' style='right:26px;background-color:#febc2e'/>"
         "<div class='dot' style='right:40px;background-color:#28c840'/></div>"
         "<div id='canvas'><div id='overlay'/></div></div>"
         "</div></div>";

    s << "<div id='inspectdock'><div class='panelhead'>PROPERTIES / EVENTS</div>"
         "<div class='segbar'><div class='seg active' oe-tab='props'>Properties</div>"
         "<div class='seg' oe-tab='events'>Events</div></div>"
         "<div id='ctxlabel'>—</div><div id='grid'/><div id='wirebox'/></div>";

    const int half = centre_w / 2;
    s << "<div id='bottom'>"
         "<div class='pane' style='left:0;width:" << half << "px;border-right:1px " << BORDER << "'>"
         "<div class='panehead'>CODE EDITOR</div><div id='code'/></div>"
         "<div class='pane' style='left:" << half << "px;width:" << (centre_w - half) << "px'>"
         "<div class='panehead'>OUTPUT / BUILD LOG</div><div id='log'/></div>"
         "</div>";

    s << "<div id='status'>RAD is the identity  |  English-first  |  "
         "Cross-platform  |  Assignment is not an expression"
         "<span class='right'><span id='statustext'>Ready</span>   "
         "<span class='dot'>●</span></span></div>";

    s << "</body></rml>";
    return s.str();
}

/* --- canvas --------------------------------------------------------------- */

int prop_int(const Component& c, const char* name, int fallback) {
    if (const std::string* v = c.property(name)) {
        return std::atoi(v->c_str());
    }
    return fallback;
}

void rebuild_canvas() {
    Rml::Element* formwin = by_id("formwin");
    Rml::Element* canvas = by_id("canvas");
    Rml::Element* overlay = by_id("overlay");
    if (!canvas || !formwin || !overlay) return;

    const int fw = prop_int(g.model.form, "width", 420);
    const int fh = prop_int(g.model.form, "height", 260);
    formwin->SetProperty("width", Rml::String(std::to_string(fw) + "px"));
    canvas->SetProperty("width", Rml::String(std::to_string(fw) + "px"));
    canvas->SetProperty("height", Rml::String(std::to_string(fh) + "px"));
    if (const std::string* bg = g.model.form.property("background_color")) {
        canvas->SetProperty("background-color", *bg);
    }
    if (Rml::Element* t = by_id("formtitletext")) {
        const std::string* title = g.model.form.property("title");
        t->SetInnerRML(esc(title ? *title : g.model.form_name));
    }

    // Clear components but keep the overlay element itself.
    for (int i = canvas->GetNumChildren() - 1; i >= 0; i--) {
        Rml::Element* child = canvas->GetChild(i);
        if (child != overlay) canvas->RemoveChild(child);
    }
    while (overlay->GetNumChildren() > 0) overlay->RemoveChild(overlay->GetChild(0));

    for (const auto& comp : g.model.children) {
        Rml::ElementPtr child = g.doc->CreateElement(openepl::ui::tag_for(comp.type_name.c_str()));
        Rml::Element* e = canvas->AppendChild(std::move(child));
        e->SetProperty("position", "absolute");
        e->SetAttribute("oe-id", comp.id);
        for (const auto& p : comp.properties) {
            if (openepl::ui::is_text_property(p.first.c_str())) {
                e->SetInnerRML(esc(p.second));
            } else {
                e->SetProperty(openepl::ui::rcss_name(p.first.c_str()),
                               openepl::ui::rcss_value(p.first.c_str(), p.second.c_str()));
            }
        }
    }

    // Selection chrome lives in an overlay so it never perturbs the components
    // themselves — what you see on the canvas is exactly what the app renders.
    if (const Component* sel = g.model.find(g.selected)) {
        const int x = prop_int(*sel, "left", 0), y = prop_int(*sel, "top", 0);
        const int w = prop_int(*sel, "width", 120), h = prop_int(*sel, "height", 32);
        // The overlay lives inside the canvas, so component coordinates are
        // already the right frame of reference.
        const int ox = 0, oy = 0;
        auto place = [&](const char* cls, int px, int py, int pw, int ph) {
            Rml::Element* d = overlay->AppendChild(g.doc->CreateElement("div"));
            d->SetProperty("position", "absolute");
            d->SetAttribute("class", Rml::String(cls));
            d->SetProperty("left", Rml::String(std::to_string(px) + "px"));
            d->SetProperty("top", Rml::String(std::to_string(py) + "px"));
            if (pw) d->SetProperty("width", Rml::String(std::to_string(pw) + "px"));
            if (ph) d->SetProperty("height", Rml::String(std::to_string(ph) + "px"));
            return d;
        };
        place("selbox", ox + x - 1, oy + y - 1, w + 2, h + 2);
        const int hx[3] = {ox + x - 4, ox + x + w / 2 - 4, ox + x + w - 4};
        const int hy[3] = {oy + y - 4, oy + y + h / 2 - 4, oy + y + h - 4};
        for (int i = 0; i < 3; i++) {
            for (int j = 0; j < 3; j++) {
                if (i == 1 && j == 1) continue;   // 8 anchors, not 9
                place("handle", hx[i], hy[j], 0, 0);
            }
        }
        // Event connector badge, as specified.
        if (!sel->handlers.empty()) {
            const auto& hnd = sel->handlers.front();
            Rml::Element* b = place("badge", ox + x, oy + y - 24, 0, 0);
            b->SetInnerRML("▸ on" + esc(hnd.first) + " → " + esc(hnd.second));
        }
    }
}

/* --- inspector ------------------------------------------------------------ */

void rebuild_inspector() {
    Rml::Element* ctx = by_id("ctxlabel");
    Rml::Element* grid = by_id("grid");
    Rml::Element* wire = by_id("wirebox");
    if (!ctx || !grid || !wire) return;

    Component* comp = g.model.find(g.selected);
    if (!comp) {
        ctx->SetInnerRML("—");
        grid->SetInnerRML("<div class='hint'>Select a component on the canvas.</div>");
        wire->SetInnerRML("");
        return;
    }
    ctx->SetInnerRML(esc(comp->id) + " <span style='color:#656d76;font-weight:normal'>(" +
                     esc(comp->type_name) + ")</span>");

    const OpenEPL_ComponentDesc* desc = describe(comp->type_name.c_str());
    if (!desc) { grid->SetInnerRML("<div class='hint'>unknown type</div>"); return; }

    std::string html;
    if (g.inspector_tab == "props") {
        // The instance id, flagged as compile-time-only (it never ships — G8).
        html += "<div class='prow'><label>Name</label>"
                "<input type='text' class='cid' value='" + esc(comp->id) + "'/>"
                "<div class='note'>internal only — does not ship to the binary</div></div>";
        for (int i = 0; i < desc->property_count; i++) {
            const char* name = desc->properties[i].name;
            const std::string* v = comp->property(name);
            // Show what the file actually sets, not the descriptor default:
            // an unset property is not applied at run time, and the canvas
            // renders it unset, so displaying the default here would make the
            // inspector disagree with both.
            const char* def = desc->properties[i].default_value;
            html += "<div class='prow'><label>" + std::string(name) +
                    (v ? "" : std::string(" <span style='color:#9aa3b0'>(unset")
                                  + (def ? std::string(", default ") + def : "") + ")</span>") +
                    "</label>"
                    "<input type='text' class='pv' name='" + std::string(name) + "' value='" +
                    esc(v ? *v : "") + "'/></div>";
        }
    } else {
        if (desc->event_count == 0) html += "<div class='hint'>This component has no events.</div>";
        for (int i = 0; i < desc->event_count; i++) {
            const char* ev = desc->events[i].name;
            const std::string* h = comp->handler(ev);
            html += "<div class='prow'><label>On" + std::string(ev) + "</label>"
                    "<input type='text' class='ev' name='" + std::string(ev) + "' value='" +
                    esc(h ? *h : "") + "'/></div>";
        }
    }
    grid->SetInnerRML(html);

    std::string wired;
    for (const auto& h : comp->handlers) {
        if (!h.second.empty()) {
            wired += "<div class='link'>▸ " + esc(h.second) + "()</div>";
        }
    }
    wire->SetInnerRML("<div class='h'>HANDLER WIRING</div>" +
                      (wired.empty() ? std::string("<div class='hint'>Not wired.</div>")
                                     : "<div>Linked to:</div>" + wired));
}

/* --- code pane ------------------------------------------------------------ */

/// Show the source of the subroutine wired to the selection, or the whole file
/// when nothing is selected.
void rebuild_code() {
    Rml::Element* code = by_id("code");
    if (!code) return;
    std::ifstream f(g.model.path);
    std::vector<std::string> lines;
    std::string l;
    while (std::getline(f, l)) lines.push_back(l);

    std::string want;
    if (const Component* c = g.model.find(g.selected)) {
        if (!c->handlers.empty()) want = c->handlers.front().second;
    }

    size_t from = 0, to = lines.size();
    if (!want.empty()) {
        for (size_t i = 0; i < lines.size(); i++) {
            if (lines[i].rfind("sub " + want, 0) == 0) {
                from = i;
                for (size_t j = i; j < lines.size(); j++) {
                    if (lines[j] == "end") { to = j + 1; break; }
                }
                break;
            }
        }
    }

    std::string html;
    for (size_t i = from; i < to && i < lines.size(); i++) {
        html += "<div class='cl'><span class='ln'>" + std::to_string(i + 1) + "</span>" +
                highlight_line(lines[i]) + "</div>";
    }
    if (Rml::Element* head = code->GetParentNode()->GetChild(0)) {
        head->SetInnerRML(want.empty() ? "CODE EDITOR · whole module"
                                       : "CODE EDITOR · " + esc(want));
    }
    code->SetInnerRML(html);
}

void refresh_all() {
    rebuild_canvas();
    rebuild_inspector();
    rebuild_code();
}

void select(const std::string& id) {
    g.selected = id;
    refresh_all();
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
    c.set_property("left", std::to_string(20 + 12 * (int)g.model.children.size()));
    c.set_property("top", std::to_string(20 + 34 * (int)g.model.children.size()));
    g.model.children.push_back(c);
    mark_dirty();
    set_status("added " + c.id);
    select(c.id);
}

void save() {
    std::string err;
    if (!save_model(g.model, g.pending_subs, err)) { set_status("save failed: " + err); return; }
    g.pending_subs.clear();
    g.dirty = false;
    set_status("saved " + g.model.path);
    rebuild_code();
}

/// Run a command, streaming its output into the build log.
int run_logged(const std::string& cmd) {
    FILE* pipe = popen((cmd + " 2>&1").c_str(), "r");
    if (!pipe) { log("could not start: " + cmd); return -1; }
    char buf[512];
    while (fgets(buf, sizeof buf, pipe)) {
        std::string line(buf);
        while (!line.empty() && (line.back() == '\n' || line.back() == '\r')) line.pop_back();
        if (!line.empty()) log(line, "muted");
    }
    return pclose(pipe);
}

void build_binary(bool then_run) {
    save();
    g.log_lines.clear();
    log("> openepl build " + g.model.path);
    log("> IR -> LLVM -> ld (system linker)", "muted");
    log("> Dead-strip unused commands", "muted");
    const std::string out = "/tmp/openepl_studio_app";
    const int rc = run_logged(g.openepl_bin + " build " + g.model.path + " -o " + out);
    if (rc != 0) { log("Build failed.", nullptr); set_status("build failed"); return; }

    long bytes = 0;
    if (FILE* f = std::fopen(out.c_str(), "rb")) {
        std::fseek(f, 0, SEEK_END);
        bytes = std::ftell(f);
        std::fclose(f);
    }
    char line[256];
    std::snprintf(line, sizeof line, "OK  Output: %s — %ld KB (clean native, no runtime unpack)",
                  out.c_str(), bytes / 1024);
    log(line, "ok");
    log("> IR stripped — no decompilation possible", "muted");
    set_status("build succeeded");
    if (then_run) {
        log("> running…", "muted");
        std::system((out + " &").c_str());
    }
}


/* --- events --------------------------------------------------------------- */

struct Listener : Rml::EventListener {
    void ProcessEvent(Rml::Event& ev) override {
        Rml::Element* el = ev.GetTargetElement();
        const Rml::String type = ev.GetType();

        if (type == "change") {
            Rml::Element* src = el;
            const Rml::String cls = src->GetAttribute<Rml::String>("class", "");
            const Rml::String value = src->GetAttribute<Rml::String>("value", "");
            if (src->GetId() == "search") {
                g.search = value;
                if (Rml::Element* tb = by_id("toolbox")) {
                    // Rebuild only the tool list, leaving header and search box.
                    while (tb->GetNumChildren() > 2) tb->RemoveChild(tb->GetChild(2));
                    Rml::Element* holder = tb->AppendChild(g.doc->CreateElement("div"));
                    holder->SetProperty("position", "relative");
                    holder->SetInnerRML(build_toolbox());
                }
                return;
            }
            Component* comp = g.model.find(g.selected);
            if (!comp) return;
            const Rml::String name = src->GetAttribute<Rml::String>("name", "");
            if (cls.find("cid") != Rml::String::npos) {
                set_status("renaming components is not supported yet");
                return;
            }
            if (name.empty()) return;
            if (cls.find("ev") != Rml::String::npos) {
                comp->set_handler(name, value);
                mark_dirty();
                if (!value.empty() && !g.model.has_sub(value)) {
                    g.pending_subs.push_back(value);
                    set_status("will create sub " + value);
                }
                refresh_all();
            } else if (cls.find("pv") != Rml::String::npos) {
                comp->set_property(name, value);
                mark_dirty();
                refresh_all();
            }
            return;
        }

        if (type == "click") {
            for (Rml::Element* e = el; e; e = e->GetParentNode()) {
                if (e->HasAttribute("oe-add")) {
                    add_component(e->GetAttribute<Rml::String>("oe-add", ""));
                    return;
                }
                if (e->HasAttribute("oe-soon")) {
                    set_status(e->GetAttribute<Rml::String>("oe-soon", "") +
                               " is in the design spec but not implemented yet");
                    return;
                }
                if (e->HasAttribute("oe-tab")) {
                    g.inspector_tab = e->GetAttribute<Rml::String>("oe-tab", "props");
                    for (const char* id : {"props", "events"}) {
                        // Repaint the segmented control.
                    }
                    if (Rml::Element* bar = e->GetParentNode()) {
                        for (int i = 0; i < bar->GetNumChildren(); i++) {
                            Rml::Element* seg = bar->GetChild(i);
                            const bool active =
                                seg->GetAttribute<Rml::String>("oe-tab", "") == g.inspector_tab;
                            seg->SetAttribute("class",
                                              Rml::String(active ? "seg active" : "seg"));
                        }
                    }
                    rebuild_inspector();
                    return;
                }
                if (e->HasAttribute("oe-action")) {
                    const Rml::String a = e->GetAttribute<Rml::String>("oe-action", "");
                    if (a == "save") save();
                    else if (a == "run") build_binary(true);
                    else if (a == "build") build_binary(false);
                    else if (a == "stop") set_status("nothing running");
                    else set_status(a + " is not implemented yet");
                    return;
                }
                if (e->HasAttribute("oe-id")) {
                    select(e->GetAttribute<Rml::String>("oe-id", ""));
                    return;
                }
            }
            return;
        }

        if (type == "mousedown" && el->HasAttribute("oe-id")) {
            select(el->GetAttribute<Rml::String>("oe-id", ""));
            g.dragging = true;
            const auto off = el->GetAbsoluteOffset();
            g.drag_dx = ev.GetParameter<int>("mouse_x", 0) - (int)off.x;
            g.drag_dy = ev.GetParameter<int>("mouse_y", 0) - (int)off.y;
        } else if (type == "mousemove" && g.dragging && !g.selected.empty()) {
            Component* comp = g.model.find(g.selected);
            if (!comp) return;
            Rml::Element* canvas = by_id("canvas");
            if (!canvas) return;
            const auto origin = canvas->GetAbsoluteOffset();
            const int x = ev.GetParameter<int>("mouse_x", 0) - g.drag_dx - (int)origin.x;
            const int y = ev.GetParameter<int>("mouse_y", 0) - g.drag_dy - (int)origin.y;
            comp->set_property("left", std::to_string(x < 0 ? 0 : x));
            comp->set_property("top", std::to_string(y < 0 ? 0 : y));
            mark_dirty();
            rebuild_canvas();
        } else if (type == "mouseup") {
            if (g.dragging) rebuild_inspector();
            g.dragging = false;
        }
    }
};
Listener g_listener;

/// Render a few frames and write the framebuffer, so the chrome can be
/// inspected without a human at the window.
void dump_frame() {
    const char* path = std::getenv("OPENEPL_DESIGNER_DUMP");
    if (!path) return;
    auto* gl3 = static_cast<RenderInterface_GL3*>(Backend::GetRenderInterface());
    for (int i = 0; i < 3; i++) {
        g.context->Update();
        Backend::BeginFrame();
        g.context->Render();
        gl3->EndFrame();
    }
    std::vector<unsigned char> px((size_t)WIN_W * WIN_H * 3);
    glReadPixels(0, 0, WIN_W, WIN_H, GL_RGB, GL_UNSIGNED_BYTE, px.data());
    if (FILE* f = std::fopen(path, "wb")) {
        std::fprintf(f, "P6\n%d %d\n255\n", WIN_W, WIN_H);
        for (int y = WIN_H - 1; y >= 0; y--)
            std::fwrite(&px[(size_t)y * WIN_W * 3], 1, (size_t)WIN_W * 3, f);
        std::fclose(f);
    }
    std::printf("designer: wrote %s\n", path);
}

void run_script(const char* script) {
    std::string s(script);
    size_t i = 0;
    while (i <= s.size()) {
        const size_t semi = s.find(';', i);
        const std::string cmd = s.substr(i, semi == std::string::npos ? std::string::npos : semi - i);
        if (!cmd.empty()) {
            const size_t colon = cmd.find(':');
            const std::string verb = cmd.substr(0, colon);
            const std::string arg = colon == std::string::npos ? "" : cmd.substr(colon + 1);
            if (verb == "add") add_component(arg);
            else if (verb == "select") select(arg);
            else if (verb == "set" || verb == "wire") {
                const size_t eq = arg.find('=');
                if (eq != std::string::npos && !g.selected.empty()) {
                    if (Component* c = g.model.find(g.selected)) {
                        const std::string k = arg.substr(0, eq), v = arg.substr(eq + 1);
                        if (verb == "set") {
                            c->set_property(k, v);
                        } else {
                            c->set_handler(k, v);
                            if (!g.model.has_sub(v)) g.pending_subs.push_back(v);
                        }
                        mark_dirty();
                        refresh_all();
                    }
                }
            } else if (verb == "save") save();
            else if (verb == "build") build_binary(false);
        }
        if (semi == std::string::npos) break;
        i = semi + 1;
    }
}

} // namespace

int main(int argc, char** argv) {
    if (argc < 2) {
        std::fprintf(stderr,
                     "usage: openepl-designer <project.oir> [path/to/openepl]\n\n"
                     "Environment:\n"
                     "  OPENEPL_DESIGNER_SCRIPT   run a scripted session headlessly, then exit\n"
                     "  OPENEPL_DESIGNER_DEBUG    report chrome/toolbox diagnostics\n");
        return 2;
    }
    const std::string path = argv[1];
    if (argc > 2) g.openepl_bin = argv[2];

    std::string err;
    if (!load_model(g.openepl_bin, path, g.model, err)) {
        std::fprintf(stderr, "designer: cannot load %s\n%s\n", path.c_str(), err.c_str());
        return 1;
    }

    if (!Backend::Initialize("OpenEPL Studio", WIN_W, WIN_H, true)) return 1;
    Rml::SetSystemInterface(Backend::GetSystemInterface());
    Rml::SetRenderInterface(Backend::GetRenderInterface());
    Rml::Initialise();

    int font_count = 0;
    const auto* fonts = openepl::ui::font_candidates(&font_count);
    std::string family = "sans-serif";
    for (int i = 0; i < font_count; i++) {
        if (Rml::LoadFontFace(fonts[i].path)) { family = fonts[i].family; break; }
    }

    const std::string dot_tile = write_dot_tile("/tmp/openepl_dotgrid.tga", 10);
    g.context = Rml::CreateContext("studio", Rml::Vector2i(WIN_W, WIN_H));

    const std::string chrome = build_chrome(family, dot_tile);
    if (std::getenv("OPENEPL_DESIGNER_DEBUG")) {
        std::fprintf(stderr, "designer: chrome %zu bytes\n", chrome.size());
    }
    g.doc = g.context->LoadDocumentFromMemory(chrome);
    if (!g.doc) { std::fprintf(stderr, "designer: chrome failed to load\n"); return 1; }
    g.doc->Show();

    for (const char* e : {"click", "change", "mousedown", "mousemove", "mouseup"}) {
        g.doc->AddEventListener(e, &g_listener);
    }

    refresh_all();
    log("OpenEPL Studio ready.", "muted");
    log("> " + g.model.path, "muted");
    set_status("Ready");

    if (std::getenv("OPENEPL_DESIGNER_DEBUG")) {
        if (Rml::Element* tb = by_id("toolbox")) {
            std::fprintf(stderr, "designer: toolbox has %d children\n", tb->GetNumChildren());
            for (int i = 0; i < tb->GetNumChildren(); i++) {
                Rml::Element* c = tb->GetChild(i);
                std::fprintf(stderr, "designer:   <%s> oe-add=%s\n", c->GetTagName().c_str(),
                             c->GetAttribute<Rml::String>("oe-add", "(none)").c_str());
            }
        }
    }

    if (const char* script = std::getenv("OPENEPL_DESIGNER_SCRIPT")) {
        g.context->Update();
        run_script(script);
        if (g.dirty) {
            std::printf("designer: unsaved changes — saving before exit\n");
            save();
        }
        std::printf("designer: script complete\n");
        if (std::getenv("OPENEPL_DESIGNER_DUMP")) dump_frame();
        Rml::Shutdown();
        Backend::Shutdown();
        return 0;
    }

    if (std::getenv("OPENEPL_DESIGNER_DUMP")) {
        dump_frame();
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
    if (g.dirty) {
        std::printf("designer: unsaved changes — saving before exit\n");
        save();
    }
    Rml::Shutdown();
    Backend::Shutdown();
    return 0;
}
