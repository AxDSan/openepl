/* "json" library metadata (design-time only; compiled into the introspection
 * .so, never a shipped program — the same split as core_libinfo.c).
 *
 * The path grammar every command shares is documented at the top of
 * json_cmds.c, because the path IS the interface: with no record type, a
 * dotted path is how a program names a place inside a document. */
#include "openepl_abi.h"

void json_parse(OpenEPL_Slot *, int32_t, OpenEPL_Slot *);
void json_parse_file(OpenEPL_Slot *, int32_t, OpenEPL_Slot *);
void json_new_object(OpenEPL_Slot *, int32_t, OpenEPL_Slot *);
void json_new_array(OpenEPL_Slot *, int32_t, OpenEPL_Slot *);
void json_close(OpenEPL_Slot *, int32_t, OpenEPL_Slot *);
void json_close_all(OpenEPL_Slot *, int32_t, OpenEPL_Slot *);
void json_stringify(OpenEPL_Slot *, int32_t, OpenEPL_Slot *);
void json_stringify_pretty(OpenEPL_Slot *, int32_t, OpenEPL_Slot *);
void json_save(OpenEPL_Slot *, int32_t, OpenEPL_Slot *);
void json_type(OpenEPL_Slot *, int32_t, OpenEPL_Slot *);
void json_has(OpenEPL_Slot *, int32_t, OpenEPL_Slot *);
void json_count(OpenEPL_Slot *, int32_t, OpenEPL_Slot *);
void json_key_at(OpenEPL_Slot *, int32_t, OpenEPL_Slot *);
void json_get_text(OpenEPL_Slot *, int32_t, OpenEPL_Slot *);
void json_get_int(OpenEPL_Slot *, int32_t, OpenEPL_Slot *);
void json_get_double(OpenEPL_Slot *, int32_t, OpenEPL_Slot *);
void json_get_bool(OpenEPL_Slot *, int32_t, OpenEPL_Slot *);
void json_set_text(OpenEPL_Slot *, int32_t, OpenEPL_Slot *);
void json_set_int(OpenEPL_Slot *, int32_t, OpenEPL_Slot *);
void json_set_double(OpenEPL_Slot *, int32_t, OpenEPL_Slot *);
void json_set_bool(OpenEPL_Slot *, int32_t, OpenEPL_Slot *);
void json_set_null(OpenEPL_Slot *, int32_t, OpenEPL_Slot *);
void json_set_object(OpenEPL_Slot *, int32_t, OpenEPL_Slot *);
void json_set_array(OpenEPL_Slot *, int32_t, OpenEPL_Slot *);
void json_remove(OpenEPL_Slot *, int32_t, OpenEPL_Slot *);

static const int32_t P_T[]   = { OE_SDT_TEXT };
static const int32_t P_I[]   = { OE_SDT_INT };
static const int32_t P_IT[]  = { OE_SDT_INT, OE_SDT_TEXT };
static const int32_t P_ITT[] = { OE_SDT_INT, OE_SDT_TEXT, OE_SDT_TEXT };
static const int32_t P_ITI[] = { OE_SDT_INT, OE_SDT_TEXT, OE_SDT_INT };
static const int32_t P_ITD[] = { OE_SDT_INT, OE_SDT_TEXT, OE_SDT_DOUBLE };
static const int32_t P_ITB[] = { OE_SDT_INT, OE_SDT_TEXT, OE_SDT_BOOL };

static const OpenEPL_CommandDesc JSON_COMMANDS[] = {
    /* opening and closing a document */
    { "json_parse",             "json_parse",             OE_SDT_INT,    1, P_T   },
    { "json_parse_file",        "json_parse_file",        OE_SDT_INT,    1, P_T   },
    { "json_new_object",        "json_new_object",        OE_SDT_INT,    0, 0     },
    { "json_new_array",         "json_new_array",         OE_SDT_INT,    0, 0     },
    { "json_close",             "json_close",             OE_SDT_BOOL,   1, P_I   },
    { "json_close_all",         "json_close_all",         OE_SDT_INT,    0, 0     },

    /* writing it back out */
    { "json_stringify",         "json_stringify",         OE_SDT_TEXT,   1, P_I   },
    { "json_stringify_pretty",  "json_stringify_pretty",  OE_SDT_TEXT,   1, P_I   },
    { "json_save",              "json_save",              OE_SDT_BOOL,   2, P_IT  },

    /* asking about a place */
    { "json_type",              "json_type",              OE_SDT_TEXT,   2, P_IT  },
    { "json_has",               "json_has",               OE_SDT_BOOL,   2, P_IT  },
    { "json_count",             "json_count",             OE_SDT_INT,    2, P_IT  },
    { "json_key_at",            "json_key_at",            OE_SDT_TEXT,   3, P_ITI },

    /* reading a value */
    { "json_get_text",          "json_get_text",          OE_SDT_TEXT,   2, P_IT  },
    { "json_get_int",           "json_get_int",           OE_SDT_INT,    2, P_IT  },
    { "json_get_double",        "json_get_double",        OE_SDT_DOUBLE, 2, P_IT  },
    { "json_get_bool",          "json_get_bool",          OE_SDT_BOOL,   2, P_IT  },

    /* writing a value */
    { "json_set_text",          "json_set_text",          OE_SDT_BOOL,   3, P_ITT },
    { "json_set_int",           "json_set_int",           OE_SDT_BOOL,   3, P_ITI },
    { "json_set_double",        "json_set_double",        OE_SDT_BOOL,   3, P_ITD },
    { "json_set_bool",          "json_set_bool",          OE_SDT_BOOL,   3, P_ITB },
    { "json_set_null",          "json_set_null",          OE_SDT_BOOL,   2, P_IT  },
    { "json_set_object",        "json_set_object",        OE_SDT_BOOL,   2, P_IT  },
    { "json_set_array",         "json_set_array",         OE_SDT_BOOL,   2, P_IT  },
    { "json_remove",            "json_remove",            OE_SDT_BOOL,   2, P_IT  },
};

static const OpenEPL_LibInfo JSON_INFO = {
    OPENEPL_ABI_VERSION,
    "json",
    "openepl-json-4f2b-8c17-9ae3-6a5d1c07b3f2",
    0, 1, 0,
    (int32_t)(sizeof(JSON_COMMANDS) / sizeof(JSON_COMMANDS[0])),
    JSON_COMMANDS,
    0, 0,
};

const OpenEPL_LibInfo *openepl_get_lib_info(void) {
    return &JSON_INFO;
}
