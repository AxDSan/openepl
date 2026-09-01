/* What the toolbox holds, assembled from metadata rather than a list here.
 *
 * Two sources, because neither is complete on its own:
 *
 *   `openepl kits` / `openepl commands --use <kit>` name every kit installed,
 *   the section it files under, and the components it declares — including
 *   kits the IDE has never heard of, which is the whole promise of the kit
 *   system: a kit dropped into `kits/` appears in Studio with no IDE change.
 *
 *   The linked `ui_libinfo.c` adds what those lines do not carry yet: whether
 *   a component is visual, each property's default, and the `editor` hint the
 *   inspector needs to offer a colour swatch instead of a hex field.
 *
 * A component the CLI reports and the linked table does not know is treated as
 * NON-VISUAL. The two mistakes are not symmetric: a visual component in the
 * tray is a nuisance, and a non-visual one dropped into a form is source the
 * validator rejects.
 */
#ifndef OPENEPL_DESIGNER_CATALOG_H
#define OPENEPL_DESIGNER_CATALOG_H

#include <cstdio>
#include <cstring>
#include <sstream>
#include <string>
#include <vector>

#include "descriptors.h"

namespace openepl::designer {

struct CatalogProp {
    std::string name;
    std::string type;            // as `openepl commands` spells it
    std::string editor;          // "", "color", "file", "font", "multiline"
    std::string default_value;
    bool has_default = false;
};

struct CatalogComponent {
    std::string type_name;
    std::string kit;             // "" for the ones the runtime itself provides
    std::string section;         // the toolbox heading it files under
    bool visual = false;
    std::vector<CatalogProp> props;
    std::vector<std::string> events;
};

struct Catalog {
    std::vector<CatalogComponent> components;   // already in toolbox order
    /// Section headings in the order the kits reported them.
    std::vector<std::string> sections;

