/* Core commands: console output (slot ABI). */
#include <stdio.h>
#include "openepl_core.h"

void oe_print_int(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv) {
    (void)argc; (void)ret;
    printf("%d\n", oe_arg_int(argv, 0));
}
void oe_print_int64(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv) {
    (void)argc; (void)ret;
    printf("%lld\n", (long long)oe_arg_int64(argv, 0));
}
void oe_print_text(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv) {
    (void)argc; (void)ret;
    const char *t = oe_arg_text(argv, 0);
    printf("%s\n", t ? t : "");
}
void oe_print_double(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv) {
    (void)argc; (void)ret;
    printf("%g\n", oe_arg_double(argv, 0));
}
