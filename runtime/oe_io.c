/* Core commands: console output. */
#include <stdio.h>
#include "openepl_core.h"

void oe_print_int(int value)      { printf("%d\n", value); }
void oe_print_int64(long long v)  { printf("%lld\n", v); }
void oe_print_text(const char *t) { printf("%s\n", t ? t : ""); }

void oe_print_double(double value) {
    /* %g gives a compact, round-trip-friendly rendering for the spike. */
    printf("%g\n", value);
}
