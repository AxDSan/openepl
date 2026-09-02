/* Headless tests for the Studio editor and the RAD gestures.
 *
 * Two halves. The catalogue half reads canned `openepl commands` listings, so
 * a kit's visual component can be checked without a kit that has one
 * installed. The session half drives the BUILT designer through scripted
 * sessions and reads what its verbs print — the cursor the platform was
 * asked for, the handler a double-click wrote, the tip a hover produced —
 * because every one of those passed a unit test and failed on screen once.
 *
 * Build and run (needs a display, as the frame dumps do):
 *
 *   clang++ -std=c++17 -I abi -I designer designer/test_studio.cpp \
 *       libs/ui/ui_libinfo.c -o /tmp/openepl_studio_test
 *   /tmp/openepl_studio_test ./target/debug/openepl designer/openepl-designer
 */
#include <algorithm>
#include <cstdio>
#include <cstdlib>
#include <fstream>
#include <unistd.h>
#include <sstream>
#include <string>
#include <vector>

#include "catalog.h"

using namespace openepl::designer;

static int failures = 0;
static void check(const char* what, bool ok) {
    std::printf("  %-64s %s\n", what, ok ? "PASS" : "*** FAIL ***");
    if (!ok) failures++;
}

static std::string slurp(const std::string& p) {
    std::ifstream f(p);
    std::ostringstream o;
    o << f.rdbuf();
    return o.str();
}

static bool has(const std::string& hay, const std::string& needle) {
    return hay.find(needle) != std::string::npos;
}

/// Run one scripted session on a fresh copy of `fixture` and return its
/// stdout. The copy is what the session saves into on exit.
static std::string session(const std::string& designer, const std::string& openepl,
                           const std::string& fixture, const std::string& script,
                           std::string* saved_path = nullptr) {
    static int n = 0;
    const std::string copy = "/tmp/openepl_studio_test_" + std::to_string(++n) + ".oir";
    { std::ofstream f(copy, std::ios::trunc); f << slurp(fixture); }
    if (saved_path) *saved_path = copy;
    const std::string cmd = "OPENEPL_DESIGNER_SCRIPT='" + script + "' " + designer + " " + copy +
                            " " + openepl + " 2>/dev/null";
    std::string out;
    if (FILE* p = popen(cmd.c_str(), "r")) {
        char buf[4096];
        while (fgets(buf, sizeof buf, p)) out += buf;
        pclose(p);
    }
    return out;
}

static void test_catalog() {
    std::printf("catalogue\n");
    using namespace catalog_detail;
    // A kit's own visual control, as `openepl commands --use gauges` would
    // list it — a component this build was never linked against.
    const std::vector<std::string> lines = {
        "command: gauge_set(text, int)",
        "component: gauge",
        "kind: gauge visual",
        "property: gauge value int",
        "property: gauge color text",
        "editor: gauge color color",
        "property: gauge left int",
        "event: gauge change",
        "component: beacon",
        "kind: beacon nonvisual",
        "property: beacon port int",
        "event: beacon ping",
        "component: relic",
        "property: relic name text",
    };
    const auto comps = parse_components(lines, "gauges");
    check("three components read", comps.size() == 3);
    check("kind: visual is read", comps[0].visual && comps[0].kind_known);
    check("kind: nonvisual is read", !comps[1].visual && comps[1].kind_known);
    check("no kind: line leaves the kind unknown", !comps[2].kind_known && !comps[2].visual);
    check("editor: hint lands on its property",
          comps[0].props.size() == 3 && comps[0].props[1].editor == "color" &&
              comps[0].props[0].editor.empty());
    check("events are read, parameters unknown",
          comps[0].events.size() == 1 && comps[0].events[0].name == "change" &&
              !comps[0].events[0].known);
    check("kit is recorded", comps[0].kit == "gauges");

    // The linked table fills in what the listing cannot say, and does not
    // overrule what it did say.
    CatalogComponent grid;
    grid.type_name = "grid";
    grid.kind_known = true;
    grid.visual = false;          // a listing that disagrees with the table wins
    grid.events.push_back({"select", false, {}});
    grid.events.push_back({"activate", false, {}});
    grid.props.push_back({"rows", "text", "", "", false});
    enrich_from_libinfo(grid);
    check("listing's kind is kept over the linked table", !grid.visual);
    check("event parameters come from the linked table",
          grid.events[0].known && grid.events[0].params.size() == 1 &&
              grid.events[0].params[0] == "int");
    check("editor hint comes from the linked table", grid.props[0].editor == "multiline");

    CatalogComponent button;
    button.type_name = "button";
    button.events.push_back({"click", false, {}});
    enrich_from_libinfo(button);
    check("no kind: line falls back to the linked table", button.visual);
    check("an event with nothing to hand over is known, with no parameters",
          button.events[0].known && button.events[0].params.empty());
}

