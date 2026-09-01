/* "units" kit metadata (design-time only; compiled into the introspection .so,
 * never a shipped program — the same split every library uses). This kit lives
 * outside libs/, so it is also the proof that resolution reaches a directory
 * the compiler was not built knowing about. */
#include "openepl_abi.h"

void units_c_to_f(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv);
void units_f_to_c(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv);

static const int32_t P_D[] = { OE_SDT_DOUBLE };

static const OpenEPL_CommandDesc UNITS_COMMANDS[] = {
    { "units_c_to_f", "units_c_to_f", OE_SDT_DOUBLE, 1, P_D },
    { "units_f_to_c", "units_f_to_c", OE_SDT_DOUBLE, 1, P_D },
};

static const OpenEPL_LibInfo UNITS_INFO = {
    OPENEPL_ABI_VERSION,
    "units",
    "openepl-units-0000-0000-0000-000000000010",
    1, 0, 0,
    (int32_t)(sizeof(UNITS_COMMANDS) / sizeof(UNITS_COMMANDS[0])),
    UNITS_COMMANDS,
};

const OpenEPL_LibInfo *openepl_get_lib_info(void) {
    return &UNITS_INFO;
}