    const CatalogComponent* find(const std::string& type_name) const {
        for (const auto& c : components) {
            if (c.type_name == type_name) return &c;
        }
        return nullptr;
    }
};

namespace catalog_detail {

inline std::vector<std::string> run(const std::string& cmd) {
    std::vector<std::string> out;
    FILE* pipe = popen((cmd + " 2>/dev/null").c_str(), "r");
    if (!pipe) return out;
    char buf[4096];
    while (fgets(buf, sizeof buf, pipe)) {
        std::string line(buf);
        while (!line.empty() && (line.back() == '\n' || line.back() == '\r')) line.pop_back();
        out.push_back(line);
    }
    pclose(pipe);
    return out;
}

/// `<prefix><rest>` -> rest, or no value when the line is a different kind.
inline bool after(const std::string& line, const char* prefix, std::string& rest) {
    const size_t n = std::strlen(prefix);
    if (line.rfind(prefix, 0) != 0) return false;
    rest = line.substr(n);
    return true;
}

inline std::vector<std::string> words(const std::string& s) {
    std::vector<std::string> out;
    std::istringstream in(s);
    std::string w;
    while (in >> w) out.push_back(w);
    return out;
}

/// One kit, as `openepl kits` describes it. Emission order IS resolution
/// order, so the vector's order is the kit ordering and nothing sorts it.
struct KitLine {
    std::string name, display, section;
};

inline std::vector<KitLine> read_kits(const std::string& bin) {
    std::vector<KitLine> kits;
    auto by_name = [&](const std::string& n) -> KitLine* {
        for (auto& k : kits) {
            if (k.name == n) return &k;
        }
        return nullptr;
    };
    for (const auto& line : run(bin + " kits")) {
        std::string rest;
        if (after(line, "kit: ", rest)) {
            const auto w = words(rest);
            if (!w.empty() && !by_name(w[0])) kits.push_back({w[0], w[0], ""});
        } else if (after(line, "name: ", rest)) {
            const auto w = words(rest);
            if (w.size() >= 2) {
                if (KitLine* k = by_name(w[0])) k->display = rest.substr(w[0].size() + 1);
            }
        } else if (after(line, "section: ", rest)) {
            const auto w = words(rest);
            if (w.size() >= 2) {
                if (KitLine* k = by_name(w[0])) k->section = rest.substr(w[0].size() + 1);
            }
        }
    }
    return kits;
}

/// The components `openepl commands` reports for `uses`, with their declared
/// properties and events.
inline std::vector<CatalogComponent> read_components(const std::string& bin,
                                                     const std::string& use) {
    std::vector<CatalogComponent> out;
    const std::string cmd = use.empty() ? bin + " commands" : bin + " commands --use " + use;
    for (const auto& line : run(cmd)) {
        std::string rest;
        if (after(line, "component: ", rest)) {
            const auto w = words(rest);
            if (!w.empty()) out.push_back(CatalogComponent{w[0], use, "", false, {}, {}});
        } else if (after(line, "property: ", rest)) {
            const auto w = words(rest);
            if (w.size() >= 3 && !out.empty()) {
                for (auto& c : out) {
                    if (c.type_name == w[0]) c.props.push_back({w[1], w[2], "", "", false});
                }
            }
        } else if (after(line, "event: ", rest)) {
            const auto w = words(rest);
            if (w.size() >= 2) {
                for (auto& c : out) {
                    if (c.type_name == w[0]) c.events.push_back(w[1]);
                }
            }
        }
    }
    return out;
}

/// Fill in what only the linked metadata table knows: visual or not, each
/// property's default, and the editor hint.
inline void enrich_from_libinfo(CatalogComponent& c) {
    const OpenEPL_ComponentDesc* desc = describe(c.type_name.c_str());
    if (!desc) return;
    c.visual = (desc->kind == OE_COMPONENT_VISUAL);
    for (auto& p : c.props) {
        for (int i = 0; i < desc->property_count; i++) {
            if (p.name != desc->properties[i].name) continue;
            if (desc->properties[i].editor) p.editor = desc->properties[i].editor;
            if (desc->properties[i].default_value) {
                p.default_value = desc->properties[i].default_value;
                p.has_default = true;
            }
        }
    }
}

}  // namespace catalog_detail

/// Assemble the catalogue. One subprocess per kit, once, at startup.
inline Catalog build_catalog(const std::string& openepl_bin) {
    using namespace catalog_detail;
    Catalog cat;

    // What exists with no `use` at all: the runtime's own components, which
    // belong to no kit. `--use net` reports them too, so this is also the
    // baseline that keeps them from being attributed to whichever kit was
    // asked first.
    std::vector<std::string> baseline_names;
    for (auto& c : read_components(openepl_bin, "")) {
        baseline_names.push_back(c.type_name);
        enrich_from_libinfo(c);
        c.section = "System";
        cat.components.push_back(c);
    }

    for (const auto& kit : read_kits(openepl_bin)) {
        for (auto& c : read_components(openepl_bin, kit.name)) {
            bool baseline = false;
            for (const auto& b : baseline_names) {
                if (b == c.type_name) baseline = true;
            }
            if (baseline || cat.find(c.type_name)) continue;
            enrich_from_libinfo(c);
            // A kit with nothing to say about its section gets `kit.rs`'s
            // generic default, which reads as a heading no better than the
            // kit's own name. The name at least distinguishes one kit's
            // controls from another's.
            const std::string kit_section =
                (kit.section.empty() || kit.section == "Libraries") ? kit.display : kit.section;
            // Everything without pixels files under one heading, whichever kit
            // it came from: the tray is one place, so its source is too.
            c.section = c.visual ? kit_section : "System";
            cat.components.push_back(c);
        }
    }

    // The linked table is the floor, not the ceiling: if the toolchain cannot
    // be run — a wrong compiler path, a tree mid-rebuild — a toolbox built
    // only from its answers would be empty, and an IDE with no components is
    // no more use than one with the wrong ones. Everything Studio was compiled
    // against is added here if the CLI did not already report it.
    {
        std::string ui_section = "Common Controls";
        for (const auto& k : read_kits(openepl_bin)) {
            if (k.name != "ui") continue;
            ui_section = (k.section.empty() || k.section == "Libraries") ? k.display : k.section;
        }
        const OpenEPL_LibInfo* lib = ui_library();
        for (int i = 0; i < lib->component_count; i++) {
            const OpenEPL_ComponentDesc& d = lib->components[i];
            if (cat.find(d.name)) continue;
            CatalogComponent c;
            c.type_name = d.name;
            c.kit = "ui";
            c.visual = (d.kind == OE_COMPONENT_VISUAL);
            c.section = c.visual ? ui_section : "System";
            for (int j = 0; j < d.property_count; j++) {
                CatalogProp p;
                p.name = d.properties[j].name;
                p.type = d.properties[j].tag == OE_SDT_INT      ? "int"
                         : d.properties[j].tag == OE_SDT_INT64  ? "int64"
                         : d.properties[j].tag == OE_SDT_DOUBLE ? "double"
                         : d.properties[j].tag == OE_SDT_BOOL   ? "bool"
                                                                : "text";
                if (d.properties[j].editor) p.editor = d.properties[j].editor;
                if (d.properties[j].default_value) {
                    p.default_value = d.properties[j].default_value;
                    p.has_default = true;
                }
                c.props.push_back(p);
            }
            for (int j = 0; j < d.event_count; j++) c.events.push_back(d.events[j].name);
            cat.components.push_back(c);
        }
    }

    for (const auto& c : cat.components) {
        bool seen = false;
        for (const auto& s : cat.sections) {
            if (s == c.section) seen = true;
        }
        if (!seen) cat.sections.push_back(c.section);
    }
    // System last: it is the tray's shelf, not the palette people reach for.
    for (size_t i = 0; i < cat.sections.size(); i++) {
        if (cat.sections[i] == "System") {
            cat.sections.erase(cat.sections.begin() + (long)i);
            cat.sections.push_back("System");
            break;
        }
    }
    return cat;
}

}  // namespace openepl::designer
#endif
