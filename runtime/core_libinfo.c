/* Core library metadata (LibInfo / GetNewInf analog) — Phase 2.
 *
 * DESIGN-TIME METADATA ONLY.  This translation unit is compiled into the
 * introspection shared object (`libopenepl_core.so`) that the compiler dlopens
 * to read command signatures — it is NEVER linked into a shipped program.  The
 * table holds a pointer to every command name/symbol; if it entered a program's
 * link line it would anchor all ~40 commands and defeat `--gc-sections`
 *.  Command *implementations* ship; this catalog does not.
 */
#include "openepl_core.h"

/* Distinct parameter-tag arrays, shared across commands. */
static const int32_t P_I[]     = { OE_SDT_INT };
static const int32_t P_II[]    = { OE_SDT_INT, OE_SDT_INT };
static const int32_t P_I64[]   = { OE_SDT_INT64 };
static const int32_t P_D[]     = { OE_SDT_DOUBLE };
static const int32_t P_DD[]    = { OE_SDT_DOUBLE, OE_SDT_DOUBLE };
static const int32_t P_T[]     = { OE_SDT_TEXT };
static const int32_t P_TT[]    = { OE_SDT_TEXT, OE_SDT_TEXT };
static const int32_t P_TI[]    = { OE_SDT_TEXT, OE_SDT_INT };
static const int32_t P_TII[]   = { OE_SDT_TEXT, OE_SDT_INT, OE_SDT_INT };
static const int32_t P_TTT[]   = { OE_SDT_TEXT, OE_SDT_TEXT, OE_SDT_TEXT };
static const int32_t P_I64T[]  = { OE_SDT_INT64, OE_SDT_TEXT };
/* Aggregates. ANY_ARRAY/ANY_ELEM keep one `append` instead of one per element
 * type; the array carries its element tag, and the compiler checks the pair. */
static const int32_t P_A[]     = { OE_SDT_ANY_ARRAY };
static const int32_t P_AE[]    = { OE_SDT_ANY_ARRAY, OE_SDT_ANY_ELEM };
static const int32_t P_AI[]    = { OE_SDT_ANY_ARRAY, OE_SDT_INT };
static const int32_t P_AT[]    = { OE_SDT_ANY_ARRAY, OE_SDT_TEXT };
static const int32_t P_AII[]   = { OE_SDT_ANY_ARRAY, OE_SDT_INT, OE_SDT_INT };
static const int32_t P_B[]     = { OE_SDT_BIN };
static const int32_t P_BI[]    = { OE_SDT_BIN, OE_SDT_INT };
static const int32_t P_BII[]   = { OE_SDT_BIN, OE_SDT_INT, OE_SDT_INT };
/* Dictionaries: ANY_DICT/ANY_ELEM for the reason the arrays use their pair —
 * the dictionary carries its value tag, and the compiler checks it against the
 * value being stored. */
static const int32_t P_K[]     = { OE_SDT_ANY_DICT };
static const int32_t P_KT[]    = { OE_SDT_ANY_DICT, OE_SDT_TEXT };
static const int32_t P_KTE[]   = { OE_SDT_ANY_DICT, OE_SDT_TEXT, OE_SDT_ANY_ELEM };
/* Pointers. Offsets and sizes are INT64 so a buffer may exceed 2 GiB and a
 * 64-bit address round-trips whole. */
static const int32_t P_P[]        = { OE_SDT_PTR };
static const int32_t P_PI64[]     = { OE_SDT_PTR, OE_SDT_INT64 };
static const int32_t P_PI64I[]    = { OE_SDT_PTR, OE_SDT_INT64, OE_SDT_INT };
static const int32_t P_PI64I64[]  = { OE_SDT_PTR, OE_SDT_INT64, OE_SDT_INT64 };
static const int32_t P_PI64D[]    = { OE_SDT_PTR, OE_SDT_INT64, OE_SDT_DOUBLE };
static const int32_t P_PI64P[]    = { OE_SDT_PTR, OE_SDT_INT64, OE_SDT_PTR };
static const int32_t P_PI64T[]    = { OE_SDT_PTR, OE_SDT_INT64, OE_SDT_TEXT };
static const int32_t P_PPI64[]    = { OE_SDT_PTR, OE_SDT_PTR, OE_SDT_INT64 };

