/* Core commands: math (slot ABI). */
#include <math.h>
#include "openepl_core.h"

#define A_I(i) oe_arg_int(argv, i)
#define A_D(i) oe_arg_double(argv, i)

void oe_abs_int(OpenEPL_Slot *r, int32_t c, OpenEPL_Slot *argv){ (void)c; int a=A_I(0); oe_ret_int(r, a<0?-a:a); }
void oe_min_int(OpenEPL_Slot *r, int32_t c, OpenEPL_Slot *argv){ (void)c; int a=A_I(0),b=A_I(1); oe_ret_int(r, a<b?a:b); }
void oe_max_int(OpenEPL_Slot *r, int32_t c, OpenEPL_Slot *argv){ (void)c; int a=A_I(0),b=A_I(1); oe_ret_int(r, a>b?a:b); }
void oe_mod_int(OpenEPL_Slot *r, int32_t c, OpenEPL_Slot *argv){ (void)c; int a=A_I(0),b=A_I(1); oe_ret_int(r, b==0?0:a%b); }
void oe_pow_int(OpenEPL_Slot *r, int32_t c, OpenEPL_Slot *argv){
    (void)c; int base=A_I(0), e=A_I(1), out=1;
    if (e<0){ oe_ret_int(r,0); return; }
    for (int i=0;i<e;i++) out*=base;
    oe_ret_int(r,out);
}

void oe_sqrt(OpenEPL_Slot *r, int32_t c, OpenEPL_Slot *argv){ (void)c; oe_ret_double(r, sqrt(A_D(0))); }
void oe_sin(OpenEPL_Slot *r, int32_t c, OpenEPL_Slot *argv){ (void)c; oe_ret_double(r, sin(A_D(0))); }
void oe_cos(OpenEPL_Slot *r, int32_t c, OpenEPL_Slot *argv){ (void)c; oe_ret_double(r, cos(A_D(0))); }
void oe_tan(OpenEPL_Slot *r, int32_t c, OpenEPL_Slot *argv){ (void)c; oe_ret_double(r, tan(A_D(0))); }
void oe_pow(OpenEPL_Slot *r, int32_t c, OpenEPL_Slot *argv){ (void)c; oe_ret_double(r, pow(A_D(0),A_D(1))); }
void oe_exp(OpenEPL_Slot *r, int32_t c, OpenEPL_Slot *argv){ (void)c; oe_ret_double(r, exp(A_D(0))); }
void oe_ln(OpenEPL_Slot *r, int32_t c, OpenEPL_Slot *argv){ (void)c; oe_ret_double(r, log(A_D(0))); }
void oe_log10(OpenEPL_Slot *r, int32_t c, OpenEPL_Slot *argv){ (void)c; oe_ret_double(r, log10(A_D(0))); }
void oe_floor(OpenEPL_Slot *r, int32_t c, OpenEPL_Slot *argv){ (void)c; oe_ret_double(r, floor(A_D(0))); }
void oe_ceil(OpenEPL_Slot *r, int32_t c, OpenEPL_Slot *argv){ (void)c; oe_ret_double(r, ceil(A_D(0))); }
void oe_round(OpenEPL_Slot *r, int32_t c, OpenEPL_Slot *argv){ (void)c; oe_ret_double(r, round(A_D(0))); }
void oe_abs_double(OpenEPL_Slot *r, int32_t c, OpenEPL_Slot *argv){ (void)c; oe_ret_double(r, fabs(A_D(0))); }
void oe_min_double(OpenEPL_Slot *r, int32_t c, OpenEPL_Slot *argv){ (void)c; double a=A_D(0),b=A_D(1); oe_ret_double(r, a<b?a:b); }
void oe_max_double(OpenEPL_Slot *r, int32_t c, OpenEPL_Slot *argv){ (void)c; double a=A_D(0),b=A_D(1); oe_ret_double(r, a>b?a:b); }
