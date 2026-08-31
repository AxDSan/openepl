#include "model.h"

#include <cstdio>
#include <fstream>
#include <sstream>

namespace openepl::designer {
namespace {

std::vector<std::string> split_words(const std::string& line, int max_parts) {
    std::vector<std::string> out;
    size_t i = 0;
    while (i < line.size() && (int)out.size() < max_parts - 1) {
        size_t sp = line.find(' ', i);
        if (sp == std::string::npos) break;
        out.push_back(line.substr(i, sp - i));
        i = sp + 1;
    }
    out.push_back(line.substr(i));
    return out;
}

std::string quote(const std::string& v) {
    std::string out = "\"";
    for (char c : v) {
        if (c == '"' || c == '\\') out += '\\';
        out += c;
    }
    return out + "\"";
}

/// Write a property value as the declared type requires: quoted for text,
/// bare for numbers and booleans.
std::string render_value(const std::string& type_name, const std::string& property,
                         const std::string& value, const NeedsQuotes& needs_quotes) {
    return needs_quotes(type_name, property) ? quote(value) : value;
}

} // namespace

bool load_model(const std::string& openepl_bin, const std::string& path, Model& out,
                std::string& error) {
    const std::string cmd = openepl_bin + " inspect " + path + " 2>&1";
    FILE* pipe = popen(cmd.c_str(), "r");
    if (!pipe) { error = "could not run openepl inspect"; return false; }

    out = Model{};
    out.path = path;
    std::string text;
    char buf[4096];
    while (fgets(buf, sizeof buf, pipe)) text += buf;
    const int rc = pclose(pipe);

    std::istringstream lines(text);
    std::string line;
    Component* current = nullptr;   // component most recently declared
    while (std::getline(lines, line)) {
        if (line.rfind("module: ", 0) == 0) {
            out.module_name = line.substr(8);
        } else if (line.rfind("use: ", 0) == 0) {
            out.uses.push_back(line.substr(5));
        } else if (line.rfind("sub: ", 0) == 0) {
            out.subs.push_back(line.substr(5));
        } else if (line.rfind("form: ", 0) == 0) {
            auto parts = split_words(line.substr(6), 2);
            out.form_name = parts[0];
            out.form.id = parts[0];
            out.form.type_name = "form";
            if (parts.size() > 1) {
                std::sscanf(parts[1].c_str(), "span=%d..%d", &out.form_first_line,
                            &out.form_last_line);
            }
            current = &out.form;
        } else if (line.rfind("component: ", 0) == 0) {
            auto parts = split_words(line.substr(11), 2);
            Component c;
            c.id = parts[0];
            c.type_name = parts.size() > 1 ? parts[1] : "";
            out.children.push_back(c);
            current = &out.children.back();
        } else if (line.rfind("prop: ", 0) == 0) {
            auto parts = split_words(line.substr(6), 3);
            if (parts.size() == 3) {
                Component* target =
                    (parts[0] == out.form_name) ? &out.form : out.find(parts[0]);
                if (target) target->set_property(parts[1], parts[2]);
            }
        } else if (line.rfind("handler: ", 0) == 0) {
            auto parts = split_words(line.substr(9), 3);
            if (parts.size() == 3) {
                Component* target =
                    (parts[0] == out.form_name) ? &out.form : out.find(parts[0]);
                if (target) target->set_handler(parts[1], parts[2]);
            }
        }
        (void)current;
    }

    if (rc != 0) {
        error = text.empty() ? "openepl inspect failed" : text;
        return false;
    }
    // A module with no form is still a project: a console program or a library
    // has code to edit even though it has nothing to lay out. Refusing to open
    // one would mean the welcome screen offers templates the IDE cannot then
    // open. The canvas simply has nothing to draw.
    return true;
}

std::string emit_form(const Model& m, const NeedsQuotes& needs_quotes) {
    std::ostringstream o;
    o << "form " << m.form_name << "\n";
    for (const auto& p : m.form.properties) {
        o << "  " << p.first << " = " << render_value("form", p.first, p.second, needs_quotes)
          << "\n";
    }
    for (const auto& h : m.form.handlers) {
        o << "  on " << h.first << ": " << h.second << "\n";
    }
    for (const auto& c : m.children) {
        o << "\n  " << c.type_name << " " << c.id << "\n";
        for (const auto& p : c.properties) {
            o << "    " << p.first << " = "
              << render_value(c.type_name, p.first, p.second, needs_quotes) << "\n";
        }
        for (const auto& h : c.handlers) {
            o << "    on " << h.first << ": " << h.second << "\n";
        }
        o << "  end\n";
    }
    o << "end\n";
    return o.str();
}

bool save_model(const Model& m, const std::vector<std::string>& new_subs,
                const NeedsQuotes& needs_quotes, std::string& error) {
    std::ifstream in(m.path);
    if (!in) { error = "cannot read " + m.path; return false; }
    std::vector<std::string> lines;
    std::string line;
    while (std::getline(in, line)) lines.push_back(line);
    in.close();

    if (m.form_first_line < 1 || m.form_last_line > (int)lines.size() ||
        m.form_first_line > m.form_last_line) {
        error = "form line span is out of range; refusing to save";
        return false;
    }

    std::ostringstream out;
    // Everything before the form, verbatim.
    for (int i = 0; i < m.form_first_line - 1; i++) out << lines[i] << "\n";
    // The regenerated form.
    out << emit_form(m, needs_quotes);
    // Everything after the form, verbatim — this is what protects hand-written
    // subroutine bodies from being clobbered by a designer save.
    for (size_t i = (size_t)m.form_last_line; i < lines.size(); i++) out << lines[i] << "\n";
    // Stubs for handlers the user just wired up.
    for (const auto& name : new_subs) {
        out << "\nsub " << name << "\n"
            << "  # TODO: written by the designer; add your code here.\n"
            << "  call print_text(\"" << name << "\")\n"
            << "end\n";
    }

    std::ofstream os(m.path, std::ios::trunc);
    if (!os) { error = "cannot write " + m.path; return false; }
    os << out.str();
    return true;
}

} // namespace openepl::designer
