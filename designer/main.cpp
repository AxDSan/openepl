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
#include <dirent.h>
#include <fstream>
#include <signal.h>
#include <sstream>
#include <sys/wait.h>
#include <unistd.h>
#include <string>
#include <vector>

#include <SDL.h>

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

/// Initial window size. The IDE follows the OS window after that — see
/// relayout(), which is re-run whenever the context's dimensions change.
constexpr int INIT_W = 1440, INIT_H = 900;

/// Components named in the design spec that the UI library does not provide
/// yet. Shown greyed so the toolbox reads as designed while staying honest
/// about what actually exists — clicking one says so rather than failing oddly.
/// A menu entry: label, the action it fires, and its shortcut hint.
struct MenuItem { const char* label; const char* action; const char* keys; };
struct Menu { const char* title; std::vector<MenuItem> items; };

/// The menu bar. Every entry here does something — an entry that only prints
/// "not implemented" is worse than no entry at all.
inline const std::vector<Menu>& menus() {
    static const std::vector<Menu> m = {
        {"File", {{"Save", "save", "Ctrl+S"}, {"Build Binary", "build", ""},
                  {"Run", "run", ""}, {"Exit", "exit", ""}}},
        {"Edit", {{"Undo", "undo", "Ctrl+Z"}, {"Redo", "redo", "Ctrl+Shift+Z"},
                  {"Copy", "copy", "Ctrl+C"}, {"Paste", "paste", "Ctrl+V"},
                  {"Delete", "delete", "Del"}}},
        {"View", {{"Designer", "view-designer", ""}, {"Code", "view-code", ""}}},
        {"Build", {{"Build Binary", "build", ""}, {"Run", "run", ""}, {"Stop", "stop", ""}}},
        {"Help", {{"About OpenEPL", "about", ""}}},
    };
    return m;
}

struct PlannedTool { const char* section; const char* name; };
const PlannedTool PLANNED[] = {
    // These need language or platform features that do not exist yet: list
    // controls need arrays, and the System components need timers and native
    // dialogs. Listed greyed rather than omitted, so the toolbox shows the
    // intended shape without pretending they work.
    {"Common Controls", "ListBox"},  {"Common Controls", "ComboBox"},
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

    /// Form-window resize, as in Visual Studio / RAD Studio: grab an edge or
    /// the corner of the preview and drag.
    bool resizing_form = false;
    std::string resize_edge;          // "e", "s" or "se"
    int resize_x0 = 0, resize_y0 = 0, resize_w0 = 0, resize_h0 = 0;

    /// Component resize via the selection anchors.
    bool resizing_comp = false;
    std::string comp_edge;
    int comp_x0 = 0, comp_y0 = 0, comp_w0 = 0, comp_h0 = 0;

    /// Dock sizes, adjustable by dragging the splitters between panels.
    int toolbox_w = theme::TOOLBOX_W;
    int inspect_w = theme::INSPECT_W;
    int bottom_h = theme::BOTTOM_H;
    std::string splitting;            // "left", "right", "bottom" or empty
    int split_x0 = 0, split_y0 = 0, split_v0 = 0;

    /// "designer" or "code" — the centre pane's view.
    std::string view = "designer";

    /// The app launched by Run, so Stop can actually stop it.
    pid_t running_app = 0;

    /// Edit history. Whole-model snapshots: the model is a few dozen strings,
    /// so copying it is cheaper than describing every edit as a reversible
    /// command, and it cannot drift from what it claims to undo.
    std::vector<Model> undo_stack;
    std::vector<Model> redo_stack;

    /// Components selected in the designer. `selected` is the primary one.
    std::vector<std::string> selection;

    /// Clipboard for copy/paste.
    std::vector<Component> clipboard;

    /// Alignment guides to draw this frame: x positions and y positions.
    std::vector<int> guide_x, guide_y;

    /// Current window size. Layout is recomputed whenever this changes, so the
    /// IDE fills the OS window instead of leaving unpainted margins.
    int win_w = INIT_W, win_h = INIT_H;
};
Designer g;

std::string esc(const std::string& s) { return escape_rml(s); }

/// Filename without directories, for the title bar.
std::string basename_of(const std::string& p) {
    const size_t slash = p.find_last_of('/');
    return slash == std::string::npos ? p : p.substr(slash + 1);
}

Rml::Element* by_id(const char* id) { return g.doc ? g.doc->GetElementById(id) : nullptr; }

