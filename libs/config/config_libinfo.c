/* "config" library metadata (design-time only; compiled into the introspection
 * .so, never a shipped program — same split as core_libinfo.c). Commands are
 * referenced by SYMBOL name, so this table needs none of the implementations. */
#include "openepl_abi.h"

void config_open(OpenEPL_Slot *, int32_t, OpenEPL_Slot *);
void config_create(OpenEPL_Slot *, int32_t, OpenEPL_Slot *);
void config_close(OpenEPL_Slot *, int32_t, OpenEPL_Slot *);
void config_close_all(OpenEPL_Slot *, int32_t, OpenEPL_Slot *);
void config_path(OpenEPL_Slot *, int32_t, OpenEPL_Slot *);
void config_save(OpenEPL_Slot *, int32_t, OpenEPL_Slot *);
void config_get(OpenEPL_Slot *, int32_t, OpenEPL_Slot *);
void config_get_int(OpenEPL_Slot *, int32_t, OpenEPL_Slot *);
void config_get_double(OpenEPL_Slot *, int32_t, OpenEPL_Slot *);
void config_get_bool(OpenEPL_Slot *, int32_t, OpenEPL_Slot *);
void config_has(OpenEPL_Slot *, int32_t, OpenEPL_Slot *);
void config_has_section(OpenEPL_Slot *, int32_t, OpenEPL_Slot *);
void config_set(OpenEPL_Slot *, int32_t, OpenEPL_Slot *);
void config_set_int(OpenEPL_Slot *, int32_t, OpenEPL_Slot *);
void config_set_double(OpenEPL_Slot *, int32_t, OpenEPL_Slot *);
void config_set_bool(OpenEPL_Slot *, int32_t, OpenEPL_Slot *);
void config_remove(OpenEPL_Slot *, int32_t, OpenEPL_Slot *);
void config_remove_section(OpenEPL_Slot *, int32_t, OpenEPL_Slot *);
void config_section_count(OpenEPL_Slot *, int32_t, OpenEPL_Slot *);
void config_section_at(OpenEPL_Slot *, int32_t, OpenEPL_Slot *);
void config_key_count(OpenEPL_Slot *, int32_t, OpenEPL_Slot *);
void config_key_at(OpenEPL_Slot *, int32_t, OpenEPL_Slot *);

static const int32_t P_T[]    = { OE_SDT_TEXT };
static const int32_t P_I[]    = { OE_SDT_INT };
static const int32_t P_IT[]   = { OE_SDT_INT, OE_SDT_TEXT };
static const int32_t P_II[]   = { OE_SDT_INT, OE_SDT_INT };
static const int32_t P_ITT[]  = { OE_SDT_INT, OE_SDT_TEXT, OE_SDT_TEXT };
static const int32_t P_ITI[]  = { OE_SDT_INT, OE_SDT_TEXT, OE_SDT_INT };
static const int32_t P_ITTT[] = { OE_SDT_INT, OE_SDT_TEXT, OE_SDT_TEXT, OE_SDT_TEXT };
static const int32_t P_ITTI[] = { OE_SDT_INT, OE_SDT_TEXT, OE_SDT_TEXT, OE_SDT_INT };
static const int32_t P_ITTD[] = { OE_SDT_INT, OE_SDT_TEXT, OE_SDT_TEXT, OE_SDT_DOUBLE };
static const int32_t P_ITTB[] = { OE_SDT_INT, OE_SDT_TEXT, OE_SDT_TEXT, OE_SDT_BOOL };

static const OpenEPL_CommandDesc CONFIG_COMMANDS[] = {
    /* opening and closing */
    { "config_open",           "config_open",           OE_SDT_INT,    1, P_T    },
    { "config_create",         "config_create",         OE_SDT_INT,    1, P_T    },
    { "config_close",          "config_close",          OE_SDT_BOOL,   1, P_I    },
    { "config_close_all",      "config_close_all",      OE_SDT_INT,    0, 0      },
    { "config_save",           "config_save",           OE_SDT_BOOL,   1, P_I    },
    { "config_path",           "config_path",           OE_SDT_TEXT,   1, P_I    },
    /* reading */
    { "config_get",            "config_get",            OE_SDT_TEXT,   3, P_ITT  },
    { "config_get_int",        "config_get_int",        OE_SDT_INT,    4, P_ITTI },
    { "config_get_double",     "config_get_double",     OE_SDT_DOUBLE, 4, P_ITTD },
    { "config_get_bool",       "config_get_bool",       OE_SDT_BOOL,   4, P_ITTB },
    { "config_has",            "config_has",            OE_SDT_BOOL,   3, P_ITT  },
    { "config_has_section",    "config_has_section",    OE_SDT_BOOL,   2, P_IT   },
    /* writing */
    { "config_set",            "config_set",            OE_SDT_BOOL,   4, P_ITTT },
    { "config_set_int",        "config_set_int",        OE_SDT_BOOL,   4, P_ITTI },
    { "config_set_double",     "config_set_double",     OE_SDT_BOOL,   4, P_ITTD },
    { "config_set_bool",       "config_set_bool",       OE_SDT_BOOL,   4, P_ITTB },
    { "config_remove",         "config_remove",         OE_SDT_BOOL,   3, P_ITT  },
    { "config_remove_section", "config_remove_section", OE_SDT_BOOL,   2, P_IT   },
    /* collections: count + indexed accessor */
    { "config_section_count",  "config_section_count",  OE_SDT_INT,    1, P_I    },
    { "config_section_at",     "config_section_at",     OE_SDT_TEXT,   2, P_II   },
    { "config_key_count",      "config_key_count",      OE_SDT_INT,    2, P_IT   },
    { "config_key_at",         "config_key_at",         OE_SDT_TEXT,   3, P_ITI  },
};

static const OpenEPL_LibInfo CONFIG_INFO = {
    OPENEPL_ABI_VERSION,
    "config",
    "openepl-config-0000-0000-0000-000000000005",
    0, 1, 0,
    (int32_t)(sizeof(CONFIG_COMMANDS) / sizeof(CONFIG_COMMANDS[0])),
    CONFIG_COMMANDS,
    0, 0,
};

const OpenEPL_LibInfo *openepl_get_lib_info(void) {
    return &CONFIG_INFO;
}
