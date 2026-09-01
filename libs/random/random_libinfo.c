/* "random" library metadata (design-time only; compiled into the introspection
 * .so, never a shipped program — same split as core_libinfo.c).
 *
 * Pseudo-random numbers for games and sampling. NOT for anything security
 * sensitive: the sequence is reproducible by design. */
#include "openepl_abi.h"

void random_seed(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv);
void random_seed_now(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv);
void random_int(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv);
void random_between(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv);
void random_double(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv);
void random_double_between(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv);
void random_bool(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv);
void random_chance(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv);
void random_hex(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv);

static const int32_t P_I[]  = { OE_SDT_INT };
static const int32_t P_II[] = { OE_SDT_INT, OE_SDT_INT };
static const int32_t P_DD[] = { OE_SDT_DOUBLE, OE_SDT_DOUBLE };

static const OpenEPL_CommandDesc RANDOM_COMMANDS[] = {
    { "random_seed",           "random_seed",           OE_SDT_NULL,   1, P_I  },
    { "random_seed_now",       "random_seed_now",       OE_SDT_INT,    0, 0    },
    { "random_int",            "random_int",            OE_SDT_INT,    1, P_I  },
    { "random_between",        "random_between",        OE_SDT_INT,    2, P_II },
    { "random_double",         "random_double",         OE_SDT_DOUBLE, 0, 0    },
    { "random_double_between", "random_double_between", OE_SDT_DOUBLE, 2, P_DD },
    { "random_bool",           "random_bool",           OE_SDT_BOOL,   0, 0    },
    { "random_chance",         "random_chance",         OE_SDT_BOOL,   1, P_I  },
    { "random_hex",            "random_hex",            OE_SDT_TEXT,   1, P_I  },
};

static const OpenEPL_LibInfo RANDOM_INFO = {
    OPENEPL_ABI_VERSION,
    "random",
    "openepl-random-0000-0000-0000-000000000011",
    0, 1, 0,
    (int32_t)(sizeof(RANDOM_COMMANDS) / sizeof(RANDOM_COMMANDS[0])),
    RANDOM_COMMANDS,
    0, 0,          /* no visual components */
};

const OpenEPL_LibInfo *openepl_get_lib_info(void) {
    return &RANDOM_INFO;
}
