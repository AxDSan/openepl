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

/* --- Record and dictionary layouts ------------------------------------
 * Their tags are in abi/openepl_abi.h with every other SDT_* value; what
 * follows is the memory each one actually has.
 *
 * A record: a fixed number of slot-width fields, one runtime-owned allocation.
 *
 *     { int32 count; int32 _pad; int64 fields[count]; }
 *
 * The shape an array has, minus the element tag — a record's fields do not
 * share one type, and each field's type is a compile-time fact, so nothing at
 * run time has to ask.  Fields are reached by POSITION, counting from 1, which
 * is also why no field name reaches a shipped binary. */
typedef struct OpenEPL_Record {
    int32_t count;
    int32_t _pad;
} OpenEPL_Record;

/* One key and its value.  The value is the same 64 raw bits a slot carries, so
 * a dictionary of text holds pointers exactly as one of int holds ints. */
typedef struct OpenEPL_DictEntry {
    char   *key;
    int64_t val;
} OpenEPL_DictEntry;

/* A dictionary: a header the program holds, and an entry block that grows.
 *
 * TWO allocations, unlike an array's one, and that is the whole design: a
 * dictionary grows in place — `d["new"] = 1` must be visible through every
 * name that holds it — so the thing that MOVES when it grows must not be the
 * thing the program is holding.  The header address never changes; the entry
 * block behind it is what reallocates. */
typedef struct OpenEPL_Dict {
    int32_t            val_tag;  /* OE_SDT_* of one value                    */
    int32_t            len;      /* entries in use                           */
    int32_t            cap;      /* entries allocated; always >= len         */
    int32_t            _pad;
    OpenEPL_DictEntry *entries;
} OpenEPL_Dict;

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
OE_CMD(oe_assert_failed);
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
OE_CMD(oe_ary_slice);
/* byte-sets */
OE_CMD(oe_bin_make); OE_CMD(oe_bin_size); OE_CMD(oe_bin_byte); OE_CMD(oe_bin_put);
OE_CMD(oe_bin_from_text); OE_CMD(oe_bin_to_text); OE_CMD(oe_bin_slice);
/* dictionaries */
OE_CMD(oe_dict_count); OE_CMD(oe_dict_has); OE_CMD(oe_dict_lookup);
OE_CMD(oe_dict_store); OE_CMD(oe_dict_erase); OE_CMD(oe_dict_keys);
/* pointers and raw memory (oe_ptr.c) */
OE_CMD(oe_ptr_null); OE_CMD(oe_ptr_is_null); OE_CMD(oe_ptr_offset);
OE_CMD(oe_ptr_from_int); OE_CMD(oe_ptr_to_int);
OE_CMD(oe_ptr_read_int); OE_CMD(oe_ptr_write_int);
OE_CMD(oe_ptr_read_int64); OE_CMD(oe_ptr_write_int64);
OE_CMD(oe_ptr_read_byte); OE_CMD(oe_ptr_write_byte);
OE_CMD(oe_ptr_read_double); OE_CMD(oe_ptr_write_double);
OE_CMD(oe_ptr_read_ptr); OE_CMD(oe_ptr_write_ptr);
OE_CMD(oe_ptr_read_text); OE_CMD(oe_ptr_write_text); OE_CMD(oe_ptr_of_text);
OE_CMD(oe_mem_alloc); OE_CMD(oe_mem_free); OE_CMD(oe_mem_zero); OE_CMD(oe_mem_copy);
/* event loop */
OE_CMD(oe_quit);

/* Aggregate access, NOT commands: indexing is syntax, so the backend calls
 * these directly rather than marshaling an argv array to read one element.
 * They move raw 64-bit values — what a slot's value field already holds. */
void   *oe_ary_new(int32_t tag, int32_t len);
int64_t oe_ary_get(void *a, int32_t i);
void    oe_ary_set(void *a, int32_t i, int64_t v);
void   *oe_bin_new(int32_t len);
int32_t oe_bin_at(void *b, int32_t i);
void    oe_bin_set(void *b, int32_t i, int32_t v);
/* A field is named in the source and reached by position here, so these take
 * the index the compiler worked out rather than the name it read. */
void   *oe_rec_new(int32_t field_count);
int64_t oe_rec_get(void *r, int32_t i);
void    oe_rec_set(void *r, int32_t i, int64_t v);
void   *oe_dict_new(int32_t val_tag);
int64_t oe_dict_at(void *d, const char *key);
void    oe_dict_put(void *d, const char *key, int64_t v);

/* Core's non-visual components (abi/openepl_abi.h).  NOT commands: the backend
 * calls these directly, exactly as it calls the oe_ui_* entry points for a
 * visual one. */
int64_t     oe_core_component_create(const char *type_name);
int32_t     oe_core_component_set(int64_t h, const char *prop, const char *value);
const char *oe_core_component_get(int64_t h, const char *prop);
int32_t     oe_core_component_get_int(int64_t h, const char *prop);
int32_t     oe_core_component_on(int64_t h, const char *event, OpenEPL_HandlerFn handler);

/* Shared internals, not commands.  The error slot and handle table are declared
 * in the ABI header so libraries reach them the same way the core does; these
 * are the few pieces that stay runtime-private. */
char *oe_empty_text(void);              /* from oe_error.c  */
int32_t oe_handle_kind_of(int32_t h);   /* from oe_handle.c */
/* The foreign-function loader (oe_dll.c).  These are called only from emitted
 * IR — a `dll` call — never by another command, but they are declared here so
 * the runtime has one header. `oe_dll_get` resolves `sym` in `library`, caching
 * the address in `*cache` so it is looked up once; `oe_dll_text` copies a C
 * string a foreign call returned into a runtime-owned text. */
void *oe_dll_get(void **cache, const char *library, const char *sym);
char *oe_dll_text(const char *p);
void oe_set_args(int argc, char **argv);/* from oe_args.c   */
int   oe_arg_total(void);
const char *oe_arg_at(int i);

#endif /* OPENEPL_CORE_H */
