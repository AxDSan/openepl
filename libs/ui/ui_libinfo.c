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
    { "background_color", OE_SDT_TEXT, "#1e2233",             "color" },
};
static const OpenEPL_EventDesc FORM_EVENTS[] = { { "load" } };

/* --- button ----------------------------------------------------------- */
static const OpenEPL_PropertyDesc BUTTON_PROPS[] = {
    { "text",             OE_SDT_TEXT, "Button",  NULL },
    { "left",             OE_SDT_INT,  "0",       NULL },
    { "top",              OE_SDT_INT,  "0",       NULL },
    { "width",            OE_SDT_INT,  "120",     NULL },
    { "height",           OE_SDT_INT,  "36",      NULL },
    { "background_color", OE_SDT_TEXT, "#4a86e8", "color" },
    { "color",            OE_SDT_TEXT, "#ffffff", "color" },
    { "border_radius",    OE_SDT_INT,  "6",       NULL },
    { "enabled",          OE_SDT_BOOL, "true",    NULL },
    { "action",           OE_SDT_TEXT, "",        NULL },
};
static const OpenEPL_EventDesc BUTTON_EVENTS[] = { { "click" } };

/* --- label ------------------------------------------------------------ */
static const OpenEPL_PropertyDesc LABEL_PROPS[] = {
    { "text",  OE_SDT_TEXT, "Label",   NULL },
    { "left",  OE_SDT_INT,  "0",       NULL },
    { "top",   OE_SDT_INT,  "0",       NULL },
    { "width", OE_SDT_INT,  "200",     NULL },
    { "color", OE_SDT_TEXT, "#ffffff", "color" },
};

/* --- editbox ---------------------------------------------------------- */
static const OpenEPL_PropertyDesc EDIT_PROPS[] = {
    { "text",   OE_SDT_TEXT, "",        NULL },
    { "left",   OE_SDT_INT,  "0",       NULL },
    { "top",    OE_SDT_INT,  "0",       NULL },
    { "width",  OE_SDT_INT,  "160",     NULL },
    { "height", OE_SDT_INT,  "26",      NULL },
    { "color",  OE_SDT_TEXT, "#1f2328", "color" },
};
static const OpenEPL_EventDesc EDIT_EVENTS[] = { { "change" } };

/* --- checkbox --------------------------------------------------------- */
static const OpenEPL_PropertyDesc CHECK_PROPS[] = {
    { "text",    OE_SDT_TEXT, "Check me", NULL },
    { "checked", OE_SDT_BOOL, "false",    NULL },
    { "left",    OE_SDT_INT,  "0",        NULL },
    { "top",     OE_SDT_INT,  "0",        NULL },
    { "color",   OE_SDT_TEXT, "#1f2328",  "color" },
};
static const OpenEPL_EventDesc CHECK_EVENTS[] = { { "change" } };

/* --- groupbox --------------------------------------------------------- */
static const OpenEPL_PropertyDesc GROUP_PROPS[] = {
    { "text",         OE_SDT_TEXT, "Group",   NULL },
    { "left",         OE_SDT_INT,  "0",       NULL },
    { "top",          OE_SDT_INT,  "0",       NULL },
    { "width",        OE_SDT_INT,  "200",     NULL },
    { "height",       OE_SDT_INT,  "120",     NULL },
    { "border_color", OE_SDT_TEXT, "#d0d7de", "color" },
};

/* --- image ------------------------------------------------------------ */
static const OpenEPL_PropertyDesc IMAGE_PROPS[] = {
    { "source", OE_SDT_TEXT, "",    "file" },
    { "left",   OE_SDT_INT,  "0",   NULL },
    { "top",    OE_SDT_INT,  "0",   NULL },
    { "width",  OE_SDT_INT,  "120", NULL },
    { "height", OE_SDT_INT,  "120", NULL },
};

/* --- progressbar ------------------------------------------------------ */
static const OpenEPL_PropertyDesc PROG_PROPS[] = {
    { "value",  OE_SDT_INT, "50",  NULL },
    { "left",   OE_SDT_INT, "0",   NULL },
    { "top",    OE_SDT_INT, "0",   NULL },
    { "width",  OE_SDT_INT, "200", NULL },
    { "height", OE_SDT_INT, "16",  NULL },
};

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
static const OpenEPL_EventDesc ACTION_EVENTS[] = { { "execute" } };

#define N(a) (int32_t)(sizeof(a) / sizeof((a)[0]))
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
    { "action", OE_ROLE_UNKNOWN, N(ACTION_PROPS), ACTION_PROPS, N(ACTION_EVENTS), ACTION_EVENTS, NONVISUAL },
};

static const OpenEPL_LibInfo UI_INFO = {
    OPENEPL_ABI_VERSION,
    "ui",
    "openepl-ui-0000-0000-0000-000000000003",
    0, 1, 0,
    0, 0,                                  /* no commands yet */
    N(UI_COMPONENTS), UI_COMPONENTS,
};

const OpenEPL_LibInfo *openepl_get_lib_info(void) { return &UI_INFO; }
