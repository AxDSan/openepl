/* Core commands: date/time (UTC). Timestamps are Unix seconds as int64. */
#include <string.h>
#include <time.h>
#include "openepl_core.h"

long long oe_now(void) {
    return (long long)time(NULL);
}

int oe_year(long long unix_seconds) {
    time_t t = (time_t)unix_seconds;
    struct tm g;
    gmtime_r(&t, &g);
    return g.tm_year + 1900;
}

char *oe_format_time(long long unix_seconds, const char *fmt) {
    time_t t = (time_t)unix_seconds;
    struct tm g;
    gmtime_r(&t, &g);
    char buf[256];
    size_t n = strftime(buf, sizeof buf, fmt ? fmt : "%Y-%m-%d %H:%M:%S", &g);
    char *out = (char *)oe_alloc((long)n + 1);
    memcpy(out, buf, n);
    out[n] = '\0';
    return out;
}
