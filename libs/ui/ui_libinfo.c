/* "ui" support library metadata — visual component descriptors (D9/D11).
 *
 * DESIGN-TIME ONLY: compiled into the introspection .so, never into a shipped
 * program (same split as core_libinfo.c. The compiler reads this
 * to learn which component types exist, what properties and events they have,
 * and their accessibility roles (D16).
 */
#include "openepl_abi.h"

/* --- form ------------------------------------------------------------- */
static const OpenEPL_PropertyDesc FORM_PROPS[] = {
    { "title",            OE_SDT_TEXT, "OpenEPL Application", NULL },
    { "width",            OE_SDT_INT,  "800",                 NULL },
    { "height",           OE_SDT_INT,  "600",                 NULL },
    /* A window's ground, not a terminal's. Every desktop the target audience
     * has used draws a form in light grey; a dark default reads as a theme
     * someone has to switch off before their first app looks normal. */
    { "background_color", OE_SDT_TEXT, "#f0f0f0",             "color" },
    /* The window's icon: a PNG beside the source, embedded at build time like
     * an image's source, so the shipped binary carries it. */
    { "icon",             OE_SDT_TEXT, "",                    "file" },
    /* Where the window opens. `default` leaves it to the window manager (the
     * substrate centres it today); `center` asks for the middle of the
     * screen; `manual` puts its top-left corner at `left`,`top`. A later
     * assignment of `left`/`top` from a subroutine moves the window when the
     * form is `manual`; assigning `position` after the window exists is
     * ignored, because a window that jumps between modes mid-run is a bug a
     * program cannot mean. */
    { "position",         OE_SDT_TEXT, "default",             NULL },
    { "left",             OE_SDT_INT,  "0",                   NULL },
    { "top",              OE_SDT_INT,  "0",                   NULL },
};
static const OpenEPL_EventDesc FORM_EVENTS[] = { { "load", 0, NULL } };

/* Delphi's `Anchors`, on every control that has a rectangle: which edges of
 * the window it keeps its distance from when the window is resized. The far
 * edge alone moves the control with the window, both edges stretch it, and
 * the default keeps it where the form put it — so a form written without a
 * thought for resizing behaves exactly as it always has. The layout rule is
 * `anchored_rect` in ui_mapping.h, shared with the designer. */
#define ANCHORS { "anchors", OE_SDT_TEXT, "left,top", "anchors" }

/* --- button ----------------------------------------------------------- */
static const OpenEPL_PropertyDesc BUTTON_PROPS[] = {
    { "text",             OE_SDT_TEXT, "Button",  NULL },
    { "left",             OE_SDT_INT,  "0",       NULL },
    { "top",              OE_SDT_INT,  "0",       NULL },
    { "width",            OE_SDT_INT,  "120",     NULL },
    { "height",           OE_SDT_INT,  "36",      NULL },
    ANCHORS,
    { "background_color", OE_SDT_TEXT, "#4a86e8", "color" },
    { "color",            OE_SDT_TEXT, "#ffffff", "color" },
    { "border_radius",    OE_SDT_INT,  "6",       NULL },
    { "enabled",          OE_SDT_BOOL, "true",    NULL },
    { "action",           OE_SDT_TEXT, "",        NULL },
};
static const OpenEPL_EventDesc BUTTON_EVENTS[] = { { "click", 0, NULL } };

/* --- label ------------------------------------------------------------ */
static const OpenEPL_PropertyDesc LABEL_PROPS[] = {
    { "text",  OE_SDT_TEXT, "Label",   NULL },
    { "left",  OE_SDT_INT,  "0",       NULL },
    { "top",   OE_SDT_INT,  "0",       NULL },
    { "width", OE_SDT_INT,  "200",     NULL },
    /* A label the designer can size only sideways is a label the designer
     * writes a `height` into anyway, and the build then rejects. */
    { "height", OE_SDT_INT, "24",      NULL },
    ANCHORS,
    { "color", OE_SDT_TEXT, "#1f2328", "color" },
};

