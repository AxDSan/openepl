#include "openepl_abi.h"

/* Both conversions are total over the doubles, so neither touches the error
 * slot: an error raised earlier must survive arithmetic that cannot fail. */

void units_c_to_f(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv) {
    (void)argc;
    oe_ret_double(ret, oe_arg_double(argv, 0) * 9.0 / 5.0 + 32.0);
}

void units_f_to_c(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv) {
    (void)argc;
    oe_ret_double(ret, (oe_arg_double(argv, 0) - 32.0) * 5.0 / 9.0);
}
