/* "hello" library metadata (design-time only; compiled into the introspection
 * .so, never a shipped program — same split as core_libinfo.c). */
#include "openepl_abi.h"

void hello_greet(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv);

static const int32_t P_T[] = { OE_SDT_TEXT };

static const OpenEPL_CommandDesc HELLO_COMMANDS[] = {
    { "greet", "hello_greet", OE_SDT_TEXT, 1, P_T },
};

static const OpenEPL_LibInfo HELLO_INFO = {
    OPENEPL_ABI_VERSION,
    "hello",
    "openepl-hello-0000-0000-0000-000000000002",
    0, 1, 0,
    (int32_t)(sizeof(HELLO_COMMANDS) / sizeof(HELLO_COMMANDS[0])),
    HELLO_COMMANDS,
};

const OpenEPL_LibInfo *openepl_get_lib_info(void) {
    return &HELLO_INFO;
}