/* --- editbox ---------------------------------------------------------- */
static const OpenEPL_PropertyDesc EDIT_PROPS[] = {
    { "text",   OE_SDT_TEXT, "",        NULL },
    { "left",   OE_SDT_INT,  "0",       NULL },
    { "top",    OE_SDT_INT,  "0",       NULL },
    { "width",  OE_SDT_INT,  "160",     NULL },
    { "height", OE_SDT_INT,  "26",      NULL },
    ANCHORS,
    { "color",  OE_SDT_TEXT, "#1f2328", "color" },
};
static const OpenEPL_EventDesc EDIT_EVENTS[] = { { "change", 0, NULL } };

/* --- checkbox --------------------------------------------------------- */
static const OpenEPL_PropertyDesc CHECK_PROPS[] = {
    { "text",    OE_SDT_TEXT, "Check me", NULL },
    { "checked", OE_SDT_BOOL, "false",    NULL },
    { "left",    OE_SDT_INT,  "0",        NULL },
    { "top",     OE_SDT_INT,  "0",        NULL },
    { "width",   OE_SDT_INT,  "140",      NULL },
    { "height",  OE_SDT_INT,  "24",       NULL },
    ANCHORS,
    { "color",   OE_SDT_TEXT, "#1f2328",  "color" },
};
static const OpenEPL_EventDesc CHECK_EVENTS[] = { { "change", 0, NULL } };

/* --- groupbox --------------------------------------------------------- */
static const OpenEPL_PropertyDesc GROUP_PROPS[] = {
    { "text",         OE_SDT_TEXT, "Group",   NULL },
    { "left",         OE_SDT_INT,  "0",       NULL },
    { "top",          OE_SDT_INT,  "0",       NULL },
    { "width",        OE_SDT_INT,  "200",     NULL },
    { "height",       OE_SDT_INT,  "120",     NULL },
    ANCHORS,
    { "border_color", OE_SDT_TEXT, "#d0d7de", "color" },
};

/* --- image ------------------------------------------------------------ */
static const OpenEPL_PropertyDesc IMAGE_PROPS[] = {
    { "source", OE_SDT_TEXT, "",    "file" },
    { "left",   OE_SDT_INT,  "0",   NULL },
    { "top",    OE_SDT_INT,  "0",   NULL },
    { "width",  OE_SDT_INT,  "120", NULL },
    { "height", OE_SDT_INT,  "120", NULL },
    ANCHORS,
};

/* --- progressbar ------------------------------------------------------ */
static const OpenEPL_PropertyDesc PROG_PROPS[] = {
    { "value",  OE_SDT_INT, "50",  NULL },
    { "left",   OE_SDT_INT, "0",   NULL },
    { "top",    OE_SDT_INT, "0",   NULL },
    { "width",  OE_SDT_INT, "200", NULL },
    { "height", OE_SDT_INT, "16",  NULL },
    ANCHORS,
};


/* --- combobox / listbox ----------------------------------------------- *
 *
 * `items` is ONE text with a newline between entries, not a `text[]`.
 *
 * That is forced, not preferred. A property value is a literal at the D10
 * boundary (backend/src/lib.rs `property_text`), so an aggregate cannot be
 * written in a form; and there is no expression form for a bare component id,
 * so the `thing_count`/`thing_at` command pair libs/README.md reaches for —
 * `combobox_add(list, "Red")` — has nothing to name the list with. A delimited
 * text is the only shape that both a designer inspector and a running
 * subroutine can write today: `list.items = concat(list.items, "\nPurple")`.
 *
 * `selected` counts from 1 and answers 0 for nothing selected, like every
 * other position in the language. `count` is read-only.
 */
