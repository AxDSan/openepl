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
#include <sys/stat.h>
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

    // The designer writes only what the descriptor declares. Through a CLI
    // whose listing omits the label's height — the build refused exactly
    // that property once — the vertical anchors must go, and a resize must
    // leave the block without it.
    {
        const std::string wrapper = "/tmp/openepl_studio_test_nolabelheight.sh";
        {
            std::ofstream f(wrapper, std::ios::trunc);
            f << "#!/bin/sh\nif [ \"$1\" = commands ]; then\n  \"" << openepl
              << "\" \"$@\" | grep -v '^property: label height'\n  exit 0\nfi\nexec \"" << openepl
              << "\" \"$@\"\n";
        }
        ::chmod(wrapper.c_str(), 0755);
        std::string path;
        const std::string out = session(designer, wrapper, form,
                                        "select:greeting;grip:s@0,30;grip:e@30,0;save", &path);
        check("no height: the vertical anchor is not offered", has(out, "grip: s (no such grip)"));
        check("no height: the side anchor still resizes", has(out, "grip: e dragged") && has(out, "width=430"));
        const std::string file = slurp(path);
        const size_t block = file.find("label greeting");
        const size_t block_end = file.find("end", block);
        check("no height: the resize wrote no height into the block",
              block != std::string::npos && file.substr(block, block_end - block).find("height") == std::string::npos);
        check("no height: the width it does declare was written", has(file, "width = 430"));
    }
    {
        const std::string out = session(designer, openepl, form, "select:ok_button;grip:s@0,30");
        check("with height: the vertical anchor resizes", has(out, "grip: s dragged") && has(out, "height=70"));
    }

    // The console: built through the real build path, then a drag over three
    // of its lines selects them, a keystroke changes nothing, and the newest
    // line is on screen.
    {
        const std::string out = session(designer, openepl, form, "build;logscroll;logselect:3,5;logdump");
        check("build log: the newest line is visible after a build", has(out, "newest=VISIBLE"));
        check("build log: a drag selects whole lines",
              has(out, "logselect:") && has(out, "|  stage 2/4") && has(out, "(--gc-sections)"));
        check("build log: typing into it changes nothing", has(out, "edited=no"));
        check("build log: the result keeps its class", has(out, "LOG [ok] OK  /tmp/openepl_studio_app"));
    }

    // Completion. Typing opens the popup, typing on narrows it, Enter takes
    // the row — and the character the platform sends after Enter stays out.
    {
        const std::string out = session(
            designer, openepl, form,
            "goto:40,1;typein:  call pri;waitcomplete;typein:nt_t;waitcomplete;key:enter;waitcomplete;bufline:40");
        check("completion: an identifier opens the popup with the server's items",
              has(out, "open=1 offered=") && has(out, "print_text"));
        check("completion: typing on narrows it", has(out, "shown=1 index=0 selected=print_text"));
        check("completion: Enter puts the item in and closes the popup",
              has(out, "buf 40:   call print_textend") && has(out, "open=0 offered=0"));
    }
    // Ctrl+Space asks with no word typed, so everything is offered; Escape
    // dismisses; and neither key leaves a character in the editor.
    {
        const std::string out = session(
            designer, openepl, form,
            "goto:40,1;typein:  call ;key:ctrl-space;waitcomplete;key:escape;waitcomplete;bufline:40");
        // The exact count is whatever the language server knows today, and it
        // grew the moment completion learned to read `use` lines; the property
        // worth holding is that the whole list is offered and all of it shown.
        int offered = 0, shown = 0;
        const size_t at = out.find("open=1 offered=");
        if (at != std::string::npos)
            std::sscanf(out.c_str() + at, "open=1 offered=%d shown=%d", &offered, &shown);
        check("completion: Ctrl+Space opens the full list", offered >= 100 && shown == offered);
        check("completion: Escape closes it", has(out, "open=0 offered=0"));
        check("completion: neither key typed anything", has(out, "buf 40:   call end"));
    }
    // The RAD loop from the keyboard: `on ` offers the events, the handler
    // position offers a subroutine that does not exist yet, and accepting it
    // writes the subroutine with the event's parameters.
    {
        std::string src = slurp("examples/eventparams.oir");
        const size_t at = src.find("  on tick: on_plain\n");
        src.insert(at + std::string("  on tick: on_plain\n").size(), "\n");
        const std::string fixture = "/tmp/openepl_studio_test_timer.oir";
        { std::ofstream f(fixture, std::ios::trunc); f << src; }
        const std::string out = session(
            designer, openepl, fixture,
            "goto:20,1;typein:  on ;waitcomplete;key:tab;typein:: pl;waitcomplete;key:enter;bufline:20;buftail:4");
        check("completion: `on ` offers the timer's event", has(out, "shown=1 index=0 selected=tick"));
        check("completion: the handler position offers the new subroutine",
              has(out, "selected=plain_tick"));
        check("completion: accepting writes the wiring line", has(out, "buf 20:   on tick: plain_tick"));
        check("completion: and the subroutine, with the event's parameter",
              has(out, "sub plain_tick(n: int)"));
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

    // The keyboard reaches the designer. The listener's keydown branch was
    // once never subscribed: every shortcut in it passed review and did
    // nothing on screen.
    {
        std::string path;
        session(designer, openepl, form, "add:button;key:ctrl-z", &path);
        check("Ctrl+Z on the canvas undoes the add", !has(slurp(path), "button1"));
        session(designer, openepl, form, "add:button;click:button1;key:delete", &path);
        check("Delete on the canvas removes the selection", !has(slurp(path), "button1"));
        const std::string out =
            session(designer, openepl, grid, "view:code;goto:104,20;focus;key:f12;waitdef");
        check("F12 pressed in the editor jumps to the declaration",
              has(out, "definition: caret 102,7"));
    }

    // Help > About: every way out closes it, a click inside does not, and a
    // link goes to the browser rather than anywhere in Studio.
    {
        const std::string out = session(
            designer, openepl, form,
            "about;aboutstate;key:escape;aboutstate;about;click:ok;aboutstate;about;click:x;"
            "aboutstate;about;clickat:30,300;aboutstate;about;clickat:720,400;aboutstate;"
            "click:GitHub-link;aboutstate");
        check("about: opens", has(out, "about: open\nkey: escape"));
        size_t closed = 0;
        for (size_t at = 0; (at = out.find("about: closed", at)) != std::string::npos; at++) closed++;
        check("about: Escape, OK, the cross and a click outside all dismiss", closed == 4);
        check("about: a click inside leaves it up", has(out, "click: GitHub-link\nabout: open"));
        check("about: a link opens in the browser",
              has(out, "about: open https://github.com/AxDSan/openepl"));
    }

    // The document tabs carry the file, and the Code tab the subroutine the
    // caret is in — the name the language server's index knows.
    {
        const std::string out = session(designer, openepl, form, "tabs;view:code;goto:39,3;tabs;goto:36,1;tabs");
        check("tabs: both name the file", has(out, "tabs: Designer [openepl_studio_test_") &&
                                          has(out, "] | Code [openepl_studio_test_"));
        check("tabs: the caret in a sub names it on the Code tab", has(out, "| Code [on_ok_click]"));
        check("tabs: outside every sub the Code tab names the file again",
              out.rfind("| Code [openepl_studio_test_") > out.find("| Code [on_ok_click]"));
    }

    // The preview's title bar is decoration: the client area starts under it
    // and the file's coordinates are measured from there, unshifted.
    {
        std::string path;
        const std::string out = session(designer, openepl, form, "geometry:greeting;geometry:ok_button", &path);
        check("title bar: the form's own size is kept", has(out, "form=480x300"));
        check("title bar: a component at top=40 is drawn 40px below it",
              has(out, "greeting=40,40") && has(out, "ok_button=40,110"));
        check("title bar: the client area starts under it", has(out, "title=32 client_top=33"));
        check("title bar: no icon set falls back to the app's", has(out, "icon=openepl-icon-64.png"));
        check("a session that does nothing leaves the file byte-identical", slurp(path) == slurp(form));
    }
    {
        // An icon beside the file is shown; one that is not there is not.
        const std::string dir = "/tmp/openepl_studio_test_icon";
        ::mkdir(dir.c_str(), 0755);
        const std::string fixture = dir + "/form.oir";
        std::string src = slurp(form);
        src.insert(src.find("  width  = 480"), "  icon   = \"mark.png\"\n");
        { std::ofstream f(fixture, std::ios::trunc); f << src; }
        { std::ofstream f(dir + "/mark.png", std::ios::trunc | std::ios::binary); f << slurp("assets/icons/button_16.png"); }
        // session() copies the fixture to /tmp, where mark.png is not.
        const std::string cmd = "OPENEPL_DESIGNER_SCRIPT='geometry:greeting' " + designer + " " + fixture +
                                " " + openepl + " 2>/dev/null";
        std::string out;
        if (FILE* p = popen(cmd.c_str(), "r")) {
            char buf[4096];
            while (fgets(buf, sizeof buf, p)) out += buf;
            pclose(p);
        }
        check("title bar: the form's icon, a path beside the file, is shown", has(out, "icon=mark.png"));
        const std::string missing = session(designer, openepl, fixture, "geometry:greeting");
        check("title bar: an icon that cannot be read falls back to the app's",
              has(missing, "icon=openepl-icon-64.png"));
    }

    // The wiring, on the canvas and in the inspector, and every way from it
    // into the handler.
    {
        const std::string out = session(designer, openepl, form, "badges;select:greeting;wiring;select:ok_button;wiring");
        check("badge: a wired component shows event and handler, above it",
              has(out, "badge: ok_button click\xe2\x86\x92on_ok_click at 40,86"));
        check("badge: an unwired one shows none", !has(out, "badge: greeting"));
        check("wiring: an unwired component says how to wire it",
              has(out, "wiring: HANDLER WIRINGNot linked \xe2\x80\x94 double-click to create"));
        check("wiring: a wired one names the handler", has(out, "wiring: HANDLER WIRINGLinked to: on_ok_click()on click"));
    }
    {
        const std::string out = session(designer, openepl, form, "select:ok_button;click:wirelink;caret");
        check("wiring: the link opens the code view in the handler",
              has(out, "designer: code view") && has(out, "caret: 39,") && !has(out, "wired"));
    }
    {
        // A badge is a way into the handler, but only the selection's: one
        // that took every click would sit over the components above it.
        std::string path;
        const std::string out = session(designer, openepl, form, "add:editbox;click:editbox1;view:designer;caret", &path);
        check("badge: an unselected badge lets the click through to the component under it",
              !has(slurp(path), "editbox1_change"));
    }
    {
        const std::string out = session(designer, openepl, form, "select:ok_button;badges");
        check("badge: the selection's badge is live", has(out, "badge: ok_button"));
    }
    {
        const std::string out = session(designer, openepl, form, "select:ok_button;click:segevents;events;click:ev-click;caret");
        check("events tab: lists the event with its handler", has(out, "events: click=on_ok_click"));
        check("events tab: choosing it jumps to the handler", has(out, "caret: 39,"));
    }
    {
        std::string path;
        const std::string out = session(designer, openepl, form, "add:button;click:segevents;events;click:ev-click;tabs", &path);
        check("events tab: a fresh component lists the event empty", has(out, "events: click=\n"));
        check("events tab: choosing it creates the handler as double-click does",
              has(out, "wired button1.click to button1_click") && has(slurp(path), "\nsub button1_click\n"));
        check("events tab: and the Code tab names the new sub", has(out, "| Code [button1_click]"));
    }

    // The window's own frame: no window-manager frame over it, and the green
    // dot maximises. What the manager did is read back from SDL, since a
    // scripted session cannot look at the screen.
    {
        const std::string out =
            session(designer, openepl, form, "winflags;click:wc-max;pump;winflags");
        check("the window is borderless", has(out, " borderless"));
        check("the maximise dot maximises", has(out, " maximized borderless"));
        const std::string closed =
            session(designer, openepl, form, "quitcheck;click:wc-close;quitcheck");
        check("the close dot asks the event loop to stop, as the manager's close does",
              has(closed, "quitcheck: running\n") && has(closed, "quitcheck: quit\n"));
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