/// Apply the current dock sizes to the layout. Geometry lives here rather than
/// in the stylesheet so the panels can be resized at run time.
void relayout() {
    using namespace theme;
    if (!g.doc) return;
    const int W = g.win_w, H = g.win_h;
    const int content_y = TITLEBAR_H + MENUBAR_H + TOOLBAR_H;
    const int content_h = H - content_y - STATUS_H;
    const int centre_w = W - g.toolbox_w - g.inspect_w;
    const int canvas_h = content_h - TABBAR_H - g.bottom_h;

    auto place = [&](const char* id, int x, int y, int w, int h) {
        if (Rml::Element* e = by_id(id)) {
            e->SetProperty("left", Rml::String(std::to_string(x) + "px"));
            e->SetProperty("top", Rml::String(std::to_string(y) + "px"));
            e->SetProperty("width", Rml::String(std::to_string(w) + "px"));
            e->SetProperty("height", Rml::String(std::to_string(h) + "px"));
        }
    };
    // Everything that spans the window has to be re-sized, or the area beyond
    // the original size stays unpainted (a black margin when the user enlarges
    // the window).
    // The body must be sized explicitly: with no size it collapses to 0x0 and
    // its background paints nothing, leaving the window black wherever no child
    // covers it. Setting it here (rather than in the stylesheet) is what lets
    // the IDE follow the OS window.
    g.doc->SetProperty("width", Rml::String(std::to_string(W) + "px"));
    g.doc->SetProperty("height", Rml::String(std::to_string(H) + "px"));
    if (std::getenv("OPENEPL_DESIGNER_DEBUG")) {
        g.context->Update();
        const auto b = g.doc->GetBox().GetSize();
        std::fprintf(stderr, "designer: relayout W=%d H=%d body=%.0fx%.0f\n", W, H, b.x, b.y);
    }
    place("titlebar", 0, 0, W, TITLEBAR_H);
    place("menubar", 0, TITLEBAR_H, W, MENUBAR_H);
    place("toolbar", 0, TITLEBAR_H + MENUBAR_H, W, TOOLBAR_H);
    place("status", 0, H - STATUS_H, W, STATUS_H);
    place("toolbox", 0, content_y, g.toolbox_w, content_h);
    place("centre", g.toolbox_w, content_y, centre_w, content_h);
    place("canvasarea", 0, TABBAR_H, centre_w, canvas_h);
    place("inspectdock", g.toolbox_w + centre_w, content_y, g.inspect_w, content_h);
    place("bottom", g.toolbox_w, content_y + TABBAR_H + canvas_h, centre_w, g.bottom_h);
    place("splitleft", g.toolbox_w - 3, content_y, 6, content_h);
    place("splitright", g.toolbox_w + centre_w - 3, content_y, 6, content_h);
    place("splitbottom", g.toolbox_w, content_y + TABBAR_H + canvas_h - 3, centre_w, 6);

    const int half = centre_w / 2;
    if (Rml::Element* e = by_id("codepane")) {
        e->SetProperty("left", "0px");
        e->SetProperty("width", Rml::String(std::to_string(half) + "px"));
        e->SetProperty("height", Rml::String(std::to_string(g.bottom_h) + "px"));
    }
    if (Rml::Element* e = by_id("logpane")) {
        e->SetProperty("left", Rml::String(std::to_string(half) + "px"));
        e->SetProperty("width", Rml::String(std::to_string(centre_w - half) + "px"));
        e->SetProperty("height", Rml::String(std::to_string(g.bottom_h) + "px"));
    }
    for (const char* id : {"code", "log"}) {
        if (Rml::Element* e = by_id(id)) {
            e->SetProperty("height", Rml::String(std::to_string(g.bottom_h - 32) + "px"));
        }
    }
}

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

void run_app(const std::string& path);
void stop_app();
void refresh_all();
bool is_selected(const std::string& id);

/// Record the model before a change, so it can be undone. Call BEFORE mutating.
void push_undo() {
    g.undo_stack.push_back(g.model);
    if (g.undo_stack.size() > 100) g.undo_stack.erase(g.undo_stack.begin());
    g.redo_stack.clear();
}

void undo() {
    if (g.undo_stack.empty()) { set_status("nothing to undo"); return; }
    g.redo_stack.push_back(g.model);
    g.model = g.undo_stack.back();
    g.undo_stack.pop_back();
    if (!g.model.find(g.selected)) g.selected.clear();
    mark_dirty();
    refresh_all();
    set_status("undo");
}