static const OpenEPL_PropertyDesc COMBO_PROPS[] = {
    { "items",    OE_SDT_TEXT, "",    "multiline" },
    { "selected", OE_SDT_INT,  "0",   NULL },
    { "count",    OE_SDT_INT,  "0",   NULL },
    { "left",     OE_SDT_INT,  "0",   NULL },
    { "top",      OE_SDT_INT,  "0",   NULL },
    { "width",    OE_SDT_INT,  "160", NULL },
    { "height",   OE_SDT_INT,  "28",  NULL },
    ANCHORS,
    { "enabled",  OE_SDT_BOOL, "true", NULL },
};
/* `change`, not `changed`: the palette already spells this event `change` on
 * editbox and checkbox, and one vocabulary for one concept is worth more than
 * matching the word a request happened to use. */
static const OpenEPL_EventDesc COMBO_EVENTS[] = { { "change", 0, NULL } };

static const OpenEPL_PropertyDesc LIST_PROPS[] = {
    { "items",    OE_SDT_TEXT, "",    "multiline" },
    { "selected", OE_SDT_INT,  "0",   NULL },
    { "count",    OE_SDT_INT,  "0",   NULL },
    { "left",     OE_SDT_INT,  "0",   NULL },
    { "top",      OE_SDT_INT,  "0",   NULL },
    { "width",    OE_SDT_INT,  "160", NULL },
    { "height",   OE_SDT_INT,  "120", NULL },
    ANCHORS,
    { "enabled",  OE_SDT_BOOL, "true", NULL },
};
static const OpenEPL_EventDesc LIST_EVENTS[] = { { "change", 0, NULL } };

/* --- radiobutton ------------------------------------------------------ *
 *
 * Exclusion is by `group` NAME rather than by containment, because the
 * component tree is flat: a form holds children, and a groupbox holds nothing
 * (openepl_ir::Form). Naming the group is the same answer `action` gives to
 * the same problem, and it survives the designer moving a button out of the
 * rectangle it happened to be drawn over.
 */
static const OpenEPL_PropertyDesc RADIO_PROPS[] = {
    { "text",    OE_SDT_TEXT, "Option",  NULL },
    { "group",   OE_SDT_TEXT, "default", NULL },
    { "checked", OE_SDT_BOOL, "false",   NULL },
    { "left",    OE_SDT_INT,  "0",       NULL },
    { "top",     OE_SDT_INT,  "0",       NULL },
    { "width",   OE_SDT_INT,  "140",     NULL },
    { "height",  OE_SDT_INT,  "24",      NULL },
    ANCHORS,
    { "color",   OE_SDT_TEXT, "#1f2328", "color" },
};
static const OpenEPL_EventDesc RADIO_EVENTS[] = { { "change", 0, NULL } };

/* --- memo ------------------------------------------------------------- *
 *
 * The `multiline` editor hint has been in ABI v2 with nothing consuming it;
 * this is the component it was written for. An inspector offering a one-line
 * field for a paragraph is the whole reason the hint exists.
 */
static const OpenEPL_PropertyDesc MEMO_PROPS[] = {
    { "text",   OE_SDT_TEXT, "",       "multiline" },
    { "left",   OE_SDT_INT,  "0",      NULL },
    { "top",    OE_SDT_INT,  "0",      NULL },
    { "width",  OE_SDT_INT,  "240",    NULL },
    { "height", OE_SDT_INT,  "100",    NULL },
    ANCHORS,
    { "color",  OE_SDT_TEXT, "#1f2328", "color" },
    { "enabled", OE_SDT_BOOL, "true",  NULL },
};
static const OpenEPL_EventDesc MEMO_EVENTS[] = { { "change", 0, NULL } };

/* --- slider ----------------------------------------------------------- *
 *
 * `min`/`max` are the range and `value` is where the handle sits. Unlike a
 * progressbar this reports back, so it carries `change`.
 */