static void test_sessions(const std::string& openepl, const std::string& designer) {
    std::printf("sessions (%s)\n", designer.c_str());
    const std::string form = "examples/form.oir";
    const std::string grid = "examples/grid.oir";

    // The resize cursor: every anchor names its direction, and the platform
    // is asked for exactly that.
    {
        const std::string out =
            session(designer, openepl, form,
                    "select:ok_button;hovergrip:se;hovergrip:n;hovergrip:e;hovergrip:ne;"
                    "hovergrip:s;hovergrip:nw;hovergrip:sw;hovergrip:w");
        check("corner anchor asks for resize-nwse", has(out, "cursor: se resize-nwse"));
        check("opposite corner asks for resize-nwse", has(out, "cursor: nw resize-nwse"));
        check("other corners ask for resize-nesw",
              has(out, "cursor: ne resize-nesw") && has(out, "cursor: sw resize-nesw"));
        check("top and bottom anchors ask for resize-ns",
              has(out, "cursor: n resize-ns") && has(out, "cursor: s resize-ns"));
        check("side anchors ask for resize-ew",
              has(out, "cursor: e resize-ew") && has(out, "cursor: w resize-ew"));
    }

    // Double-click on a component with a handler: just go there.
    {
        const std::string out = session(designer, openepl, form, "dblclick:ok_button;caret");
        check("existing handler: switches to the code view", has(out, "designer: code view"));
        check("existing handler: caret inside on_ok_click", has(out, "caret: 39,"));
        check("existing handler: nothing was wired", !has(out, "designer: wired"));
    }

    // Double-click on a fresh component: the default event gets a handler
    // with the event's parameters, and the caret lands in it.
    {
        std::string path;
        const std::string out =
            session(designer, openepl, form, "add:grid;dblclick:grid1;caret", &path);
        const std::string file = slurp(path);
        check("grid: wires select, the descriptor's first event",
              has(out, "wired grid1.select to grid1_select"));
        check("grid: the handler line is in the component block",
              has(file, "    on select: grid1_select\n"));
        check("grid: the stub takes the row the event hands over",
              has(file, "\nsub grid1_select(n: int)\n  \nend\n"));
        check("grid: the hand-written handler survived", has(file, "sub on_ok_click\n  call print_text"));
        // The blank line between `sub` and `end`, which is the last line but one.
        const long lines = std::count(file.begin(), file.end(), '\n');
        check("grid: caret is inside the new sub", has(out, "caret: " + std::to_string(lines - 1) + ","));
    }
    {
        std::string path;
        const std::string out =
            session(designer, openepl, form, "add:editbox;dblclick:editbox1", &path);
        check("editbox: change, with no parameters",
              has(out, "wired editbox1.change") && has(slurp(path), "\nsub editbox1_change\n"));
    }
    // A component from the tray, whose descriptor this build never linked:
    // the parameters come from the language server.
    {
        std::string path;
        const std::string out = session(designer, openepl, form, "add:timer;dblclick:timer1", &path);
        check("timer: tick is wired from the tray", has(out, "wired timer1.tick to timer1_tick"));
        check("timer: the stub takes the tick count",
              has(slurp(path), "\nsub timer1_tick(n: int)\n  \nend\n"));
    }
    {
        const std::string out = session(designer, openepl, grid, "dblclick:people");
        check("a component with no events says so", has(out, "datasource has no events"));
    }

    // Hover, definition and references through the language server.
    {
        const std::string out = session(designer, openepl, form, "hoverat:39,10");
        check("hover on a command shows its signature", has(out, "tip=block") && has(out, "print_text(text)"));
    }
    {
        const std::string out = session(designer, openepl, grid, "gotodef:104,20;refs:96,7");
        check("F12 on a local jumps to its declaration", has(out, "definition: caret 102,7"));
        check("Shift+F12 lists the wiring line and the declaration",
              has(out, "references: 2") && has(out, "line 49:18") && has(out, "line 96:5"));
    }

    // Diagnostics carry columns, and a scrolled editor keeps them on the row.
    {
        std::string bad = slurp(form);
        const size_t at = bad.find("print_text(\"button");
        bad.replace(at, 10, "print_txt");
        const std::string fixture = "/tmp/openepl_studio_test_bad.oir";
        { std::ofstream f(fixture, std::ios::trunc); f << bad; }
        const std::string out = session(designer, openepl, fixture, "view:code;waitdiag;scroll:6;waitdiag:1");
        check("the diagnostic names the line", has(out, "line 39: in `on_ok_click`"));
    }
}

int main(int argc, char** argv) {
    const std::string openepl = argc > 1 ? argv[1] : "./target/debug/openepl";
    const std::string designer = argc > 2 ? argv[2] : "designer/openepl-designer";
    test_catalog();
    if (::access(designer.c_str(), X_OK) == 0) {
        test_sessions(openepl, designer);
    } else {
        std::printf("sessions: %s not built, skipped\n", designer.c_str());
    }
    std::printf("%s\n", failures ? "FAILED" : "all passed");
    return failures ? 1 : 0;
}
