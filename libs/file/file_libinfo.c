/* "file" library metadata (design-time only; compiled into the introspection
 * .so, never a shipped program — the same split as core_libinfo.c).
 *
 * Commands are referenced by symbol NAME, so this translation unit has no
 * dependency on the implementations and needs nothing but the ABI header. */
#include "openepl_abi.h"

/* --- implementations (in file_cmds.c) --------------------------------- */
#define D(sym) void sym(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv);
D(file_read_text)   D(file_write_text)  D(file_append_text) D(file_exists)
D(file_size)        D(file_delete)      D(file_copy)        D(file_move)
D(file_modified)    D(file_line_count)
D(file_open)        D(file_read_line)   D(file_at_end)      D(file_write_line)
D(file_close)       D(file_close_all)
D(dir_exists)       D(dir_create)       D(dir_delete)       D(dir_current)
D(dir_set_current)  D(dir_entry_count)  D(dir_entry)
D(path_join)        D(path_name)        D(path_parent)      D(path_extension)
D(path_absolute)
#undef D

static const int32_t P_T[]   = { OE_SDT_TEXT };
static const int32_t P_TT[]  = { OE_SDT_TEXT, OE_SDT_TEXT };
static const int32_t P_TI[]  = { OE_SDT_TEXT, OE_SDT_INT };
static const int32_t P_I[]   = { OE_SDT_INT };
static const int32_t P_IT[]  = { OE_SDT_INT, OE_SDT_TEXT };

static const OpenEPL_CommandDesc FILE_COMMANDS[] = {
    /* --- one-shot, path-level: the documented surface ------------------ */
    { "file_read_text",   "file_read_text",   OE_SDT_TEXT,  1, P_T  },
    { "file_write_text",  "file_write_text",  OE_SDT_BOOL,  2, P_TT },
    { "file_append_text", "file_append_text", OE_SDT_BOOL,  2, P_TT },
    { "file_exists",      "file_exists",      OE_SDT_BOOL,  1, P_T  },
    { "file_size",        "file_size",        OE_SDT_INT64, 1, P_T  },
    { "file_delete",      "file_delete",      OE_SDT_BOOL,  1, P_T  },
    { "file_copy",        "file_copy",        OE_SDT_BOOL,  2, P_TT },
    { "file_move",        "file_move",        OE_SDT_BOOL,  2, P_TT },
    { "file_modified",    "file_modified",    OE_SDT_INT64, 1, P_T  },
    { "file_line_count",  "file_line_count",  OE_SDT_INT,   1, P_T  },

    /* --- handles: the escape hatch for what does not fit in memory ----- */
    { "file_open",        "file_open",        OE_SDT_INT,   2, P_TT },
    { "file_read_line",   "file_read_line",   OE_SDT_TEXT,  1, P_I  },
    { "file_at_end",      "file_at_end",      OE_SDT_BOOL,  1, P_I  },
    { "file_write_line",  "file_write_line",  OE_SDT_BOOL,  2, P_IT },
    { "file_close",       "file_close",       OE_SDT_BOOL,  1, P_I  },
    { "file_close_all",   "file_close_all",   OE_SDT_INT,   0, 0    },

    /* --- directories --------------------------------------------------- */
    { "dir_exists",       "dir_exists",       OE_SDT_BOOL,  1, P_T  },
    { "dir_create",       "dir_create",       OE_SDT_BOOL,  1, P_T  },
    { "dir_delete",       "dir_delete",       OE_SDT_BOOL,  1, P_T  },
    { "dir_current",      "dir_current",      OE_SDT_TEXT,  0, 0    },
    { "dir_set_current",  "dir_set_current",  OE_SDT_BOOL,  1, P_T  },
    { "dir_entry_count",  "dir_entry_count",  OE_SDT_INT,   1, P_T  },
    { "dir_entry",        "dir_entry",        OE_SDT_TEXT,  2, P_TI },

    /* --- paths: pure text, and so infallible --------------------------- */
    { "path_join",        "path_join",        OE_SDT_TEXT,  2, P_TT },
    { "path_name",        "path_name",        OE_SDT_TEXT,  1, P_T  },
    { "path_parent",      "path_parent",      OE_SDT_TEXT,  1, P_T  },
    { "path_extension",   "path_extension",   OE_SDT_TEXT,  1, P_T  },
    { "path_absolute",    "path_absolute",    OE_SDT_TEXT,  1, P_T  },
};

static const OpenEPL_LibInfo FILE_INFO = {
    OPENEPL_ABI_VERSION,
    "file",
    "openepl-file-0000-0000-0000-000000000004",
    0, 1, 0,
    (int32_t)(sizeof(FILE_COMMANDS) / sizeof(FILE_COMMANDS[0])),
    FILE_COMMANDS,
    0, 0,
};

const OpenEPL_LibInfo *openepl_get_lib_info(void) {
    return &FILE_INFO;
}