static const OpenEPL_PropertyDesc SLIDER_PROPS[] = {
    { "value",   OE_SDT_INT,  "50",   NULL },
    { "min",     OE_SDT_INT,  "0",    NULL },
    { "max",     OE_SDT_INT,  "100",  NULL },
    { "left",    OE_SDT_INT,  "0",    NULL },
    { "top",     OE_SDT_INT,  "0",    NULL },
    { "width",   OE_SDT_INT,  "200",  NULL },
    { "height",  OE_SDT_INT,  "20",   NULL },
    ANCHORS,
    { "enabled", OE_SDT_BOOL, "true", NULL },
};
static const OpenEPL_EventDesc SLIDER_EVENTS[] = { { "change", 0, NULL } };

/* --- spinner ---------------------------------------------------------- *
 *
 * A number with the two buttons that step it. `step` is what one press moves,
 * and the value is clamped to `min`..`max` however it was reached — typed,
 * stepped, or assigned from a subroutine — because a spinner whose bounds hold
 * only for the arrows is not bounded.
 */
static const OpenEPL_PropertyDesc SPIN_PROPS[] = {
    { "value",   OE_SDT_INT,  "0",    NULL },
    { "min",     OE_SDT_INT,  "0",    NULL },
    { "max",     OE_SDT_INT,  "100",  NULL },
    { "step",    OE_SDT_INT,  "1",    NULL },
    { "left",    OE_SDT_INT,  "0",    NULL },
    { "top",     OE_SDT_INT,  "0",    NULL },
    { "width",   OE_SDT_INT,  "110",  NULL },
    { "height",  OE_SDT_INT,  "28",   NULL },
    ANCHORS,
    { "enabled", OE_SDT_BOOL, "true", NULL },
};
static const OpenEPL_EventDesc SPIN_EVENTS[] = { { "change", 0, NULL } };

/* --- action ----------------------------------------------------------- *
 *
 * The one thing in Delphi this language had no answer for: the text, the
 * enabled state and the code behind a command live in ONE place, and every
 * control that offers that command follows it.  A button points at an action
 * through its `action` property; disabling the action greys the button.
 *
 * The reference is by `name` rather than by the component's identifier
 * because a property value is a literal (backend/src/lib.rs) and component
 * identifiers deliberately never reach the binary.
 */
static const OpenEPL_PropertyDesc ACTION_PROPS[] = {
    { "name",     OE_SDT_TEXT, "",        NULL },
    { "text",     OE_SDT_TEXT, "",        NULL },
    { "shortcut", OE_SDT_TEXT, "",        NULL },
    { "enabled",  OE_SDT_BOOL, "true",    NULL },
};
static const OpenEPL_EventDesc ACTION_EVENTS[] = { { "execute", 0, NULL } };

/* --- grid / datasource ------------------------------------------------ *
 *
 * A grid's data is the property that most wants to be a `text[][]`, and it
 * cannot be one for the two reasons `items` above cannot: a property value is
 * a literal, and a bare component id is not an expression.  So `rows` is ONE
 * text — a newline between rows, a tab between cells — and `columns` is the
 * header, tab-separated.  A cell can hold neither character, and there is no
 * escape for them: an escape the inspector shows and a program must spell is
 * worse than a stated limit.
 *
 * The commands are what make that representation bearable.  `grid_add_row`
 * and `grid_set_cell` put real data in from a subroutine with no string
 * building, and `grid_cell` reads one back.  They take the grid's `name` —
 * the same answer `action` gives to the same problem — because nothing else
 * a program can write names a component.
 *
 * `bind` names a datasource.  While one by that name exists the grid shows
 * ITS rows, and every grid bound to it shows the same rows; the grid's own
 * `rows` are what it falls back to.  A grid's commands reach whichever table
 * it is showing, so a program written against an unbound grid keeps working
 * when a datasource is wired in.  This is the shape that lets a `database`
 * kit hand a query result to a datasource and have it on screen with no code
 * between.
 *
 * `selected` counts from 1 and is 0 for no row; `select` hands the handler
 * that position, and `activate` — a double-click, or Enter on the selected
 * row — hands it the same.  `count` is read-only.
 */
