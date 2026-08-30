/* libopenepl_core — Phase 1 spike runtime (PRD G3).
 *
 * A portable C reimplementation of a slice of EPL's core library (`krnln`).
 * Commands are grouped by family into a few translation units; per-command
 * dead-stripping is achieved with `-ffunction-sections` + `--gc-sections`
 * (each function gets its own section), which realizes BlackMoon "fragment
 * extraction" (PRD D3) without one-file-per-command sprawl.  See ADR 0002.
 *
 * MEMORY (PRD D4): the runtime owns EPL-data allocation via oe_alloc/oe_free.
 * Text-returning commands allocate their result with oe_alloc.  The Phase-1
 * spike does NOT yet reclaim these (no GC / notification channel) — results
 * live for the process lifetime.  Ownership tracking arrives with the ABI's
 * NRS_MALLOC/NRS_MFREE channel in Phase 2.
 *
 * Runtime language (C vs Rust) remains open (ADR 0001); nothing here is ABI-C.
 */
#ifndef OPENEPL_CORE_H
#define OPENEPL_CORE_H

/* Program entry emitted by the backend (PRD §1.4 lean-entry model). */
extern int ECodeStart(void);

/* Runtime lifecycle. */
void E_Init(void);
void E_DestroyRes(void);

/* Memory (runtime-owned, PRD D4). */
void *oe_alloc(long size);
void  oe_free(void *p);

/* --- I/O ------------------------------------------------------------- */
void oe_print_int(int value);
void oe_print_int64(long long value);
void oe_print_double(double value);
void oe_print_text(const char *text); /* NULL = empty string */

/* --- Integer math ---------------------------------------------------- */
int oe_abs_int(int a);
int oe_min_int(int a, int b);
int oe_max_int(int a, int b);
int oe_mod_int(int a, int b);
int oe_pow_int(int base, int exp);

/* --- Floating-point math --------------------------------------------- */
double oe_sqrt(double x);
double oe_sin(double x);
double oe_cos(double x);
double oe_tan(double x);
double oe_pow(double base, double exp);
double oe_exp(double x);
double oe_ln(double x);
double oe_log10(double x);
double oe_floor(double x);
double oe_ceil(double x);
double oe_round(double x);
double oe_abs_double(double x);
double oe_min_double(double a, double b);
double oe_max_double(double a, double b);

/* --- Conversions ----------------------------------------------------- */
double     oe_int_to_double(int a);
int        oe_double_to_int(double a);   /* truncates toward zero */
long long  oe_int_to_int64(int a);
int        oe_int64_to_int(long long a);
char      *oe_int_to_text(int a);
char      *oe_int64_to_text(long long a);
char      *oe_double_to_text(double a);
int        oe_text_to_int(const char *s);
double     oe_text_to_double(const char *s);

/* --- Text (results allocated via oe_alloc) --------------------------- */
int   oe_length(const char *s);
char *oe_uppercase(const char *s);
char *oe_lowercase(const char *s);
char *oe_trim(const char *s);
char *oe_substr(const char *s, int start, int count);
int   oe_find(const char *haystack, const char *needle); /* index or -1 */
char *oe_replace(const char *s, const char *from, const char *to);
char *oe_concat(const char *a, const char *b);
char *oe_repeat(const char *s, int times);
char *oe_reverse(const char *s);

/* --- Date / time ----------------------------------------------------- */
long long oe_now(void);                                  /* Unix seconds (UTC) */
int       oe_year(long long unix_seconds);               /* full year, UTC */
char     *oe_format_time(long long unix_seconds, const char *fmt); /* strftime, UTC */

#endif /* OPENEPL_CORE_H */
