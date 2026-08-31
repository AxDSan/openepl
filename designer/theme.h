/* OpenEPL Studio design tokens.
 *
 * From the OpenEPL Studio design specification. Kept in one place so the chrome
 * never hard-codes a colour: changing the palette here changes the whole IDE.
 */
#ifndef OPENEPL_DESIGNER_THEME_H
#define OPENEPL_DESIGNER_THEME_H

namespace openepl::designer::theme {

// Primary / accent
/// Padding inside the code editor. Named because relayout() must subtract it
/// from the editor's box: RmlUi sizes content, not border, boxes.
/// Line box for the code editor. Both editor layers must use it: the
/// highlight layer lines up with the text only if their line heights match.
constexpr int CODE_LINE_H = 18;
/// The output pane stacks a PROBLEMS strip over the build log; both need fixed
/// heights so the log can scroll instead of growing to fit its content.
constexpr int PANEHEAD_H = 26;
constexpr int PROBLEMS_H = 72;
/// Line box in the output console, so the tail calculation is exact.
constexpr int LOG_LINE_H = 17;
constexpr int CODE_PAD_X = 10;
constexpr int CODE_PAD_Y = 8;

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