static const OpenEPL_PropertyDesc GRID_PROPS[] = {
    { "name",     OE_SDT_TEXT, "",     NULL },
    { "bind",     OE_SDT_TEXT, "",     NULL },
    { "columns",  OE_SDT_TEXT, "",     NULL },
    { "rows",     OE_SDT_TEXT, "",     "multiline" },
    { "selected", OE_SDT_INT,  "0",    NULL },
    { "count",    OE_SDT_INT,  "0",    NULL },
    { "left",     OE_SDT_INT,  "0",    NULL },
    { "top",      OE_SDT_INT,  "0",    NULL },
    { "width",    OE_SDT_INT,  "320",  NULL },
    { "height",   OE_SDT_INT,  "160",  NULL },
    ANCHORS,
    { "enabled",  OE_SDT_BOOL, "true", NULL },
};
static const int32_t ROW_PARAM[] = { OE_SDT_INT };
static const OpenEPL_EventDesc GRID_EVENTS[] = {
    { "select",   1, ROW_PARAM },
    { "activate", 1, ROW_PARAM },
};

/* A datasource is rows with no rectangle: filled once, shown by every grid
 * that binds it.  It has no events — the grids watch it, not the program. */
static const OpenEPL_PropertyDesc DATASOURCE_PROPS[] = {
    { "name",    OE_SDT_TEXT, "", NULL },
    { "columns", OE_SDT_TEXT, "", NULL },
    { "rows",    OE_SDT_TEXT, "", "multiline" },
    { "count",   OE_SDT_INT,  "0", NULL },
};

#define N(a) (int32_t)(sizeof(a) / sizeof((a)[0]))

/* One signature table per shape; the grid and datasource families share
 * them, and differ only in which component the name is looked up among. */
static const int32_t A_NAME[]          = { OE_SDT_TEXT };
static const int32_t A_NAME_ROW[]      = { OE_SDT_TEXT, OE_SDT_TEXT };
static const int32_t A_NAME_CELL[]     = { OE_SDT_TEXT, OE_SDT_INT, OE_SDT_INT };
static const int32_t A_NAME_CELL_VAL[] = { OE_SDT_TEXT, OE_SDT_INT, OE_SDT_INT, OE_SDT_TEXT };

static const OpenEPL_CommandDesc UI_COMMANDS[] = {
    { "grid_clear",           "ui_grid_clear",           OE_SDT_BOOL, 1, A_NAME },
    { "grid_add_row",         "ui_grid_add_row",         OE_SDT_INT,  2, A_NAME_ROW },
    { "grid_cell",            "ui_grid_cell",            OE_SDT_TEXT, 3, A_NAME_CELL },
    { "grid_set_cell",        "ui_grid_set_cell",        OE_SDT_BOOL, 4, A_NAME_CELL_VAL },
    { "grid_row_count",       "ui_grid_row_count",       OE_SDT_INT,  1, A_NAME },
    { "datasource_clear",     "ui_datasource_clear",     OE_SDT_BOOL, 1, A_NAME },
    { "datasource_add_row",   "ui_datasource_add_row",   OE_SDT_INT,  2, A_NAME_ROW },
    { "datasource_cell",      "ui_datasource_cell",      OE_SDT_TEXT, 3, A_NAME_CELL },
    { "datasource_set_cell",  "ui_datasource_set_cell",  OE_SDT_BOOL, 4, A_NAME_CELL_VAL },
    { "datasource_row_count", "ui_datasource_row_count", OE_SDT_INT,  1, A_NAME },
};
#define VISUAL OE_COMPONENT_VISUAL
#define NONVISUAL OE_COMPONENT_NONVISUAL

