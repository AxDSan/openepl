/* "ui" support library metadata — visual component descriptors (D9/D11).
 *
 * DESIGN-TIME ONLY: compiled into the introspection .so, never into a shipped
 * program (same split as core_libinfo.c, ADR 0003/D12). The compiler reads this
 * to learn which component types exist, what properties and events they have,
 * and their accessibility roles (D16).
 */
#include "openepl_abi.h"

/* --- form ------------------------------------------------------------- */
static const OpenEPL_PropertyDesc FORM_PROPS[] = {
    { "title",            OE_SDT_TEXT, "OpenEPL Application" },
    { "width",            OE_SDT_INT,  "800" },
    { "height",           OE_SDT_INT,  "600" },
    { "background_color", OE_SDT_TEXT, "#1e2233" },
};
static const OpenEPL_EventDesc FORM_EVENTS[] = { { "load" } };

/* --- button ----------------------------------------------------------- */
static const OpenEPL_PropertyDesc BUTTON_PROPS[] = {
    { "text",             OE_SDT_TEXT, "Button" },
    { "left",             OE_SDT_INT,  "0" },
    { "top",              OE_SDT_INT,  "0" },
    { "width",            OE_SDT_INT,  "120" },
    { "height",           OE_SDT_INT,  "36" },
    { "background_color", OE_SDT_TEXT, "#4a86e8" },
    { "color",            OE_SDT_TEXT, "#ffffff" },
    { "border_radius",    OE_SDT_INT,  "6" },
};
static const OpenEPL_EventDesc BUTTON_EVENTS[] = { { "click" } };

/* --- label ------------------------------------------------------------ */
static const OpenEPL_PropertyDesc LABEL_PROPS[] = {
    { "text",  OE_SDT_TEXT, "Label" },
    { "left",  OE_SDT_INT,  "0" },
    { "top",   OE_SDT_INT,  "0" },
    { "width", OE_SDT_INT,  "200" },
    { "color", OE_SDT_TEXT, "#ffffff" },
};

/* --- editbox ---------------------------------------------------------- */
static const OpenEPL_PropertyDesc EDIT_PROPS[] = {
    { "text",   OE_SDT_TEXT, "" },
    { "left",   OE_SDT_INT,  "0" },
    { "top",    OE_SDT_INT,  "0" },
    { "width",  OE_SDT_INT,  "160" },
    { "height", OE_SDT_INT,  "26" },
    { "color",  OE_SDT_TEXT, "#1f2328" },
};
static const OpenEPL_EventDesc EDIT_EVENTS[] = { { "change" } };

/* --- checkbox --------------------------------------------------------- */
static const OpenEPL_PropertyDesc CHECK_PROPS[] = {
    { "text",    OE_SDT_TEXT, "Check me" },
    { "checked", OE_SDT_BOOL, "false" },
    { "left",    OE_SDT_INT,  "0" },
    { "top",     OE_SDT_INT,  "0" },
    { "color",   OE_SDT_TEXT, "#1f2328" },
};
static const OpenEPL_EventDesc CHECK_EVENTS[] = { { "change" } };

/* --- groupbox --------------------------------------------------------- */
static const OpenEPL_PropertyDesc GROUP_PROPS[] = {
    { "text",         OE_SDT_TEXT, "Group" },
    { "left",         OE_SDT_INT,  "0" },
    { "top",          OE_SDT_INT,  "0" },
    { "width",        OE_SDT_INT,  "200" },
    { "height",       OE_SDT_INT,  "120" },
    { "border_color", OE_SDT_TEXT, "#d0d7de" },
};

/* --- image ------------------------------------------------------------ */
static const OpenEPL_PropertyDesc IMAGE_PROPS[] = {
    { "source", OE_SDT_TEXT, "" },
    { "left",   OE_SDT_INT,  "0" },
    { "top",    OE_SDT_INT,  "0" },
    { "width",  OE_SDT_INT,  "120" },
    { "height", OE_SDT_INT,  "120" },
};

/* --- progressbar ------------------------------------------------------ */
static const OpenEPL_PropertyDesc PROG_PROPS[] = {
    { "value",  OE_SDT_INT, "50" },
    { "left",   OE_SDT_INT, "0" },
    { "top",    OE_SDT_INT, "0" },
    { "width",  OE_SDT_INT, "200" },
    { "height", OE_SDT_INT, "16" },
};

#define N(a) (int32_t)(sizeof(a) / sizeof((a)[0]))

static const OpenEPL_ComponentDesc UI_COMPONENTS[] = {
    { "form",   OE_ROLE_WINDOW, N(FORM_PROPS),   FORM_PROPS,   N(FORM_EVENTS),   FORM_EVENTS },
    { "button", OE_ROLE_BUTTON, N(BUTTON_PROPS), BUTTON_PROPS, N(BUTTON_EVENTS), BUTTON_EVENTS },
    { "label",  OE_ROLE_LABEL,  N(LABEL_PROPS),  LABEL_PROPS,  0,                0 },
    { "editbox", OE_ROLE_TEXTBOX, N(EDIT_PROPS),  EDIT_PROPS,   N(EDIT_EVENTS),   EDIT_EVENTS },
    { "checkbox", OE_ROLE_CHECKBOX, N(CHECK_PROPS), CHECK_PROPS, N(CHECK_EVENTS), CHECK_EVENTS },
    { "groupbox", OE_ROLE_GROUP,  N(GROUP_PROPS),  GROUP_PROPS,  0,               0 },
    { "image",   OE_ROLE_UNKNOWN, N(IMAGE_PROPS),  IMAGE_PROPS,  0,               0 },
    { "progressbar", OE_ROLE_UNKNOWN, N(PROG_PROPS), PROG_PROPS, 0,               0 },
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
