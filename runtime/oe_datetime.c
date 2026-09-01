/* Core commands: date/time (slot ABI), UTC, int64 Unix seconds. */
#include <string.h>
#include <time.h>
#include "openepl_core.h"

/* Windows has no gmtime_r, and its gmtime_s takes the two arguments the other
 * way round. One shim here so the commands below read the same on both, which
 * is the rule libs/README.md states for every library. */
static void oe_gmtime(time_t t, struct tm *out){
    memset(out, 0, sizeof *out);
#ifdef _WIN32
    gmtime_s(out, &t);
#else
    gmtime_r(&t, out);
#endif
}

void oe_now(OpenEPL_Slot *r, int32_t c, OpenEPL_Slot *argv){ (void)c; (void)argv; oe_ret_int64(r,(long long)time(NULL)); }

void oe_year(OpenEPL_Slot *r, int32_t c, OpenEPL_Slot *argv){
    (void)c; time_t t=(time_t)oe_arg_int64(argv,0); struct tm g; oe_gmtime(t,&g); oe_ret_int(r,g.tm_year+1900);
}
void oe_format_time(OpenEPL_Slot *r, int32_t c, OpenEPL_Slot *argv){
    (void)c; time_t t=(time_t)oe_arg_int64(argv,0); const char*fmt=oe_arg_text(argv,1);
    struct tm g; oe_gmtime(t,&g); char buf[256];
    size_t n=strftime(buf,sizeof buf, fmt?fmt:"%Y-%m-%d %H:%M:%S", &g);
    char*o=(char*)oe_malloc((long)n+1); memcpy(o,buf,n); o[n]='\0'; oe_ret_text(r,o);
}