static const OpenEPL_ComponentDesc UI_COMPONENTS[] = {
    { "form",   OE_ROLE_WINDOW, N(FORM_PROPS),   FORM_PROPS,   N(FORM_EVENTS),   FORM_EVENTS,   VISUAL },
    { "button", OE_ROLE_BUTTON, N(BUTTON_PROPS), BUTTON_PROPS, N(BUTTON_EVENTS), BUTTON_EVENTS, VISUAL },
    { "label",  OE_ROLE_LABEL,  N(LABEL_PROPS),  LABEL_PROPS,  0,                0,             VISUAL },
    { "editbox", OE_ROLE_TEXTBOX, N(EDIT_PROPS),  EDIT_PROPS,   N(EDIT_EVENTS),   EDIT_EVENTS,  VISUAL },
    { "checkbox", OE_ROLE_CHECKBOX, N(CHECK_PROPS), CHECK_PROPS, N(CHECK_EVENTS), CHECK_EVENTS, VISUAL },
    { "groupbox", OE_ROLE_GROUP,  N(GROUP_PROPS),  GROUP_PROPS,  0,               0,            VISUAL },
    { "image",   OE_ROLE_UNKNOWN, N(IMAGE_PROPS),  IMAGE_PROPS,  0,               0,            VISUAL },
    { "progressbar", OE_ROLE_UNKNOWN, N(PROG_PROPS), PROG_PROPS, 0,               0,            VISUAL },
    { "combobox", OE_ROLE_LIST, N(COMBO_PROPS), COMBO_PROPS, N(COMBO_EVENTS), COMBO_EVENTS, VISUAL },
    { "listbox", OE_ROLE_LIST, N(LIST_PROPS), LIST_PROPS, N(LIST_EVENTS), LIST_EVENTS, VISUAL },
    /* No OE_ROLE_RADIO exists in abi/openepl_abi.h, and that header is not
     * this library's to extend; checkbox is the nearest true role — a
     * two-state control that announces its state. */
    { "radiobutton", OE_ROLE_CHECKBOX, N(RADIO_PROPS), RADIO_PROPS, N(RADIO_EVENTS), RADIO_EVENTS, VISUAL },
    { "memo", OE_ROLE_TEXTBOX, N(MEMO_PROPS), MEMO_PROPS, N(MEMO_EVENTS), MEMO_EVENTS, VISUAL },
    { "slider", OE_ROLE_UNKNOWN, N(SLIDER_PROPS), SLIDER_PROPS, N(SLIDER_EVENTS), SLIDER_EVENTS, VISUAL },
    { "spinner", OE_ROLE_TEXTBOX, N(SPIN_PROPS), SPIN_PROPS, N(SPIN_EVENTS), SPIN_EVENTS, VISUAL },
    { "action", OE_ROLE_UNKNOWN, N(ACTION_PROPS), ACTION_PROPS, N(ACTION_EVENTS), ACTION_EVENTS, NONVISUAL },
    /* No table role exists in abi/openepl_abi.h; a list of rows is the
     * nearest true one, and a reader stepping through rows is served by it. */
    { "grid", OE_ROLE_LIST, N(GRID_PROPS), GRID_PROPS, N(GRID_EVENTS), GRID_EVENTS, VISUAL },
    { "datasource", OE_ROLE_UNKNOWN, N(DATASOURCE_PROPS), DATASOURCE_PROPS, 0, 0, NONVISUAL },
};

static const OpenEPL_LibInfo UI_INFO = {
    OPENEPL_ABI_VERSION,
    "ui",
    "openepl-ui-0000-0000-0000-000000000003",
    0, 1, 0,
    N(UI_COMMANDS), UI_COMMANDS,
    N(UI_COMPONENTS), UI_COMPONENTS,
};

const OpenEPL_LibInfo *openepl_get_lib_info(void) { return &UI_INFO; }
