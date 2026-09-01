/* "text" library metadata (design-time only; compiled into the introspection
 * .so, never a shipped program — same split as core_libinfo.c).
 *
 * Core already owns text_eq, text_to_int and text_to_double inside this
 * prefix, and owns length/uppercase/lowercase/trim/substr/find/replace/
 * concat/repeat/reverse outside it. None of them appear here. */
#include "openepl_abi.h"

void text_starts_with(OpenEPL_Slot *, int32_t, OpenEPL_Slot *);
void text_ends_with(OpenEPL_Slot *, int32_t, OpenEPL_Slot *);
void text_contains(OpenEPL_Slot *, int32_t, OpenEPL_Slot *);
void text_equals_ignore_case(OpenEPL_Slot *, int32_t, OpenEPL_Slot *);
void text_index_of(OpenEPL_Slot *, int32_t, OpenEPL_Slot *);
void text_last_index_of(OpenEPL_Slot *, int32_t, OpenEPL_Slot *);
void text_count(OpenEPL_Slot *, int32_t, OpenEPL_Slot *);
void text_compare(OpenEPL_Slot *, int32_t, OpenEPL_Slot *);
void text_trim_start(OpenEPL_Slot *, int32_t, OpenEPL_Slot *);
void text_trim_end(OpenEPL_Slot *, int32_t, OpenEPL_Slot *);
void text_pad_left(OpenEPL_Slot *, int32_t, OpenEPL_Slot *);
void text_pad_right(OpenEPL_Slot *, int32_t, OpenEPL_Slot *);
void text_title_case(OpenEPL_Slot *, int32_t, OpenEPL_Slot *);
void text_insert(OpenEPL_Slot *, int32_t, OpenEPL_Slot *);
void text_remove(OpenEPL_Slot *, int32_t, OpenEPL_Slot *);
void text_char_at(OpenEPL_Slot *, int32_t, OpenEPL_Slot *);
void text_char_code(OpenEPL_Slot *, int32_t, OpenEPL_Slot *);
void text_from_code(OpenEPL_Slot *, int32_t, OpenEPL_Slot *);
void text_split_count(OpenEPL_Slot *, int32_t, OpenEPL_Slot *);
void text_split_at(OpenEPL_Slot *, int32_t, OpenEPL_Slot *);

static const int32_t P_T[]   = { OE_SDT_TEXT };
static const int32_t P_TT[]  = { OE_SDT_TEXT, OE_SDT_TEXT };
static const int32_t P_TI[]  = { OE_SDT_TEXT, OE_SDT_INT };
static const int32_t P_TIT[] = { OE_SDT_TEXT, OE_SDT_INT, OE_SDT_TEXT };
static const int32_t P_TII[] = { OE_SDT_TEXT, OE_SDT_INT, OE_SDT_INT };
static const int32_t P_TTI[] = { OE_SDT_TEXT, OE_SDT_TEXT, OE_SDT_INT };
static const int32_t P_I[]   = { OE_SDT_INT };

static const OpenEPL_CommandDesc TEXT_COMMANDS[] = {
    { "text_starts_with",        "text_starts_with",        OE_SDT_BOOL, 2, P_TT  },
    { "text_ends_with",          "text_ends_with",          OE_SDT_BOOL, 2, P_TT  },
    { "text_contains",           "text_contains",           OE_SDT_BOOL, 2, P_TT  },
    { "text_equals_ignore_case", "text_equals_ignore_case", OE_SDT_BOOL, 2, P_TT  },
    { "text_index_of",           "text_index_of",           OE_SDT_INT,  2, P_TT  },
    { "text_last_index_of",      "text_last_index_of",      OE_SDT_INT,  2, P_TT  },
    { "text_count",              "text_count",              OE_SDT_INT,  2, P_TT  },
    { "text_compare",            "text_compare",            OE_SDT_INT,  2, P_TT  },
    { "text_trim_start",         "text_trim_start",         OE_SDT_TEXT, 1, P_T   },
    { "text_trim_end",           "text_trim_end",           OE_SDT_TEXT, 1, P_T   },
    { "text_pad_left",           "text_pad_left",           OE_SDT_TEXT, 3, P_TIT },
    { "text_pad_right",          "text_pad_right",          OE_SDT_TEXT, 3, P_TIT },
    { "text_title_case",         "text_title_case",         OE_SDT_TEXT, 1, P_T   },
    { "text_insert",             "text_insert",             OE_SDT_TEXT, 3, P_TIT },
    { "text_remove",             "text_remove",             OE_SDT_TEXT, 3, P_TII },
    { "text_char_at",            "text_char_at",            OE_SDT_TEXT, 2, P_TI  },
    { "text_char_code",          "text_char_code",          OE_SDT_INT,  2, P_TI  },
    { "text_from_code",          "text_from_code",          OE_SDT_TEXT, 1, P_I   },
    { "text_split_count",        "text_split_count",        OE_SDT_INT,  2, P_TT  },
    { "text_split_at",           "text_split_at",           OE_SDT_TEXT, 3, P_TTI },
};

static const OpenEPL_LibInfo TEXT_INFO = {
    OPENEPL_ABI_VERSION,
    "text",
    "openepl-text-0000-0000-0000-746578740001",
    0, 1, 0,
    (int32_t)(sizeof(TEXT_COMMANDS) / sizeof(TEXT_COMMANDS[0])),
    TEXT_COMMANDS,
    0, 0,
};

const OpenEPL_LibInfo *openepl_get_lib_info(void) {
    return &TEXT_INFO;
}
