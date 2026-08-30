/* Core commands: type conversions. Text results are owned by the runtime. */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "openepl_core.h"

double    oe_int_to_double(int a)   { return (double)a; }
int       oe_double_to_int(double a){ return (int)a; }        /* trunc toward 0 */
long long oe_int_to_int64(int a)    { return (long long)a; }
int       oe_int64_to_int(long long a) { return (int)a; }

int    oe_text_to_int(const char *s)    { return s ? atoi(s) : 0; }
double oe_text_to_double(const char *s) { return s ? atof(s) : 0.0; }

static char *dup_buf(const char *tmp) {
    long n = (long)strlen(tmp) + 1;
    char *out = (char *)oe_alloc(n);
    memcpy(out, tmp, n);
    return out;
}

char *oe_int_to_text(int a) {
    char tmp[32];
    snprintf(tmp, sizeof tmp, "%d", a);
    return dup_buf(tmp);
}
char *oe_int64_to_text(long long a) {
    char tmp[32];
    snprintf(tmp, sizeof tmp, "%lld", a);
    return dup_buf(tmp);
}
char *oe_double_to_text(double a) {
    char tmp[64];
    snprintf(tmp, sizeof tmp, "%g", a);
    return dup_buf(tmp);
}