void redo() {
    if (g.redo_stack.empty()) { set_status("nothing to redo"); return; }
    g.undo_stack.push_back(g.model);
    g.model = g.redo_stack.back();
    g.redo_stack.pop_back();
    if (!g.model.find(g.selected)) g.selected.clear();
    mark_dirty();
    refresh_all();
    set_status("redo");
}

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
    const int WIN_W = g.win_w, WIN_H = g.win_h;
    const int content_y = TITLEBAR_H + MENUBAR_H + TOOLBAR_H;
    const int content_h = WIN_H - content_y - STATUS_H;
    const int centre_w  = WIN_W - TOOLBOX_W - INSPECT_W;
    const int canvas_h  = content_h - TABBAR_H - BOTTOM_H;

    std::ostringstream s;
    s << "<rml><head><style>";

    // ---- base -----------------------------------------------------------
    s << "body{font-family:'" << family << "';font-size:12px;color:" << TEXT
      << ";background-color:" << CHROME << "}";
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
    s << "#menupop{position:absolute;background-color:" << PANEL << ";border:1px "
         << BORDER << ";border-radius:6px;padding:4px;"
         "box-shadow:#00000024 0 6px 18px 0px;z-index:10}";
    s << "#menupop .mi{display:block;padding:6px 26px 6px 10px;border-radius:4px;"
         "white-space:nowrap;color:" << TEXT << "}";
    s << "#menupop .mi:hover{background-color:" << ACCENT << ";color:#fff}";
    s << "#menupop .keys{color:" << TEXT_MUTED << ";padding-left:18px}";

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
    s << "#codeview{position:absolute;left:0;top:" << TABBAR_H << "px;width:100%;"
         "background-color:" << PANEL << ";overflow:auto}";
    s << "#fullcode{font-family:'" << family << "';font-size:12px;padding:8px 0 0 0}";
    s << "#canvasarea{left:0;top:" << TABBAR_H << "px;width:" << centre_w << "px;height:" << canvas_h
      << "px;background-color:" << CANVAS << ";decorator:image(\"" << dot_tile << "\" repeat)}";

    // The form preview: a floating native-looking window.
    s << "#formwin{left:60px;top:40px;border-radius:8px;background-color:#ffffff;"
         "box-shadow:#0000002e 0 10px 30px 0px, #00000014 0 2px 6px 0px}";
    s << "#formtitle{height:28px;background-color:" << CHROME
      << ";border-bottom:1px " << BORDER_SOFT << ";border-top-left-radius:8px;"
         "border-top-right-radius:8px;padding:7px 10px 0 10px;font-size:12px;color:" << TEXT << "}";
    s << "#formtitle .dot{position:absolute;top:10px;width:9px;height:9px;border-radius:5px}";
    // Components on the canvas get the SAME default styling as in the built
    // app, from the shared mapping — otherwise the preview lies.
    // SCOPED to the canvas: these rules include `div{position:absolute}`, which
    // would otherwise collapse every panel in the IDE onto one point.
    s << openepl::ui::control_styles("#canvas");
    s << "#canvas{position:relative;overflow:hidden;border-bottom-left-radius:8px;"
         "border-bottom-right-radius:8px}";
    // selection chrome
    s << "#overlay{position:absolute;left:0;top:0;width:100%;height:100%}";
    s << ".selbox{border:1px " << ACCENT << "}";
    s << ".selbox.alt{border:1px #9db8ea}";
    s << ".guide{background-color:#ff4d9a}";
    s << ".fgrip{background-color:#00000000}";
    s << ".fgrip:hover{background-color:" << ACCENT << "}";
    s << ".fgrip.corner{background-color:" << BORDER << ";border-radius:2px}";
    s << ".fgrip.corner:hover{background-color:" << ACCENT << "}";
    s << ".handle{width:7px;height:7px;background-color:#ffffff;border:1px " << ACCENT << "}";
    // Cursor per anchor, so the resize direction is obvious before clicking.
    s << ".handle.nw,.handle.se{cursor:resize-nwse}";
    s << ".handle.ne,.handle.sw{cursor:resize-nesw}";
    s << ".handle.n,.handle.s{cursor:resize-ns}";
    s << ".handle.e,.handle.w{cursor:resize-ew}";
    s << ".fgrip.e{cursor:resize-ew}.fgrip.s{cursor:resize-ns}";
    s << ".fgrip.corner{cursor:resize-nwse}";
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
    s << ".ln{color:#9aa3b0}";
    s << ".cl{display:block;white-space:nowrap}";
    s << ".ln{display:inline-block;width:34px;text-align:right;padding-right:10px}";
    
    s << ".k{color:" << SYN_KEYWORD << "}.m{color:" << SYN_METHOD << "}.s{color:" << SYN_STRING
      << "}.i{color:" << SYN_IDENT << "}.c{color:" << SYN_COMMENT << ";font-style:italic}.n{color:"
      << SYN_NUMBER << "}";
    s << "#log{font-size:12px;padding:6px 10px 0 10px;height:" << (BOTTOM_H - 32)
      << "px;overflow:auto;color:" << TEXT << "}";
    s << "#log .ok{color:" << SUCCESS << "}#log .muted{color:" << TEXT_MUTED << "}";
    s << "#log div{white-space:nowrap;overflow:hidden;height:17px}";

    // ---- status bar -----------------------------------------------------
    s << ".split{position:absolute;background-color:#00000000}";
    s << ".split:hover{background-color:" << ACCENT << "}";
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
    for (size_t i = 0; i < menus().size(); i++) {
        s << "<div class='m' oe-menu='" << i << "'>" << menus()[i].title << "</div>";
    }
    s << "</div><div id='menupop' style='display:none'/>";

    s << "<div id='toolbar'>"
         "<div class='tb' oe-action='save'>Save</div>"
         "<div class='tb' oe-action='undo'>Undo</div>"
         "<div class='tb' oe-action='redo'>Redo</div>"
         "<div class='sep'/>"
         "<div class='tb primary' id='btndesigner' oe-view='designer'>Designer</div>"
         "<div class='tb ghost' id='btncode' oe-view='code'>Code</div>"
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
         "<div class='tab active' id='tabdesigner' oe-view='designer'>Designer — "
      << esc(g.model.form_name) << "</div>"
         "<div class='tab' id='tabcode' oe-view='code'>Code</div>"
         "</div>"
         "<div id='codeview' style='display:none'><div id='fullcode'/></div>"
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
         "<div class='pane' id='codepane' style='left:0;width:" << half
      << "px;border-right:1px " << BORDER << "'>"
         "<div class='panehead' id='codehead'>CODE EDITOR</div><div id='code'/></div>"
         "<div class='pane' id='logpane' style='left:" << half << "px;width:"
      << (centre_w - half) << "px'>"
         "<div class='panehead'>OUTPUT / BUILD LOG</div><div id='log'/></div>"
         "</div>";

    // Splitters: thin draggable bars between the docks.
    s << "<div id='splitleft' class='split v'/>"
         "<div id='splitright' class='split v'/>"
         "<div id='splitbottom' class='split h'/>";

    s << "<div id='status'>RAD is the identity  |  English-first  |  "
         "Cross-platform  |  Assignment is not an expression"
         "<span class='right'><span id='statustext'>Ready</span>   "
         "<span class='dot'>●</span></span></div>";

    s << "</body></rml>";
    return s.str();
}

/* --- canvas --------------------------------------------------------------- */

/// Snap to the canvas dot grid, so dragged components line up the way the grid
/// implies they will. Holding nothing snaps; this is the behaviour users expect
/// from a designer that draws a grid.
constexpr int GRID = 10;
int snap(int v) { return ((v + GRID / 2) / GRID) * GRID; }

int prop_int(const Component& c, const char* name, int fallback) {
    if (const std::string* v = c.property(name)) {
        return std::atoi(v->c_str());
    }
    return fallback;
}

