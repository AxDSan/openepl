/* Core command: print_int (SDT_INT). One command per object (PRD D3). */
#include <stdio.h>
#include "openepl_core.h"

void oe_print_int(int value) {
    printf("%d\n", value);
}
