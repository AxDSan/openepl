/* Core commands: math (integer + floating point). */
#include <math.h>
#include "openepl_core.h"

int oe_abs_int(int a)            { return a < 0 ? -a : a; }
int oe_min_int(int a, int b)     { return a < b ? a : b; }
int oe_max_int(int a, int b)     { return a > b ? a : b; }
int oe_mod_int(int a, int b)     { return b == 0 ? 0 : a % b; }

int oe_pow_int(int base, int exp) {
    if (exp < 0) return 0;
    int r = 1;
    for (int i = 0; i < exp; i++) r *= base;
    return r;
}

double oe_sqrt(double x)  { return sqrt(x); }
double oe_sin(double x)   { return sin(x); }
double oe_cos(double x)   { return cos(x); }
double oe_tan(double x)   { return tan(x); }
double oe_pow(double b, double e) { return pow(b, e); }
double oe_exp(double x)   { return exp(x); }
double oe_ln(double x)    { return log(x); }
double oe_log10(double x) { return log10(x); }
double oe_floor(double x) { return floor(x); }
double oe_ceil(double x)  { return ceil(x); }
double oe_round(double x) { return round(x); }
double oe_abs_double(double x)        { return fabs(x); }
double oe_min_double(double a, double b) { return a < b ? a : b; }
double oe_max_double(double a, double b) { return a > b ? a : b; }