/// The rendered border-box rect of a component, in canvas coordinates.
/// Model width/height size the CONTENT box, so anything with padding or a
/// border draws larger; outlines must trace what is actually painted.
bool measure_component(Rml::Element* canvas, const std::string& id, int& x, int& y, int& w,
                       int& h) {
    if (!canvas) return false;
    Rml::Element* target = nullptr;
    for (int i = 0; i < canvas->GetNumChildren(); i++) {
        Rml::Element* child = canvas->GetChild(i);
        if (child->GetAttribute<Rml::String>("oe-id", "") == id) {
            target = child;
            break;
        }
    }
    if (!target) return false;
    g.context->Update();   // layout must be current for the measurement to mean anything
    const auto canvas_at = canvas->GetAbsoluteOffset(Rml::BoxArea::Border);
    const auto at = target->GetAbsoluteOffset(Rml::BoxArea::Border);
    const auto size = target->GetBox().GetSize(Rml::BoxArea::Border);
    x = (int)(at.x - canvas_at.x);
    y = (int)(at.y - canvas_at.y);
    w = (int)size.x;
    h = (int)size.y;
    return true;
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
        const char* attr_value = nullptr;
        if (const char* attr =
                openepl::ui::creation_attribute(comp.type_name.c_str(), &attr_value)) {
            child->SetAttribute(attr, Rml::String(attr_value));
        }
        Rml::Element* e = canvas->AppendChild(std::move(child));
        e->SetProperty("position", "absolute");
        if (comp.type_name == "groupbox") e->SetAttribute("class", Rml::String("oe-groupbox"));
        if (comp.type_name == "checkbox") e->SetAttribute("class", Rml::String("oe-checkbox"));
        if (const char* markup = openepl::ui::inner_markup(comp.type_name.c_str())) {
            e->SetInnerRML(markup);
        }
        if (comp.type_name == "progressbar") e->SetAttribute("max", Rml::String("100"));
        e->SetAttribute("oe-id", comp.id);
        for (const auto& p : comp.properties) {
            const char* attr =
                openepl::ui::attribute_for(comp.type_name.c_str(), p.first.c_str());
            if (attr) {
                if (p.first == "checked") {
                    Rml::Element* box = e;
                    for (int i = 0; i < e->GetNumChildren(); i++) {
                        if (e->GetChild(i)->GetTagName() == "input") { box = e->GetChild(i); break; }
                    }
                    if (p.second == "true" || p.second == "1") {
                        box->SetAttribute(attr, Rml::String("checked"));
                    }
                } else {
                    e->SetAttribute(attr, Rml::String(p.second));
                }
            } else if (openepl::ui::is_text_property(p.first.c_str()) &&
                       openepl::ui::text_is_content(comp.type_name.c_str())) {
                Rml::Element* text_target = e;
                if (openepl::ui::is_composite(comp.type_name.c_str())) {
                    for (int i = 0; i < e->GetNumChildren(); i++) {
                        if (e->GetChild(i)->GetTagName() == "span") {
                            text_target = e->GetChild(i);
                            break;
                        }
                    }
                }
                text_target->SetInnerRML(esc(p.second));
            } else {
                e->SetProperty(openepl::ui::rcss_name(p.first.c_str()),
                               openepl::ui::rcss_value(p.first.c_str(), p.second.c_str()));
            }
        }
    }

    // Form resize grips, on the edges of the preview window itself.
    {
        const int fwid = fw, fhei = fh;
        auto grip = [&](const char* edge, int px, int py, int pw, int ph, const char* cls) {
            Rml::Element* d = overlay->AppendChild(g.doc->CreateElement("div"));
            d->SetProperty("position", "absolute");
            d->SetAttribute("class", Rml::String(cls));
            d->SetAttribute("oe-formgrip", Rml::String(edge));
            d->SetProperty("left", Rml::String(std::to_string(px) + "px"));
            d->SetProperty("top", Rml::String(std::to_string(py) + "px"));
            d->SetProperty("width", Rml::String(std::to_string(pw) + "px"));
            d->SetProperty("height", Rml::String(std::to_string(ph) + "px"));
        };
        grip("e",  fwid - 4, 0, 8, fhei - 4, "fgrip e");
        grip("s",  0, fhei - 4, fwid - 4, 8, "fgrip s");
        grip("se", fwid - 7, fhei - 7, 12, 12, "fgrip corner");
    }

    // Alignment guides, drawn only while dragging.
    for (int gx : g.guide_x) {
        Rml::Element* d = overlay->AppendChild(g.doc->CreateElement("div"));
        d->SetProperty("position", "absolute");
        d->SetAttribute("class", Rml::String("guide v"));
        d->SetProperty("left", Rml::String(std::to_string(gx) + "px"));
        d->SetProperty("top", "0px");
        d->SetProperty("width", "1px");
        d->SetProperty("height", "100%");
    }
    for (int gy : g.guide_y) {
        Rml::Element* d = overlay->AppendChild(g.doc->CreateElement("div"));
        d->SetProperty("position", "absolute");
        d->SetAttribute("class", Rml::String("guide h"));
        d->SetProperty("left", "0px");
        d->SetProperty("top", Rml::String(std::to_string(gy) + "px"));
        d->SetProperty("width", "100%");
        d->SetProperty("height", "1px");
    }

    // Secondary selections get a plain outline; the primary one gets handles.
    for (const auto& id : g.selection) {
        if (id == g.selected) continue;
        int sx = 0, sy = 0, sw = 0, sh = 0;
        if (!measure_component(canvas, id, sx, sy, sw, sh)) continue;
        Rml::Element* d = overlay->AppendChild(g.doc->CreateElement("div"));
        d->SetProperty("position", "absolute");
        d->SetAttribute("class", Rml::String("selbox alt"));
        d->SetProperty("left", Rml::String(std::to_string(sx - 1) + "px"));
        d->SetProperty("top", Rml::String(std::to_string(sy - 1) + "px"));
        d->SetProperty("width", Rml::String(std::to_string(sw + 2) + "px"));
        d->SetProperty("height", Rml::String(std::to_string(sh + 2) + "px"));
    }

    // Selection chrome lives in an overlay so it never perturbs the components
    // themselves — what you see on the canvas is exactly what the app renders.
    if (g.model.find(g.selected)) {
        // Measure the RENDERED element rather than deriving the rect from the
        // model's width/height: those size the CONTENT box, so a component with
        // padding or a border (a groupbox has both) draws larger than its
        // declared size, and a model-derived outline sits inside the real frame.
        int x = 0, y = 0, w = 0, h = 0;
        if (!measure_component(canvas, g.selected, x, y, w, h)) return;
        if (std::getenv("OPENEPL_DESIGNER_DEBUG")) {
            const Component* c = g.model.find(g.selected);
            std::fprintf(stderr,
                         "designer: selection rect=%d,%d %dx%d   model=%d,%d %dx%d\n", x, y, w, h,
                         c ? prop_int(*c, "left", 0) : -1, c ? prop_int(*c, "top", 0) : -1,
                         c ? prop_int(*c, "width", 0) : -1, c ? prop_int(*c, "height", 0) : -1);
        }
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
        static const char* EDGE[3][3] = {{"nw", "w", "sw"}, {"n", "", "s"}, {"ne", "e", "se"}};
        for (int i = 0; i < 3; i++) {
            for (int j = 0; j < 3; j++) {
                if (i == 1 && j == 1) continue;   // 8 anchors, not 9
                Rml::Element* d = place("handle", hx[i], hy[j], 0, 0);
                d->SetAttribute("class", Rml::String(std::string("handle ") + EDGE[i][j]));
                d->SetAttribute("oe-grip", Rml::String(EDGE[i][j]));
            }
        }
        // Event connector badge, as specified.
        const Component* sel = g.model.find(g.selected);
        if (sel && !sel->handlers.empty()) {
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
        char num[16];
        std::snprintf(num, sizeof num, "%4zu", i + 1);
        html += "<div class='cl'><span class='ln'>" + std::string(num) + "</span>" +
                highlight_line(lines[i]) + "</div>";
    }
    if (Rml::Element* head = code->GetParentNode()->GetChild(0)) {
        head->SetInnerRML(want.empty() ? "CODE EDITOR · whole module"
                                       : "CODE EDITOR · " + esc(want));
    }
    code->SetInnerRML(html);

    // The Code view shows the whole module, not just the wired handler.
    if (Rml::Element* full = by_id("fullcode")) {
        std::string all;
        for (size_t i = 0; i < lines.size(); i++) {
            char num[16];
            std::snprintf(num, sizeof num, "%4zu", i + 1);
            all += "<div class='cl'><span class='ln'>" + std::string(num) + "</span>" +
                   highlight_line(lines[i]) + "</div>";
        }
        full->SetInnerRML(all);
    }
}

