/* The designer's document model: what `openepl inspect` reported, plus edits.
 *
 * Deliberately dumb — a list of components with string properties. The compiler
 * owns all meaning; the designer only moves text around.
 */
#ifndef OPENEPL_DESIGNER_MODEL_H
#define OPENEPL_DESIGNER_MODEL_H

#include <functional>
#include <string>
#include <utility>
#include <vector>

namespace openepl::designer {

struct Component {
    std::string id;
    std::string type_name;
    std::vector<std::pair<std::string, std::string>> properties;  // ordered
    std::vector<std::pair<std::string, std::string>> handlers;    // event -> sub

    const std::string* property(const std::string& name) const {
        for (const auto& p : properties) {
            if (p.first == name) return &p.second;
        }
        return nullptr;
    }
    void set_property(const std::string& name, const std::string& value) {
        for (auto& p : properties) {
            if (p.first == name) { p.second = value; return; }
        }
        properties.emplace_back(name, value);
    }
    const std::string* handler(const std::string& event) const {
        for (const auto& h : handlers) {
            if (h.first == event) return &h.second;
        }
        return nullptr;
    }
    void set_handler(const std::string& event, const std::string& sub) {
        for (auto& h : handlers) {
            if (h.first == event) { h.second = sub; return; }
        }
        handlers.emplace_back(event, sub);
    }
};

struct Model {
    std::string path;                 // the .oir being edited
    std::string module_name;
    std::vector<std::string> uses;
    std::vector<std::string> subs;    // existing subroutine names
    std::string form_name;
    int form_first_line = 0;          // 1-based, inclusive
    int form_last_line = 0;
    Component form;                   // the form's own properties/handlers
    std::vector<Component> children;

    Component* find(const std::string& id) {
        for (auto& c : children) {
            if (c.id == id) return &c;
        }
        return nullptr;
    }
    bool has_sub(const std::string& name) const {
        for (const auto& s : subs) {
            if (s == name) return true;
        }
        return false;
    }
    /// An id not already taken, based on a component type (`button1`, `button2`…).
    std::string fresh_id(const std::string& type_name) const {
        for (int n = 1;; n++) {
            std::string candidate = type_name + std::to_string(n);
            bool taken = false;
            for (const auto& c : children) {
                if (c.id == candidate) { taken = true; break; }
            }
            if (!taken) return candidate;
        }
    }
};

/// Load by running `openepl inspect` — never by parsing .oir here (ADR 0011).
bool load_model(const std::string& openepl_bin, const std::string& path, Model& out,
                std::string& error);

/// Decides whether a property value must be written as a quoted text literal.
/// The designer supplies this from the component descriptors, because only the
/// declared type can tell `text = "true"` from `checked = true` — guessing from
/// the value's shape writes uncompilable source.
using NeedsQuotes =
    std::function<bool(const std::string& type_name, const std::string& property)>;

/// Render the model's `form … end` block as .oir source.
std::string emit_form(const Model& m, const NeedsQuotes& needs_quotes);

/// Save by SPLICING the emitted form over the original file's form lines,
/// leaving everything else — every hand-written subroutine body — byte-identical.
/// Newly wired handlers are appended as stub subroutines.
bool save_model(const Model& m, const std::vector<std::string>& new_subs,
                const NeedsQuotes& needs_quotes, std::string& error);

} // namespace openepl::designer
#endif
