/* "math" library metadata (design-time only; compiled into the introspection
 * .so, never a shipped program — same split as core_libinfo.c). */
#include "openepl_abi.h"

static const int32_t P_D[]   = { OE_SDT_DOUBLE };
static const int32_t P_DD[]  = { OE_SDT_DOUBLE, OE_SDT_DOUBLE };
static const int32_t P_DDD[] = { OE_SDT_DOUBLE, OE_SDT_DOUBLE, OE_SDT_DOUBLE };
static const int32_t P_DI[]  = { OE_SDT_DOUBLE, OE_SDT_INT };
static const int32_t P_I[]   = { OE_SDT_INT };
static const int32_t P_II[]  = { OE_SDT_INT, OE_SDT_INT };
static const int32_t P_III[] = { OE_SDT_INT, OE_SDT_INT, OE_SDT_INT };

#define CMD(name, sym, ret, argc, tags) { name, #sym, ret, argc, tags }

static const OpenEPL_CommandDesc MATH_COMMANDS[] = {
    /* constants */
    CMD("math_pi",       math_pi,       OE_SDT_DOUBLE, 0, NULL),
    CMD("math_tau",      math_tau,      OE_SDT_DOUBLE, 0, NULL),
    CMD("math_e",        math_e,        OE_SDT_DOUBLE, 0, NULL),
    CMD("math_infinity", math_infinity, OE_SDT_DOUBLE, 0, NULL),
    /* inverse trigonometry */
    CMD("math_asin",  math_asin,  OE_SDT_DOUBLE, 1, P_D),
    CMD("math_acos",  math_acos,  OE_SDT_DOUBLE, 1, P_D),
    CMD("math_atan",  math_atan,  OE_SDT_DOUBLE, 1, P_D),
    CMD("math_atan2", math_atan2, OE_SDT_DOUBLE, 2, P_DD),
    /* hyperbolic */
    CMD("math_sinh", math_sinh, OE_SDT_DOUBLE, 1, P_D),
    CMD("math_cosh", math_cosh, OE_SDT_DOUBLE, 1, P_D),
    CMD("math_tanh", math_tanh, OE_SDT_DOUBLE, 1, P_D),
    /* logarithms and roots */
    CMD("math_log2",     math_log2,     OE_SDT_DOUBLE, 1, P_D),
    CMD("math_log_base", math_log_base, OE_SDT_DOUBLE, 2, P_DD),
    CMD("math_cbrt",     math_cbrt,     OE_SDT_DOUBLE, 1, P_D),
    CMD("math_hypot",    math_hypot,    OE_SDT_DOUBLE, 2, P_DD),
    /* rounding and remainder */
    CMD("math_trunc",    math_trunc,    OE_SDT_DOUBLE, 1, P_D),
    CMD("math_fmod",     math_fmod,     OE_SDT_DOUBLE, 2, P_DD),
    CMD("math_round_to", math_round_to, OE_SDT_DOUBLE, 2, P_DI),
    /* sign, range, interpolation */
    CMD("math_sign",      math_sign,      OE_SDT_INT,    1, P_D),
    CMD("math_sign_int",  math_sign_int,  OE_SDT_INT,    1, P_I),
    CMD("math_clamp",     math_clamp,     OE_SDT_DOUBLE, 3, P_DDD),
    CMD("math_clamp_int", math_clamp_int, OE_SDT_INT,    3, P_III),
    CMD("math_lerp",      math_lerp,      OE_SDT_DOUBLE, 3, P_DDD),
    /* angles */
    CMD("math_degrees", math_degrees, OE_SDT_DOUBLE, 1, P_D),
    CMD("math_radians", math_radians, OE_SDT_DOUBLE, 1, P_D),
    /* integers */
    CMD("math_gcd",       math_gcd,       OE_SDT_INT,   2, P_II),
    CMD("math_lcm",       math_lcm,       OE_SDT_INT64, 2, P_II),
    CMD("math_factorial", math_factorial, OE_SDT_INT64, 1, P_I),
    CMD("math_is_prime",  math_is_prime,  OE_SDT_BOOL,  1, P_I),
    /* float predicates */
    CMD("math_is_nan",    math_is_nan,    OE_SDT_BOOL, 1, P_D),
    CMD("math_is_finite", math_is_finite, OE_SDT_BOOL, 1, P_D),
};

static const OpenEPL_LibInfo MATH_INFO = {
    OPENEPL_ABI_VERSION,
    "math",
    "openepl-math-0000-0000-0000-6d6174680001",
    0, 1, 0,
    (int32_t)(sizeof(MATH_COMMANDS) / sizeof(MATH_COMMANDS[0])),
    MATH_COMMANDS,
    0, NULL,
};

const OpenEPL_LibInfo *openepl_get_lib_info(void) { return &MATH_INFO; }
