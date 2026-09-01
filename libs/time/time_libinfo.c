/* "time" library metadata (design-time only; compiled into the introspection
 * .so, never a shipped program — the same split as core_libinfo.c).
 *
 * Every command works in UTC and speaks Unix seconds as int64, the same basis
 * core's now() uses, so the two libraries compose. See time_cmds.c. */
#include "openepl_abi.h"

void time_now_ms(OpenEPL_Slot *, int32_t, OpenEPL_Slot *);
void time_monotonic_ms(OpenEPL_Slot *, int32_t, OpenEPL_Slot *);
void time_month(OpenEPL_Slot *, int32_t, OpenEPL_Slot *);
void time_day(OpenEPL_Slot *, int32_t, OpenEPL_Slot *);
void time_hour(OpenEPL_Slot *, int32_t, OpenEPL_Slot *);
void time_minute(OpenEPL_Slot *, int32_t, OpenEPL_Slot *);
void time_second(OpenEPL_Slot *, int32_t, OpenEPL_Slot *);
void time_weekday(OpenEPL_Slot *, int32_t, OpenEPL_Slot *);
void time_day_of_year(OpenEPL_Slot *, int32_t, OpenEPL_Slot *);
void time_from_parts(OpenEPL_Slot *, int32_t, OpenEPL_Slot *);
void time_add_seconds(OpenEPL_Slot *, int32_t, OpenEPL_Slot *);
void time_diff_seconds(OpenEPL_Slot *, int32_t, OpenEPL_Slot *);
void time_format_iso(OpenEPL_Slot *, int32_t, OpenEPL_Slot *);
void time_parse_iso(OpenEPL_Slot *, int32_t, OpenEPL_Slot *);
void time_is_leap_year(OpenEPL_Slot *, int32_t, OpenEPL_Slot *);
void time_days_in_month(OpenEPL_Slot *, int32_t, OpenEPL_Slot *);

static const int32_t P_I[]     = { OE_SDT_INT };
static const int32_t P_II[]    = { OE_SDT_INT, OE_SDT_INT };
static const int32_t P_I64[]   = { OE_SDT_INT64 };
static const int32_t P_I64I64[]= { OE_SDT_INT64, OE_SDT_INT64 };
static const int32_t P_T[]     = { OE_SDT_TEXT };
static const int32_t P_6I[]    = { OE_SDT_INT, OE_SDT_INT, OE_SDT_INT,
                                   OE_SDT_INT, OE_SDT_INT, OE_SDT_INT };

static const OpenEPL_CommandDesc TIME_COMMANDS[] = {
    /* clocks */
    { "time_now_ms",       "time_now_ms",       OE_SDT_INT64, 0, NULL },
    { "time_monotonic_ms", "time_monotonic_ms", OE_SDT_INT64, 0, NULL },
    /* UTC calendar fields of a timestamp */
    { "time_month",        "time_month",        OE_SDT_INT,   1, P_I64 },
    { "time_day",          "time_day",          OE_SDT_INT,   1, P_I64 },
    { "time_hour",         "time_hour",         OE_SDT_INT,   1, P_I64 },
    { "time_minute",       "time_minute",       OE_SDT_INT,   1, P_I64 },
    { "time_second",       "time_second",       OE_SDT_INT,   1, P_I64 },
    { "time_weekday",      "time_weekday",      OE_SDT_INT,   1, P_I64 },
    { "time_day_of_year",  "time_day_of_year",  OE_SDT_INT,   1, P_I64 },
    /* building and moving timestamps */
    { "time_from_parts",   "time_from_parts",   OE_SDT_INT64, 6, P_6I },
    { "time_add_seconds",  "time_add_seconds",  OE_SDT_INT64, 2, P_I64I64 },
    { "time_diff_seconds", "time_diff_seconds", OE_SDT_INT64, 2, P_I64I64 },
    /* ISO 8601, UTC */
    { "time_format_iso",   "time_format_iso",   OE_SDT_TEXT,  1, P_I64 },
    { "time_parse_iso",    "time_parse_iso",    OE_SDT_INT64, 1, P_T },
    /* calendar questions */
    { "time_is_leap_year", "time_is_leap_year", OE_SDT_BOOL,  1, P_I },
    { "time_days_in_month","time_days_in_month",OE_SDT_INT,   2, P_II },
};

static const OpenEPL_LibInfo TIME_INFO = {
    OPENEPL_ABI_VERSION,
    "time",
    "openepl-time-7c1e-4b2a-9f30-5d8e2a41c6b7",
    0, 1, 0,
    (int32_t)(sizeof(TIME_COMMANDS) / sizeof(TIME_COMMANDS[0])),
    TIME_COMMANDS,
};

const OpenEPL_LibInfo *openepl_get_lib_info(void) {
    return &TIME_INFO;
}
