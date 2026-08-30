/* Headless test for the part of the designer that can destroy user code.
 * Build/run via designer/build.sh test. */
#include <cstdio>
#include <cstdlib>
#include <fstream>
#include <sstream>
#include "model.h"

using namespace openepl::designer;

static int failures = 0;
static void check(const char* what, bool ok) {
    std::printf("  %-56s %s\n", what, ok ? "PASS" : "*** FAIL ***");
    if (!ok) failures++;
}

static std::string slurp(const std::string& p) {
    std::ifstream f(p);
    std::ostringstream o;
    o << f.rdbuf();
    return o.str();
}

int main(int argc, char** argv) {
    const std::string openepl = argc > 1 ? argv[1] : "./target/debug/openepl";
    const std::string fixture = "/tmp/openepl_designer_fixture.oir";

    // A file with a hand-written subroutine body that MUST survive a save.
    const char* source =
        "module fixture\n"
        "use ui\n"
        "\n"
        "var tally: int = 0\n"
        "\n"
        "form win\n"
        "  title = \"Fixture\"\n"
        "  width = 300\n"
        "  height = 200\n"
        "\n"
        "  button go\n"
        "    text = \"Go\"\n"
        "    left = 10\n"
        "    top = 10\n"
        "    on click: on_go\n"
        "  end\n"
        "end\n"
        "\n"
        "sub on_go\n"
        "  # A comment the designer must not touch.\n"
        "  tally = tally + 7\n"
        "  call print_int(tally)\n"
        "end\n";
    { std::ofstream f(fixture, std::ios::trunc); f << source; }
    const std::string original = slurp(fixture);

    Model m;
    std::string err;
    check("load via `openepl inspect`", load_model(openepl, fixture, m, err));
    if (!err.empty()) std::printf("    error: %s\n", err.c_str());
    check("module name", m.module_name == "fixture");
    check("form span found", m.form_first_line == 6 && m.form_last_line == 17);
    check("one component", m.children.size() == 1 && m.children[0].id == "go");
    check("handler read back", m.children[0].handler("click") &&
                                   *m.children[0].handler("click") == "on_go");
    check("existing sub listed", m.has_sub("on_go"));

    // Edit one property, exactly as the designer would.
    m.find("go")->set_property("left", "42");
    check("save", save_model(m, {}, err));

    const std::string saved = slurp(fixture);
    // THE test that matters: the hand-written body survives byte-identical.
    const char* body =
        "sub on_go\n"
        "  # A comment the designer must not touch.\n"
        "  tally = tally + 7\n"
        "  call print_int(tally)\n"
        "end\n";
    check("hand-written subroutine body is byte-identical",
          saved.find(body) != std::string::npos);
    check("module header preserved", saved.find("var tally: int = 0") != std::string::npos);
    check("edit applied", saved.find("left = 42") != std::string::npos);
    check("file actually changed", saved != original);

    // Reload: the saved file must parse and report the edit.
    Model m2;
    check("saved file re-inspects", load_model(openepl, fixture, m2, err));
    check("reloaded value", m2.find("go") && *m2.find("go")->property("left") == "42");

    // Adding a component + wiring a new handler appends a stub.
    Component c;
    c.id = m2.fresh_id("label");
    c.type_name = "label";
    c.set_property("text", "Added");
    c.set_property("left", "10");
    c.set_property("top", "60");
    m2.children.push_back(c);
    m2.find("go")->set_handler("click", "on_go_click2");
    check("save with a new sub", save_model(m2, {"on_go_click2"}, err));
    const std::string saved2 = slurp(fixture);
    check("stub subroutine appended", saved2.find("sub on_go_click2") != std::string::npos);
    check("original body still intact", saved2.find(body) != std::string::npos);
    check("new component present", saved2.find("label label1") != std::string::npos);

    std::printf("\n%d failure(s)\n", failures);
    return failures == 0 ? 0 : 1;
}
