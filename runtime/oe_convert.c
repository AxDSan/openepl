/* Core commands: type conversions (slot ABI). Text results via the channel. */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "openepl_core.h"

static char *dup_buf(const char *tmp){
    long n=(long)strlen(tmp)+1; char *out=(char*)oe_malloc(n); memcpy(out,tmp,n); return out;
}

void oe_int_to_double(OpenEPL_Slot *r, int32_t c, OpenEPL_Slot *argv){ (void)c; oe_ret_double(r,(double)oe_arg_int(argv,0)); }
void oe_double_to_int(OpenEPL_Slot *r, int32_t c, OpenEPL_Slot *argv){ (void)c; oe_ret_int(r,(int)oe_arg_double(argv,0)); }
void oe_int_to_int64(OpenEPL_Slot *r, int32_t c, OpenEPL_Slot *argv){ (void)c; oe_ret_int64(r,(long long)oe_arg_int(argv,0)); }
void oe_int64_to_int(OpenEPL_Slot *r, int32_t c, OpenEPL_Slot *argv){ (void)c; oe_ret_int(r,(int)oe_arg_int64(argv,0)); }

void oe_text_to_int(OpenEPL_Slot *r, int32_t c, OpenEPL_Slot *argv){ (void)c; const char*s=oe_arg_text(argv,0); oe_ret_int(r, s?atoi(s):0); }
void oe_text_to_double(OpenEPL_Slot *r, int32_t c, OpenEPL_Slot *argv){ (void)c; const char*s=oe_arg_text(argv,0); oe_ret_double(r, s?atof(s):0.0); }

void oe_int_to_text(OpenEPL_Slot *r, int32_t c, OpenEPL_Slot *argv){ (void)c; char t[32]; snprintf(t,sizeof t,"%d",oe_arg_int(argv,0)); oe_ret_text(r,dup_buf(t)); }
void oe_int64_to_text(OpenEPL_Slot *r, int32_t c, OpenEPL_Slot *argv){ (void)c; char t[32]; snprintf(t,sizeof t,"%lld",(long long)oe_arg_int64(argv,0)); oe_ret_text(r,dup_buf(t)); }
void oe_double_to_text(OpenEPL_Slot *r, int32_t c, OpenEPL_Slot *argv){ (void)c; char t[64]; snprintf(t,sizeof t,"%g",oe_arg_double(argv,0)); oe_ret_text(r,dup_buf(t)); }
