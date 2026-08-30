/* OpenEPL Studio design tokens.
 *
 * From the OpenEPL Studio design specification. Kept in one place so the chrome
 * never hard-codes a colour: changing the palette here changes the whole IDE.
 */
#ifndef OPENEPL_DESIGNER_THEME_H
#define OPENEPL_DESIGNER_THEME_H

namespace openepl::designer::theme {

// Primary / accent
constexpr const char* ACCENT        = "#1e60d5";
constexpr const char* ACCENT_TEXT   = "#ffffff";

// Backgrounds
constexpr const char* PANEL         = "#ffffff";
constexpr const char* CHROME        = "#f3f3f3";
constexpr const char* CHROME_ALT    = "#f8f9fa";
constexpr const char* CANVAS        = "#fafafa";

// Text
constexpr const char* TEXT          = "#1f2328";
constexpr const char* TEXT_MUTED    = "#656d76";
constexpr const char* BORDER        = "#d0d7de";
constexpr const char* BORDER_SOFT   = "#e1e4e8";

// Semantic
constexpr const char* SUCCESS       = "#1a7f37";
constexpr const char* DANGER        = "#cf222e";

// Syntax highlighting
constexpr const char* SYN_KEYWORD   = "#6f42c1";
constexpr const char* SYN_METHOD    = "#0550ae";
constexpr const char* SYN_STRING    = "#9a6700";
constexpr const char* SYN_IDENT     = "#0e7490";
constexpr const char* SYN_COMMENT   = "#656d76";
constexpr const char* SYN_NUMBER    = "#0550ae";

// Metrics
constexpr int TOOLBOX_W  = 220;
constexpr int INSPECT_W  = 280;
constexpr int TITLEBAR_H = 32;
constexpr int MENUBAR_H  = 28;
constexpr int TOOLBAR_H  = 40;
constexpr int TABBAR_H   = 32;
constexpr int STATUS_H   = 24;
constexpr int BOTTOM_H   = 220;

} // namespace openepl::designer::theme
#endif
