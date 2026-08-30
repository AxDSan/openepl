/* Core commands: date/time (slot ABI), UTC, int64 Unix seconds. */
#include <string.h>
#include <time.h>
#include "openepl_core.h"

void oe_now(OpenEPL_Slot *r, int32_t c, OpenEPL_Slot *argv){ (void)c; (void)argv; oe_ret_int64(r,(long long)time(NULL)); }

void oe_year(OpenEPL_Slot *r, int32_t c, OpenEPL_Slot *argv){
    (void)c; time_t t=(time_t)oe_arg_int64(argv,0); struct tm g; gmtime_r(&t,&g); oe_ret_int(r,g.tm_year+1900);
}
void oe_format_time(OpenEPL_Slot *r, int32_t c, OpenEPL_Slot *argv){
    (void)c; time_t t=(time_t)oe_arg_int64(argv,0); const char*fmt=oe_arg_text(argv,1);
    struct tm g; gmtime_r(&t,&g); char buf[256];
    size_t n=strftime(buf,sizeof buf, fmt?fmt:"%Y-%m-%d %H:%M:%S", &g);
    char*o=(char*)oe_malloc((long)n+1); memcpy(o,buf,n); o[n]='\0'; oe_ret_text(r,o);
}