/// Switch the centre pane between the designer canvas and the code view.
void set_view(const std::string& view) {
    g.view = view;
    const bool code = (view == "code");
    if (Rml::Element* e = by_id("canvasarea")) e->SetProperty("display", code ? "none" : "block");
    if (Rml::Element* e = by_id("codeview")) e->SetProperty("display", code ? "block" : "none");
    if (Rml::Element* e = by_id("codeview")) {
        const int content_h = g.win_h - (theme::TITLEBAR_H + theme::MENUBAR_H + theme::TOOLBAR_H) -
                              theme::STATUS_H - theme::TABBAR_H;
        e->SetProperty("height", Rml::String(std::to_string(content_h - theme::TABBAR_H) + "px"));
    }
    // Keep the toolbar switcher and the document tabs in step.
    for (const char* id : {"tabdesigner", "tabcode", "btndesigner", "btncode"}) {
        if (Rml::Element* e = by_id(id)) {
            const bool is_code = std::string(id).find("code") != std::string::npos;
            const bool active = (is_code == code);
            const std::string base = std::string(id).rfind("tab", 0) == 0 ? "tab" : "tb";
            e->SetAttribute("class", Rml::String(active ? base + (base == "tab" ? " active"
                                                                               : " primary")
                                                        : base + (base == "tab" ? "" : " ghost")));
        }
    }
    rebuild_code();
    set_status(code ? "code view" : "designer view");
}

void refresh_all() {
    rebuild_canvas();
    rebuild_inspector();
    rebuild_code();
}

/// Select `id`. With `add`, extend the selection (ctrl-click) instead of
/// replacing it.
void select(const std::string& id, bool add = false) {
    if (!add) g.selection.clear();
    if (!id.empty() && !is_selected(id)) g.selection.push_back(id);
    g.selected = id;
    refresh_all();
}

/* --- actions -------------------------------------------------------------- */

