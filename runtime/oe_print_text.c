/* Core command: print_text (SDT_TEXT). NULL denotes the empty string,
 * per the ABI's text-slot rule (PRD §1.2). */
#include <stdio.h>
#include "openepl_core.h"

void oe_print_text(const char *text) {
    printf("%s\n", text ? text : "");
}
