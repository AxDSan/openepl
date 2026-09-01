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
};

static const OpenEPL_LibInfo CORE_INFO = {
    OPENEPL_ABI_VERSION,
    "core",
    "openepl-core-0000-0000-0000-000000000001",
    0, 2, 0,
    (int32_t)(sizeof(CORE_COMMANDS) / sizeof(CORE_COMMANDS[0])),
    CORE_COMMANDS,
    0, NULL,   /* the core library contributes no visual components */
};

const OpenEPL_LibInfo *openepl_get_lib_info(void) {
    return &CORE_INFO;
}