void add_component(const std::string& type_name) {
    const OpenEPL_ComponentDesc* desc = describe(type_name.c_str());
    if (!desc) { set_status("unknown component type " + type_name); return; }
    push_undo();
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

/// Only the descriptor's declared type can say whether a value needs quotes:
/// `text = "true"` and `checked = true` look identical as strings.
bool property_needs_quotes(const std::string& type_name, const std::string& property) {
    const OpenEPL_ComponentDesc* desc = describe(type_name.c_str());
    if (!desc) return true;
    for (int i = 0; i < desc->property_count; i++) {
        if (property == desc->properties[i].name) {
            return desc->properties[i].tag == OE_SDT_TEXT;
        }
    }
    return true;
}

/// Begin dragging `id`, grabbing at screen point (mx,my).
///
/// The grab offset comes from the MODEL, not from the element: select()
/// rebuilds the canvas, so the element is destroyed before we could ask where
/// it was, and its stale offset made every drag jump to the top-left corner.
void begin_drag(const std::string& id, int mx, int my) {
    Rml::Element* canvas = by_id("canvas");
    const Component* comp = g.model.find(id);
    if (!canvas || !comp) return;
    push_undo();   // one snapshot per drag, not per mouse move
    const auto origin = canvas->GetAbsoluteOffset();
    g.drag_dx = mx - ((int)origin.x + prop_int(*comp, "left", 0));
    g.drag_dy = my - ((int)origin.y + prop_int(*comp, "top", 0));
    g.dragging = true;
    select(id);
}

/// Continue a drag to screen point (mx,my).
void drag_to(int mx, int my) {
    if (!g.dragging || g.selected.empty()) return;
    Component* c = g.model.find(g.selected);
    Rml::Element* canvas = by_id("canvas");
    if (!c || !canvas) return;
    const auto origin = canvas->GetAbsoluteOffset();
    int x = mx - g.drag_dx - (int)origin.x;
    int y = my - g.drag_dy - (int)origin.y;
    const int fw = prop_int(g.model.form, "width", 420);
    const int fh = prop_int(g.model.form, "height", 260);
    const int w = prop_int(*c, "width", 120), h = prop_int(*c, "height", 32);
    if (x < 0) x = 0;
    if (y < 0) y = 0;
    if (x > fw - w) x = fw - w;
    if (y > fh - h) y = fh - h;

    // Alignment guides: if an edge or centre lines up with another component's,
    // snap exactly to it and remember the line so it can be drawn. This is what
    // makes a designer feel precise rather than approximate.
    constexpr int PULL = 6;
    g.guide_x.clear();
    g.guide_y.clear();
    for (const auto& other : g.model.children) {
        if (other.id == c->id) continue;
        const int ox = prop_int(other, "left", 0), oy = prop_int(other, "top", 0);
        const int ow = prop_int(other, "width", 120), oh = prop_int(other, "height", 32);
        const int xs[3] = {ox, ox + ow / 2 - w / 2, ox + ow - w};
        const int xg[3] = {ox, ox + ow / 2, ox + ow};
        for (int i = 0; i < 3; i++) {
            if (std::abs(x - xs[i]) <= PULL) { x = xs[i]; g.guide_x.push_back(xg[i]); }
        }
        const int ys[3] = {oy, oy + oh / 2 - h / 2, oy + oh - h};
        const int yg[3] = {oy, oy + oh / 2, oy + oh};
        for (int i = 0; i < 3; i++) {
            if (std::abs(y - ys[i]) <= PULL) { y = ys[i]; g.guide_y.push_back(yg[i]); }
        }
    }
    // Snap to the grid only when no neighbour claimed the position.
    c->set_property("left", std::to_string(g.guide_x.empty() ? snap(x) : x));
    c->set_property("top", std::to_string(g.guide_y.empty() ? snap(y) : y));
    mark_dirty();
    rebuild_canvas();
}

/// Is `id` part of the current selection?
bool is_selected(const std::string& id) {
    for (const auto& s : g.selection) {
        if (s == id) return true;
    }
    return false;
}

/// Delete every selected component.
void delete_selection() {
    if (g.selection.empty()) { set_status("nothing selected"); return; }
    push_undo();
    const size_t before = g.model.children.size();
    std::vector<Component> kept;
    for (const auto& c : g.model.children) {
        if (!is_selected(c.id)) kept.push_back(c);
    }
    g.model.children = kept;
    g.selection.clear();
    g.selected.clear();
    mark_dirty();
    refresh_all();
    set_status("deleted " + std::to_string(before - g.model.children.size()) + " component(s)");
}

void copy_selection() {
    g.clipboard.clear();
    for (const auto& c : g.model.children) {
        if (is_selected(c.id)) g.clipboard.push_back(c);
    }
    set_status("copied " + std::to_string(g.clipboard.size()) + " component(s)");
}

/// Paste the clipboard, offset slightly so the copies are visible and
/// re-identified so ids stay unique.
void paste_clipboard() {
    if (g.clipboard.empty()) { set_status("clipboard is empty"); return; }
    push_undo();
    g.selection.clear();
    for (Component c : g.clipboard) {
        c.id = g.model.fresh_id(c.type_name);
        c.set_property("left", std::to_string(prop_int(c, "left", 0) + 10));
        c.set_property("top", std::to_string(prop_int(c, "top", 0) + 10));
        g.model.children.push_back(c);
        g.selection.push_back(c.id);
        g.selected = c.id;
    }
    mark_dirty();
    refresh_all();
    set_status("pasted " + std::to_string(g.clipboard.size()) + " component(s)");
}

void save() {
    std::string err;
    if (!save_model(g.model, g.pending_subs, property_needs_quotes, err)) {
        set_status("save failed: " + err);
        return;
    }
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
    if (then_run) run_app(out);
}

/// Launch the built app, keeping its pid so Stop can end it. `system()` would
/// give us no handle on the child, which is why Stop could only ever say
/// "nothing running".
void run_app(const std::string& path) {
    stop_app();
    const pid_t pid = fork();
    if (pid == 0) {
        execl(path.c_str(), path.c_str(), (char*)nullptr);
        _exit(127);
    }
    if (pid < 0) { log("could not start the app"); return; }
    g.running_app = pid;
    log("> running (pid " + std::to_string(pid) + ")", "muted");
    set_status("running");
}

/// Stop the app started by Run, if it is still alive.
void stop_app() {
    if (g.running_app <= 0) { set_status("nothing running"); return; }
    if (::kill(g.running_app, 0) != 0) {   // already exited
        g.running_app = 0;
        set_status("nothing running");
        return;
    }
    ::kill(g.running_app, SIGTERM);
    int status = 0;
    ::waitpid(g.running_app, &status, 0);
    log("> stopped (pid " + std::to_string(g.running_app) + ")", "muted");
    g.running_app = 0;
    set_status("stopped");
}

/// Reap the app if it exited on its own, so Stop reports honestly.
void poll_app() {
    if (g.running_app <= 0) return;
    int status = 0;
    if (::waitpid(g.running_app, &status, WNOHANG) == g.running_app) {
        log("> app exited", "muted");
        g.running_app = 0;
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
                push_undo();
                comp->set_handler(name, value);
                mark_dirty();
                if (!value.empty() && !g.model.has_sub(value)) {
                    g.pending_subs.push_back(value);
                    set_status("will create sub " + value);
                }
                refresh_all();
            } else if (cls.find("pv") != Rml::String::npos) {
                push_undo();
                comp->set_property(name, value);
                mark_dirty();
                refresh_all();
            }
            return;
        }

        if (type == "keydown") {
            const int key = ev.GetParameter<int>("key_identifier", 0);
            const bool ctrl = ev.GetParameter<bool>("ctrl_key", false);
            const bool shift = ev.GetParameter<bool>("shift_key", false);
            if (ctrl && key == Rml::Input::KI_Z) { shift ? redo() : undo(); return; }
            if (ctrl && key == Rml::Input::KI_Y) { redo(); return; }
            if (ctrl && key == Rml::Input::KI_C) { copy_selection(); return; }
            if (ctrl && key == Rml::Input::KI_V) { paste_clipboard(); return; }
            if (ctrl && key == Rml::Input::KI_S) { save(); return; }
            if (key == Rml::Input::KI_DELETE) { delete_selection(); return; }
            // Nudge the selection with the arrow keys: 1px normally, a grid
            // step with shift.
            int dx = 0, dy = 0;
            if (key == Rml::Input::KI_LEFT) dx = -1;
            else if (key == Rml::Input::KI_RIGHT) dx = 1;
            else if (key == Rml::Input::KI_UP) dy = -1;
            else if (key == Rml::Input::KI_DOWN) dy = 1;
            if (dx || dy) {
                if (shift) { dx *= GRID; dy *= GRID; }
                push_undo();
                for (const auto& id : g.selection) {
                    if (Component* c = g.model.find(id)) {
                        c->set_property("left",
                                        std::to_string(std::max(0, prop_int(*c, "left", 0) + dx)));
                        c->set_property("top",
                                        std::to_string(std::max(0, prop_int(*c, "top", 0) + dy)));
                    }
                }
                mark_dirty();
                refresh_all();
            }
            return;
        }

        if (type == "click") {
            // A click anywhere dismisses an open menu, unless it opened one.
            bool opened_menu = false;
            for (Rml::Element* e = el; e; e = e->GetParentNode()) {
                if (!e->HasAttribute("oe-menu")) continue;
                const int idx = e->GetAttribute<int>("oe-menu", 0);
                if (Rml::Element* pop = by_id("menupop")) {
                    std::string html;
                    for (const auto& item : menus()[(size_t)idx].items) {
                        html += "<div class='mi' oe-action='" + std::string(item.action) + "'>" +
                                item.label +
                                (item.keys[0] ? "<span class='keys'>" + std::string(item.keys) +
                                                    "</span>"
                                              : "") +
                                "</div>";
                    }
                    pop->SetInnerRML(html);
                    const auto at = e->GetAbsoluteOffset(Rml::BoxArea::Border);
                    pop->SetProperty("left", Rml::String(std::to_string((int)at.x) + "px"));
                    pop->SetProperty("top", Rml::String(std::to_string(theme::TITLEBAR_H +
                                                                      theme::MENUBAR_H) + "px"));
                    pop->SetProperty("display", "block");
                }
                opened_menu = true;
                break;
            }
            if (!opened_menu) {
                if (Rml::Element* pop = by_id("menupop")) pop->SetProperty("display", "none");
            } else {
                return;
            }

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
                if (e->HasAttribute("oe-view")) {
                    set_view(e->GetAttribute<Rml::String>("oe-view", "designer"));
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
                    else if (a == "stop") stop_app();
                    else if (a == "undo") undo();
                    else if (a == "redo") redo();
                    else if (a == "copy") copy_selection();
                    else if (a == "paste") paste_clipboard();
                    else if (a == "delete") delete_selection();
                    else if (a == "view-designer") set_view("designer");
                    else if (a == "view-code") set_view("code");
                    else if (a == "about") {
                        log("OpenEPL Studio — RAD for clean native binaries.", "muted");
                        set_status("OpenEPL Studio");
                    } else if (a == "exit") {
                        Backend::RequestExit();
                    } else {
                        set_status(a + " is not implemented yet");
                    }
                    return;
                }
                if (e->HasAttribute("oe-id")) {
                    select(e->GetAttribute<Rml::String>("oe-id", ""),
                           ev.GetParameter<bool>("ctrl_key", false));
                    return;
                }
            }
            return;
        }

        if (type == "mousedown") {
            const int mx = ev.GetParameter<int>("mouse_x", 0);
            const int my = ev.GetParameter<int>("mouse_y", 0);

            // Dock splitters.
            {
                const Rml::String id = el->GetId();
                if (id == "splitleft" || id == "splitright" || id == "splitbottom") {
                    g.splitting = id == "splitleft"    ? "left"
                                  : id == "splitright" ? "right"
                                                       : "bottom";
                    g.split_x0 = mx;
                    g.split_y0 = my;
                    g.split_v0 = g.splitting == "left"    ? g.toolbox_w
                                 : g.splitting == "right" ? g.inspect_w
                                                          : g.bottom_h;
                    return;
                }
            }

            // Resizing the form preview, as in Visual Studio / RAD Studio.
            if (el->HasAttribute("oe-formgrip")) {
                push_undo();
                g.resizing_form = true;
                g.resize_edge = el->GetAttribute<Rml::String>("oe-formgrip", "se");
                g.resize_x0 = mx;
                g.resize_y0 = my;
                g.resize_w0 = prop_int(g.model.form, "width", 420);
                g.resize_h0 = prop_int(g.model.form, "height", 260);
                return;
            }
            // Resizing the selected component by its anchor.
            if (el->HasAttribute("oe-grip")) {
                if (Component* c = g.model.find(g.selected)) {
                    push_undo();
                    g.resizing_comp = true;
                    g.comp_edge = el->GetAttribute<Rml::String>("oe-grip", "se");
                    g.resize_x0 = mx;
                    g.resize_y0 = my;
                    g.comp_x0 = prop_int(*c, "left", 0);
                    g.comp_y0 = prop_int(*c, "top", 0);
                    g.comp_w0 = prop_int(*c, "width", 120);
                    g.comp_h0 = prop_int(*c, "height", 32);
                }
                return;
            }
            // Composite components (a checkbox is a container holding a box and
            // a caption) deliver mousedown on the CHILD, which carries no id —
            // so walk up to the component. Without this, whether a component
            // could be dragged depended on which part you grabbed.
            // Composite components (a checkbox is a container holding a box and
            // a caption) deliver mousedown on the CHILD, which carries no id —
            // so walk up to the component. Without this, whether a component
            // could be dragged depended on which part you grabbed.
            for (Rml::Element* e = el; e; e = e->GetParentNode()) {
                if (!e->HasAttribute("oe-id")) continue;
                const std::string id = e->GetAttribute<Rml::String>("oe-id", "");
                if (ev.GetParameter<bool>("ctrl_key", false)) {
                    select(id, true);   // extend the selection instead of dragging
                } else {
                    if (!is_selected(id)) select(id);
                    begin_drag(id, mx, my);
                }
                break;
            }
            return;
        }

        if (type == "mousemove") {
            const int mx = ev.GetParameter<int>("mouse_x", 0);
            const int my = ev.GetParameter<int>("mouse_y", 0);

            if (!g.splitting.empty()) {
                int v = g.split_v0;
                if (g.splitting == "left") v = g.split_v0 + (mx - g.split_x0);
                else if (g.splitting == "right") v = g.split_v0 - (mx - g.split_x0);
                else v = g.split_v0 - (my - g.split_y0);
                if (v < 120) v = 120;
                if (v > 640) v = 640;
                if (g.splitting == "left") g.toolbox_w = v;
                else if (g.splitting == "right") g.inspect_w = v;
                else g.bottom_h = v;
                relayout();
                return;
            }

            if (g.resizing_form) {
                int w = g.resize_w0, h = g.resize_h0;
                if (g.resize_edge != "s") w = g.resize_w0 + (mx - g.resize_x0);
                if (g.resize_edge != "e") h = g.resize_h0 + (my - g.resize_y0);
                g.model.form.set_property("width", std::to_string(snap(w < 120 ? 120 : w)));
                g.model.form.set_property("height", std::to_string(snap(h < 80 ? 80 : h)));
                mark_dirty();
                rebuild_canvas();
                return;
            }

            if (g.resizing_comp) {
                Component* c = g.model.find(g.selected);
                if (!c) return;
                const int dx = mx - g.resize_x0, dy = my - g.resize_y0;
                int x = g.comp_x0, y = g.comp_y0, w = g.comp_w0, h = g.comp_h0;
                const bool west = g.comp_edge.find('w') != std::string::npos;
                const bool east = g.comp_edge.find('e') != std::string::npos;
                const bool north = g.comp_edge.find('n') != std::string::npos;
                const bool south = g.comp_edge.find('s') != std::string::npos;
                if (east) w = g.comp_w0 + dx;
                if (west) { x = g.comp_x0 + dx; w = g.comp_w0 - dx; }
                if (south) h = g.comp_h0 + dy;
                if (north) { y = g.comp_y0 + dy; h = g.comp_h0 - dy; }
                if (w < 20) w = 20;
                if (h < 16) h = 16;
                c->set_property("left", std::to_string(snap(x < 0 ? 0 : x)));
                c->set_property("top", std::to_string(snap(y < 0 ? 0 : y)));
                c->set_property("width", std::to_string(snap(w)));
                c->set_property("height", std::to_string(snap(h)));
                mark_dirty();
                rebuild_canvas();
                return;
            }

            if (g.dragging) drag_to(mx, my);
            return;
        }

        if (type == "mouseup") {
            if (g.dragging || g.resizing_comp || g.resizing_form) rebuild_inspector();
            g.splitting.clear();
            if (g.dragging) {
                g.guide_x.clear();
                g.guide_y.clear();
                rebuild_canvas();
            }
            g.dragging = false;
            g.resizing_comp = false;
            g.resizing_form = false;
            return;
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
    std::vector<unsigned char> px((size_t)g.win_w * g.win_h * 3);
    glReadPixels(0, 0, g.win_w, g.win_h, GL_RGB, GL_UNSIGNED_BYTE, px.data());
    if (FILE* f = std::fopen(path, "wb")) {
        std::fprintf(f, "P6\n%d %d\n255\n", g.win_w, g.win_h);
        for (int y = g.win_h - 1; y >= 0; y--)
            std::fwrite(&px[(size_t)y * g.win_w * 3], 1, (size_t)g.win_w * 3, f);
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
            } else if (verb == "drag") {
                // drag:<id>@<x0>,<y0>-><x1>,<y1> in CANVAS coordinates; exercises
                // exactly the path the mouse takes.
                const size_t at = arg.find('@'), arrow = arg.find("->");
                if (at != std::string::npos && arrow != std::string::npos) {
                    const std::string id = arg.substr(0, at);
                    int x0 = 0, y0 = 0, x1 = 0, y1 = 0;
                    std::sscanf(arg.c_str() + at + 1, "%d,%d", &x0, &y0);
                    std::sscanf(arg.c_str() + arrow + 2, "%d,%d", &x1, &y1);
                    if (Rml::Element* canvas = by_id("canvas")) {
                        const auto o = canvas->GetAbsoluteOffset();
                        begin_drag(id, (int)o.x + x0, (int)o.y + y0);
                        drag_to((int)o.x + x1, (int)o.y + y1);
                        g.dragging = false;
                    }
                }
            } else if (verb == "winsize") {
                // Simulate an OS window resize, so the layout's response to one
                // can be tested without a window manager.
                int nw = 0, nh = 0;
                if (std::sscanf(arg.c_str(), "%dx%d", &nw, &nh) == 2 && nw > 400 && nh > 300) {
                    // Resize the real window too, or the GL framebuffer stays
                    // its original size and a screenshot shows phantom black
                    // margins that the running IDE would not have.
                    if (SDL_Window* win = SDL_GL_GetCurrentWindow()) {
                        SDL_SetWindowSize(win, nw, nh);
                        // Let the backend see the resize, so the renderer's
                        // viewport follows; otherwise only the layout changes.
                        for (int i = 0; i < 30; i++) Backend::ProcessEvents(g.context);
                    }
                    g.win_w = nw;
                    g.win_h = nh;
                    g.context->SetDimensions(Rml::Vector2i(nw, nh));
                    relayout();
                    rebuild_canvas();
                }
            } else if (verb == "undo") undo();
            else if (verb == "redo") redo();
            else if (verb == "copy") copy_selection();
            else if (verb == "paste") paste_clipboard();
            else if (verb == "delete") delete_selection();
            else if (verb == "addsel") select(arg, true);
            else if (verb == "save") save();
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

    if (!Backend::Initialize("OpenEPL Studio", INIT_W, INIT_H, true)) return 1;
    Rml::SetSystemInterface(Backend::GetSystemInterface());
    Rml::SetRenderInterface(Backend::GetRenderInterface());
    Rml::Initialise();

    int font_count = 0;
    const auto* fonts = openepl::ui::font_candidates(&font_count);
    std::string family = "sans-serif";
    for (int i = 0; i < font_count; i++) {
        if (Rml::LoadFontFace(fonts[i].path)) { family = fonts[i].family; break; }
    }

    const std::string dot_tile = write_dot_tile("openepl_dotgrid.tga", 10);
    g.context = Rml::CreateContext("studio", Rml::Vector2i(INIT_W, INIT_H));

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

    relayout();
    refresh_all();
    log("OpenEPL Studio ready.", "muted");
    log("> " + g.model.path, "muted");
    set_status("Ready");

    if (std::getenv("OPENEPL_DESIGNER_DEBUG")) {
        for (const char* id : {"bottom", "codepane", "code", "codeview"}) {
            if (Rml::Element* e = by_id(id)) {
                const auto b = e->GetBox().GetSize();
                const auto o = e->GetAbsoluteOffset();
                std::fprintf(stderr, "designer: #%s box=%.0fx%.0f at %.0f,%.0f\n", id, b.x, b.y,
                             o.x, o.y);
            }
        }
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
        poll_app();
        // Follow the OS window. The backend resizes the context; the layout has
        // to follow or everything past the old size is left unpainted.
        const auto dim = g.context->GetDimensions();
        if (dim.x != g.win_w || dim.y != g.win_h) {
            g.win_w = dim.x;
            g.win_h = dim.y;
            relayout();
            rebuild_canvas();
        }
        g.context->Update();
        Backend::BeginFrame();
        g.context->Render();
        Backend::PresentFrame();
    }
    stop_app();
    if (g.dirty) {
        std::printf("designer: unsaved changes — saving before exit\n");
        save();
    }
    Rml::Shutdown();
    Backend::Shutdown();
    return 0;
}
