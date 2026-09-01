/* libopenepl_core — internal runtime header (Phase 2).
 *
 * The core support library, now speaking the real slot ABI (abi/openepl_abi.h):
 * every command is an `OpenEPL_CommandFn` — `void cmd(Slot* ret, int argc,
 * Slot* argv)`.  Command implementations live in the family `.c` files and are
 * static-linked into each program; the LibInfo *table* that names them lives in
 * `core_libinfo.c`, compiled ONLY into the introspection `.so` (never a shipped
 * program) so `--gc-sections` still strips unused commands.
 */
#ifndef OPENEPL_CORE_H
#define OPENEPL_CORE_H

#include "openepl_abi.h"

/* Program entry emitted by the backend. */
extern int ECodeStart(void);

/* Runtime lifecycle. */
void E_Init(void);
void E_DestroyRes(void);

/* Notification channel + allocation.  `oe_notify` is declared
 * in the ABI header; these are the concrete runtime entry points behind it. */
void *E_MAlloc(long size);
void  E_MFree(void *p);
void *E_MRealloc(void *p, long size);

/* Every core command (OpenEPL_CommandFn).  Referenced by core_libinfo.c. */
#define OE_CMD(n) void n(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv)

/* I/O */
OE_CMD(oe_print_int); OE_CMD(oe_print_int64); OE_CMD(oe_print_double); OE_CMD(oe_print_text);
OE_CMD(oe_read_line); OE_CMD(oe_input_ended); OE_CMD(oe_ask);
/* errors */
OE_CMD(oe_last_error_code); OE_CMD(oe_last_error_text);
/* integer math */
OE_CMD(oe_abs_int); OE_CMD(oe_min_int); OE_CMD(oe_max_int); OE_CMD(oe_mod_int); OE_CMD(oe_pow_int);
/* float math */
OE_CMD(oe_sqrt); OE_CMD(oe_sin); OE_CMD(oe_cos); OE_CMD(oe_tan); OE_CMD(oe_pow);
OE_CMD(oe_exp); OE_CMD(oe_ln); OE_CMD(oe_log10); OE_CMD(oe_floor); OE_CMD(oe_ceil);
OE_CMD(oe_round); OE_CMD(oe_abs_double); OE_CMD(oe_min_double); OE_CMD(oe_max_double);
/* conversions */
OE_CMD(oe_int_to_double); OE_CMD(oe_double_to_int); OE_CMD(oe_int_to_int64); OE_CMD(oe_int64_to_int);
OE_CMD(oe_int_to_text); OE_CMD(oe_int64_to_text); OE_CMD(oe_double_to_text);
OE_CMD(oe_text_to_int); OE_CMD(oe_text_to_double);
/* text */
OE_CMD(oe_text_eq); OE_CMD(oe_length); OE_CMD(oe_uppercase); OE_CMD(oe_lowercase); OE_CMD(oe_trim); OE_CMD(oe_substr);
OE_CMD(oe_find); OE_CMD(oe_replace); OE_CMD(oe_concat); OE_CMD(oe_repeat); OE_CMD(oe_reverse);
/* datetime */
OE_CMD(oe_now); OE_CMD(oe_year); OE_CMD(oe_format_time);
/* arrays */
OE_CMD(oe_ary_count); OE_CMD(oe_ary_append); OE_CMD(oe_ary_remove); OE_CMD(oe_ary_sort);
OE_CMD(oe_ary_contains); OE_CMD(oe_ary_index_of); OE_CMD(oe_ary_join); OE_CMD(oe_ary_split);
/* byte-sets */
OE_CMD(oe_bin_make); OE_CMD(oe_bin_size); OE_CMD(oe_bin_byte); OE_CMD(oe_bin_put);
OE_CMD(oe_bin_from_text); OE_CMD(oe_bin_to_text);

/* Aggregate access, NOT commands: indexing is syntax, so the backend calls
 * these directly rather than marshaling an argv array to read one element.
 * They move raw 64-bit values — what a slot's value field already holds. */
void   *oe_ary_new(int32_t tag, int32_t len);
int64_t oe_ary_get(void *a, int32_t i);
void    oe_ary_set(void *a, int32_t i, int64_t v);
void   *oe_bin_new(int32_t len);
int32_t oe_bin_at(void *b, int32_t i);
void    oe_bin_set(void *b, int32_t i, int32_t v);

/* Shared internals, not commands.  The error slot and handle table are declared
 * in the ABI header so libraries reach them the same way the core does; these
 * are the few pieces that stay runtime-private. */
char *oe_empty_text(void);              /* from oe_error.c  */
int32_t oe_handle_kind_of(int32_t h);   /* from oe_handle.c */
void oe_set_args(int argc, char **argv);/* from oe_args.c   */
int   oe_arg_total(void);
const char *oe_arg_at(int i);

#endif /* OPENEPL_CORE_H */
