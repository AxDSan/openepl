// Splash and Welcome screens for OpenEPL Studio.
//
// Two separate documents shown before the IDE chrome exists, for two reasons.
// The splash has to be on screen *during* the expensive startup work — loading
// the runtime registry through `openepl inspect` — so it must render before
// that work begins, which means before the model exists. And the welcome screen
// has no project yet, so a half-initialised IDE behind it would have nothing
// coherent to draw.
//
// The template tiles are generated from `openepl templates` rather than from a
// list here: the CLI is the single reader of the templates directory (ADR
// 0011/0013), so adding a template adds a tile with no change to this file.
#ifndef OPENEPL_DESIGNER_WELCOME_H
#define OPENEPL_DESIGNER_WELCOME_H

#include <cstdio>
#include <cstdlib>
#include <sys/stat.h>
#include <sys/types.h>
#include <unistd.h>
#include <string>
#include <vector>

#include "theme.h"

namespace openepl::welcome {

/// One project template, as reported by `openepl templates`.
struct TemplateInfo {
    std::string id;
    std::string target;
    std::string name;
    std::string desc;
    std::string entry;
};

/// Run `openepl templates` and parse its line-based output.
///
/// Same shape as the designer's use of `openepl inspect`: the CLI reads the
/// templates directory, this only renders what it reports.
inline std::vector<TemplateInfo> load_templates(const std::string& openepl_bin) {
    std::vector<TemplateInfo> out;
    const std::string cmd = openepl_bin + " templates 2>/dev/null";
    FILE* pipe = popen(cmd.c_str(), "r");
    if (!pipe) return out;

    auto find = [&out](const std::string& id) -> TemplateInfo& {
        for (auto& t : out) {
            if (t.id == id) return t;
        }
        out.push_back(TemplateInfo{id, "", id, "", "main.oir"});
        return out.back();
    };

    char buf[1024];
    while (fgets(buf, sizeof buf, pipe)) {
        std::string line(buf);
        while (!line.empty() && (line.back() == '\n' || line.back() == '\r')) line.pop_back();
        const size_t colon = line.find(':');
        if (colon == std::string::npos) continue;
        const std::string key = line.substr(0, colon);
        std::string rest = line.substr(colon + 1);
        if (!rest.empty() && rest[0] == ' ') rest.erase(0, 1);
        const size_t sp = rest.find(' ');
        const std::string id = rest.substr(0, sp);
        const std::string value = sp == std::string::npos ? "" : rest.substr(sp + 1);
        if (id.empty()) continue;
        TemplateInfo& t = find(id);
        if (key == "template") t.target = value;
        else if (key == "name") t.name = value;
        else if (key == "desc") t.desc = value;
        else if (key == "entry") t.entry = value;
    }
    pclose(pipe);
    return out;
}

/// Shared styling for both screens: the same palette as the IDE, so startup
/// does not flash a different-looking product.
inline std::string base_styles() {
    using namespace openepl::designer::theme;
    std::string s;
    // RmlUi has no HTML-like default stylesheet: every element is `inline`
    // unless told otherwise. Without this the tiles flow into one another
    // instead of stacking, which is exactly what the first render looked like.
    s += "div{display:block}";
    s += "body{background-color:";
    s += CANVAS;
    s += ";color:";
    s += TEXT;
    s += ";font-size:14px}";
    s += "#mark{font-size:34px;font-weight:bold;color:";
    s += TEXT;
    s += "}";
    s += ".accent{color:";
    s += ACCENT;
    s += "}";
    s += "#tag{font-size:13px;color:";
    s += TEXT_MUTED;
    s += ";margin-top:6px}";
    return s;
}

/// The splash: shown while the runtime registry loads.
inline std::string splash_markup(const std::string& family, int w, int h) {
    std::string s = "<rml><head><style>";
    s += "body{width:" + std::to_string(w) + "px;height:" + std::to_string(h) +
         "px;font-family:'" + family + "'}";
    s += base_styles();
    s += "#box{position:absolute;left:0;top:0;width:" + std::to_string(w) + "px;height:" +
         std::to_string(h) + "px}";
    s += "#inner{position:absolute;left:0;top:" + std::to_string(h / 2 - 60) +
         "px;width:" + std::to_string(w) + "px;text-align:center}";
    s += "</style></head><body><div id='box'><div id='inner'>";
    s += "<div id='mark'>OpenEPL <span class='accent'>Studio</span></div>";
    // Literal UTF-8, not entities: RmlUi decodes only a small set (lt, gt, amp,
    // quot, nbsp) and prints anything else verbatim, so `&middot;` would show
    // up on screen exactly as written.
    s += "<div id='tag'>RAD is the identity \u00b7 English-first \u00b7 Cross-platform</div>";
    s += "<div id='tag'>Loading the component library\u2026</div>";
    s += "</div></div></body></rml>";
    return s;
}

/// The welcome screen: pick a project kind, or open something recent.
inline std::string welcome_markup(const std::string& family, int w, int h,
                                  const std::vector<TemplateInfo>& templates,
                                  const std::vector<std::string>& recent) {
    using namespace openepl::designer::theme;
    std::string s = "<rml><head><style>";
    s += "body{width:" + std::to_string(w) + "px;height:" + std::to_string(h) +
         "px;font-family:'" + family + "'}";
    s += base_styles();
    s += "#head{position:absolute;left:56px;top:48px;width:" + std::to_string(w - 112) + "px}";
    s += "#cols{position:absolute;left:56px;top:150px;width:" + std::to_string(w - 112) + "px}";
    s += ".colhead{font-size:11px;font-weight:bold;color:" + std::string(TEXT_MUTED) +
         ";margin-bottom:10px}";
    // Tiles are buttons: give them a hover state so they read as clickable.
    s += ".tile{background-color:" + std::string(PANEL) + ";border:1px " + BORDER +
         ";border-radius:8px;padding:14px 16px;margin-bottom:10px;cursor:pointer}";
    s += ".tile:hover{border:1px " + std::string(ACCENT) + ";background-color:#f2f6ff}";
    s += ".tname{font-size:15px;font-weight:bold;color:" + std::string(TEXT) + "}";
    s += ".tdesc{font-size:12px;color:" + std::string(TEXT_MUTED) + ";margin-top:4px}";
    s += ".ttarget{font-size:10px;color:" + std::string(ACCENT) + ";margin-top:6px}";
    s += ".recent{padding:8px 10px;border-radius:6px;cursor:pointer;font-size:13px;color:" +
         std::string(TEXT) + "}";
    s += ".recent:hover{background-color:" + std::string(PANEL) + "}";
    s += ".rpath{font-size:11px;color:" + std::string(TEXT_MUTED) + "}";
    s += ".empty{font-size:12px;color:" + std::string(TEXT_MUTED) + ";font-style:italic}";
    s += "#left{position:absolute;left:0;top:0;width:520px}";
    s += "#right{position:absolute;left:560px;top:0;width:420px}";
    s += "#foot{position:absolute;left:56px;top:" + std::to_string(h - 56) +
         "px;font-size:11px;color:" + TEXT_MUTED + "}";
    // An explicit backdrop rather than relying on the body's own background:
    // the body paints its box, but a full-window rectangle is what actually
    // guarantees no unpainted region shows through as black.
    s += "#bg{position:absolute;left:0;top:0;width:" + std::to_string(w) + "px;height:" +
         std::to_string(h) + "px;background-color:" + CANVAS + "}";
    s += "</style></head><body><div id='bg'/>";

    s += "<div id='head'><div id='mark'>OpenEPL <span class='accent'>Studio</span></div>"
         "<div id='tag'>Start something. Every template below compiles to a clean native "
         "binary.</div></div>";

    s += "<div id='cols'><div id='left'><div class='colhead'>NEW PROJECT</div>";
    for (const auto& t : templates) {
        s += "<div class='tile' oe-new='" + t.id + "'>";
        s += "<div class='tname'>" + t.name + "</div>";
        s += "<div class='tdesc'>" + t.desc + "</div>";
        s += "<div class='ttarget'>target: " + t.target + "</div>";
        s += "</div>";
    }
    if (templates.empty()) {
        s += "<div class='empty'>No templates found — is the templates/ directory present?</div>";
    }
    s += "</div>";

    s += "<div id='right'><div class='colhead'>RECENT</div>";
    for (const auto& r : recent) {
        const size_t slash = r.find_last_of('/');
        const std::string base = slash == std::string::npos ? r : r.substr(slash + 1);
        s += "<div class='recent' oe-open='" + r + "'><div>" + base + "</div>";
        s += "<div class='rpath'>" + r + "</div></div>";
    }
    if (recent.empty()) {
        s += "<div class='empty'>Nothing yet. Pick a template to begin.</div>";
    }
    s += "</div></div>";

    s += "<div id='foot'>openepl-designer &lt;project.oir&gt; opens a file directly</div>";
    s += "</body></rml>";
    return s;
}

/// Where the recent-projects list lives.
inline std::string recent_path() {
    const char* home = std::getenv("HOME");
    if (!home) return "";
    return std::string(home) + "/.local/share/openepl/recent";
}

inline std::vector<std::string> load_recent(size_t limit = 8) {
    std::vector<std::string> out;
    const std::string path = recent_path();
    if (path.empty()) return out;
    FILE* f = std::fopen(path.c_str(), "r");
    if (!f) return out;
    char buf[1024];
    while (out.size() < limit && fgets(buf, sizeof buf, f)) {
        std::string line(buf);
        while (!line.empty() && (line.back() == '\n' || line.back() == '\r')) line.pop_back();
        // Skip files that have since been deleted or moved: offering a dead
        // path is worse than offering nothing.
        if (!line.empty() && ::access(line.c_str(), R_OK) == 0) out.push_back(line);
    }
    std::fclose(f);
    return out;
}

/// Record `path` as the most recent project, de-duplicated, newest first.
///
/// Stored absolute: a relative path is meaningless from the next session's
/// working directory, and the entry would silently vanish from the list.
inline void remember_recent(const std::string& relative_or_absolute, size_t limit = 8) {
    std::string path = relative_or_absolute;
    if (char* real = ::realpath(relative_or_absolute.c_str(), nullptr)) {
        path = real;
        std::free(real);
    }
    const std::string file = recent_path();
    if (file.empty()) return;
    const size_t slash = file.find_last_of('/');
    if (slash != std::string::npos) {
        const std::string dir = file.substr(0, slash);
        std::string acc;
        for (size_t i = 0; i < dir.size(); i++) {
            acc += dir[i];
            if (dir[i] == '/' || i + 1 == dir.size()) ::mkdir(acc.c_str(), 0755);
        }
    }
    std::vector<std::string> keep{path};
    for (const auto& r : load_recent(limit * 2)) {
        if (r != path && keep.size() < limit) keep.push_back(r);
    }
    FILE* f = std::fopen(file.c_str(), "w");
    if (!f) return;
    for (const auto& r : keep) std::fprintf(f, "%s\n", r.c_str());
    std::fclose(f);
}

} // namespace openepl::welcome

#endif