#define CMD(name, sym, ret, argc, tags) \
    { name, #sym, ret, argc, tags }

static const OpenEPL_CommandDesc CORE_COMMANDS[] = {
    /* I/O (void) */
    CMD("print_int",    oe_print_int,    OE_SDT_NULL, 1, P_I),
    CMD("print_int64",  oe_print_int64,  OE_SDT_NULL, 1, P_I64),
    CMD("print_double", oe_print_double, OE_SDT_NULL, 1, P_D),
    CMD("print_text",   oe_print_text,   OE_SDT_NULL, 1, P_T),
    /* input — the other half of the pair */
    CMD("read_line",    oe_read_line,    OE_SDT_TEXT, 0, NULL),
    CMD("input_ended",  oe_input_ended,  OE_SDT_BOOL, 0, NULL),
    CMD("ask",          oe_ask,          OE_SDT_TEXT, 1, P_T),
    /* what a failed `assert` runs: print the message and stop, failing */
    CMD("assert_failed", oe_assert_failed, OE_SDT_NULL, 1, P_T),
    /* errors — zero arity is what makes an out-parameter expressible */
    CMD("last_error_code", oe_last_error_code, OE_SDT_INT,  0, NULL),
    CMD("last_error_text", oe_last_error_text, OE_SDT_TEXT, 0, NULL),
    /* integer math */
    CMD("abs_int", oe_abs_int, OE_SDT_INT, 1, P_I),
    CMD("min_int", oe_min_int, OE_SDT_INT, 2, P_II),
    CMD("max_int", oe_max_int, OE_SDT_INT, 2, P_II),
    CMD("mod_int", oe_mod_int, OE_SDT_INT, 2, P_II),
    CMD("pow_int", oe_pow_int, OE_SDT_INT, 2, P_II),
    /* float math */
    CMD("sqrt",  oe_sqrt,  OE_SDT_DOUBLE, 1, P_D),
    CMD("sin",   oe_sin,   OE_SDT_DOUBLE, 1, P_D),
    CMD("cos",   oe_cos,   OE_SDT_DOUBLE, 1, P_D),
    CMD("tan",   oe_tan,   OE_SDT_DOUBLE, 1, P_D),
    CMD("pow",   oe_pow,   OE_SDT_DOUBLE, 2, P_DD),
    CMD("exp",   oe_exp,   OE_SDT_DOUBLE, 1, P_D),
    CMD("ln",    oe_ln,    OE_SDT_DOUBLE, 1, P_D),
    CMD("log10", oe_log10, OE_SDT_DOUBLE, 1, P_D),
    CMD("floor", oe_floor, OE_SDT_DOUBLE, 1, P_D),
    CMD("ceil",  oe_ceil,  OE_SDT_DOUBLE, 1, P_D),
    CMD("round", oe_round, OE_SDT_DOUBLE, 1, P_D),
    CMD("abs_double", oe_abs_double, OE_SDT_DOUBLE, 1, P_D),
    CMD("min_double", oe_min_double, OE_SDT_DOUBLE, 2, P_DD),
    CMD("max_double", oe_max_double, OE_SDT_DOUBLE, 2, P_DD),
    /* conversions */
    CMD("int_to_double", oe_int_to_double, OE_SDT_DOUBLE, 1, P_I),
    CMD("double_to_int", oe_double_to_int, OE_SDT_INT,    1, P_D),
    CMD("int_to_int64",  oe_int_to_int64,  OE_SDT_INT64,  1, P_I),
    CMD("int64_to_int",  oe_int64_to_int,  OE_SDT_INT,    1, P_I64),
    CMD("int_to_text",   oe_int_to_text,   OE_SDT_TEXT,   1, P_I),
    CMD("int64_to_text", oe_int64_to_text, OE_SDT_TEXT,   1, P_I64),
    CMD("double_to_text",oe_double_to_text,OE_SDT_TEXT,   1, P_D),
    CMD("text_to_int",   oe_text_to_int,   OE_SDT_INT,    1, P_T),
    CMD("text_to_double",oe_text_to_double,OE_SDT_DOUBLE, 1, P_T),
    /* text */
    CMD("text_eq",   oe_text_eq,   OE_SDT_BOOL, 2, P_TT),
    CMD("length",    oe_length,    OE_SDT_INT,  1, P_T),
    CMD("uppercase", oe_uppercase, OE_SDT_TEXT, 1, P_T),
    CMD("lowercase", oe_lowercase, OE_SDT_TEXT, 1, P_T),
    CMD("trim",      oe_trim,      OE_SDT_TEXT, 1, P_T),
    CMD("substr",    oe_substr,    OE_SDT_TEXT, 3, P_TII),
    CMD("find",      oe_find,      OE_SDT_INT,  2, P_TT),
    CMD("replace",   oe_replace,   OE_SDT_TEXT, 3, P_TTT),
    CMD("concat",    oe_concat,    OE_SDT_TEXT, 2, P_TT),
    CMD("repeat",    oe_repeat,    OE_SDT_TEXT, 2, P_TI),
    CMD("reverse",   oe_reverse,   OE_SDT_TEXT, 1, P_T),
    /* datetime */
    CMD("now",         oe_now,         OE_SDT_INT64, 0, NULL),
    CMD("year",        oe_year,        OE_SDT_INT,   1, P_I64),
    CMD("format_time", oe_format_time, OE_SDT_TEXT,  2, P_I64T),
    /* arrays — the operations that cannot be syntax */
    CMD("count",    oe_ary_count,    OE_SDT_INT,       1, P_A),
    CMD("append",   oe_ary_append,   OE_SDT_ANY_ARRAY, 2, P_AE),
    CMD("remove",   oe_ary_remove,   OE_SDT_NULL,      2, P_AI),
    CMD("sort",     oe_ary_sort,     OE_SDT_NULL,      1, P_A),
    CMD("contains", oe_ary_contains, OE_SDT_BOOL,      2, P_AE),
    CMD("index_of", oe_ary_index_of, OE_SDT_INT,       2, P_AE),
    CMD("join",     oe_ary_join,     OE_SDT_TEXT,      2, P_AT),
    CMD("split",    oe_ary_split,    OE_SDT_ARRAY_OF(OE_SDT_TEXT), 2, P_TT),
    /* `slice(xs, start, count)` is what `xs[a..b]` becomes; it is a command in
     * its own right so the shorthand adds no semantics the language did not
     * already have, and so a computed run can be taken without one. */
    CMD("slice",    oe_ary_slice,    OE_SDT_ANY_ARRAY, 3, P_AII),
    /* byte-sets */
    CMD("bytes_new",       oe_bin_make,      OE_SDT_BIN,  1, P_I),
    CMD("bytes_count",     oe_bin_size,      OE_SDT_INT,  1, P_B),
    CMD("bytes_at",        oe_bin_byte,      OE_SDT_INT,  2, P_BI),
    CMD("bytes_set",       oe_bin_put,       OE_SDT_NULL, 3, P_BII),
    CMD("bytes_from_text", oe_bin_from_text, OE_SDT_BIN,  1, P_T),
    CMD("text_from_bytes", oe_bin_to_text,   OE_SDT_TEXT, 1, P_B),
    CMD("bytes_slice",     oe_bin_slice,     OE_SDT_BIN,  3, P_BII),
    /* dictionaries — values found by name.  `dict_get` on a key that is not
     * there answers the sentinel for its value type and sets the error slot;
     * `dict_has` is the predicate that tells that apart from a stored 0. */
    CMD("dict_count",  oe_dict_count,  OE_SDT_INT,      1, P_K),
    CMD("dict_has",    oe_dict_has,    OE_SDT_BOOL,     2, P_KT),
    CMD("dict_get",    oe_dict_lookup, OE_SDT_ANY_ELEM, 2, P_KT),
    CMD("dict_set",    oe_dict_store,  OE_SDT_NULL,     3, P_KTE),
    CMD("dict_remove", oe_dict_erase,  OE_SDT_BOOL,     2, P_KT),
    CMD("dict_keys",   oe_dict_keys,   OE_SDT_ARRAY_OF(OE_SDT_TEXT), 1, P_K),
    /* pointers and raw memory — the escape hatch to C. */
    CMD("ptr_null",        oe_ptr_null,        OE_SDT_PTR,   0, NULL),
    CMD("ptr_is_null",     oe_ptr_is_null,     OE_SDT_BOOL,  1, P_P),
    CMD("ptr_offset",      oe_ptr_offset,      OE_SDT_PTR,   2, P_PI64),
    CMD("ptr_from_int",    oe_ptr_from_int,    OE_SDT_PTR,   1, P_I64),
    CMD("ptr_to_int",      oe_ptr_to_int,      OE_SDT_INT64, 1, P_P),
    CMD("ptr_read_int",    oe_ptr_read_int,    OE_SDT_INT,   2, P_PI64),
    CMD("ptr_write_int",   oe_ptr_write_int,   OE_SDT_NULL,  3, P_PI64I),
    CMD("ptr_read_int64",  oe_ptr_read_int64,  OE_SDT_INT64, 2, P_PI64),
    CMD("ptr_write_int64", oe_ptr_write_int64, OE_SDT_NULL,  3, P_PI64I64),
    CMD("ptr_read_byte",   oe_ptr_read_byte,   OE_SDT_INT,   2, P_PI64),
    CMD("ptr_write_byte",  oe_ptr_write_byte,  OE_SDT_NULL,  3, P_PI64I),
    CMD("ptr_read_double", oe_ptr_read_double, OE_SDT_DOUBLE,2, P_PI64),
    CMD("ptr_write_double",oe_ptr_write_double,OE_SDT_NULL,  3, P_PI64D),
    CMD("ptr_read_ptr",    oe_ptr_read_ptr,    OE_SDT_PTR,   2, P_PI64),
    CMD("ptr_write_ptr",   oe_ptr_write_ptr,   OE_SDT_NULL,  3, P_PI64P),
    CMD("ptr_read_text",   oe_ptr_read_text,   OE_SDT_TEXT,  1, P_P),
    CMD("ptr_write_text",  oe_ptr_write_text,  OE_SDT_NULL,  3, P_PI64T),
    CMD("ptr_of_text",     oe_ptr_of_text,     OE_SDT_PTR,   1, P_T),
    CMD("mem_alloc",       oe_mem_alloc,       OE_SDT_PTR,   1, P_I64),
    CMD("mem_free",        oe_mem_free,        OE_SDT_NULL,  1, P_P),
    CMD("mem_zero",        oe_mem_zero,        OE_SDT_NULL,  2, P_PI64),
    CMD("mem_copy",        oe_mem_copy,        OE_SDT_NULL,  3, P_PPI64),
    /* event loop */
    CMD("quit",            oe_quit,          OE_SDT_NULL, 0, NULL),
};

/* --- timer: the core library's one non-visual component ----------------
 * Properties and events are declared exactly as a button's are — the whole
 * point of the `kind` field is that nothing else about the mechanism changes. */
static const OpenEPL_PropertyDesc TIMER_PROPS[] = {
    { "interval", OE_SDT_INT,  "1000", NULL },
    { "enabled",  OE_SDT_BOOL, "true", NULL },
};
/* The tick count, counting from 1 like every other position in the language.
 * A handler that wants it says so; one that does not is bound unchanged, which
 * is why adding this breaks nothing that already uses a timer. */
static const int32_t TIMER_TICK_PARAMS[] = { OE_SDT_INT };
static const OpenEPL_EventDesc TIMER_EVENTS[] = {
    { "tick", 1, TIMER_TICK_PARAMS },
};

static const OpenEPL_ComponentDesc CORE_COMPONENTS[] = {
    { "timer", OE_ROLE_UNKNOWN,
      (int32_t)(sizeof(TIMER_PROPS) / sizeof(TIMER_PROPS[0])), TIMER_PROPS,
      (int32_t)(sizeof(TIMER_EVENTS) / sizeof(TIMER_EVENTS[0])), TIMER_EVENTS,
      OE_COMPONENT_NONVISUAL },
};

static const OpenEPL_LibInfo CORE_INFO = {
    OPENEPL_ABI_VERSION,
    "core",
    "openepl-core-0000-0000-0000-000000000001",
    0, 2, 0,
    (int32_t)(sizeof(CORE_COMMANDS) / sizeof(CORE_COMMANDS[0])),
    CORE_COMMANDS,
    (int32_t)(sizeof(CORE_COMPONENTS) / sizeof(CORE_COMPONENTS[0])),
    CORE_COMPONENTS,
};

const OpenEPL_LibInfo *openepl_get_lib_info(void) {
    return &CORE_INFO;
}
