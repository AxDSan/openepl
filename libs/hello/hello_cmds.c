/* The "hello" kit — a minimal third-party OpenEPL library (.4,
 * M5). It includes ONLY the public SDK header (abi/openepl_abi.h) — not any
 * runtime-internal header — and allocates its result through the notification
 * channel (oe_malloc -> oe_notify), proving the ABI is a real extension point. */
#include <string.h>
#include "openepl_abi.h"

/* greet(text name) -> text : "Hello, <name>!" */
void hello_greet(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv) {
    (void)argc;
    const char *name = oe_arg_text(argv, 0);
    if (!name) name = "";
    const char *pre = "Hello, ", *post = "!";
    long n = (long)(strlen(pre) + strlen(name) + strlen(post) + 1);
    char *out = (char *)oe_malloc(n);
    strcpy(out, pre);
    strcat(out, name);
    strcat(out, post);
    oe_ret_text(ret, out);
}
