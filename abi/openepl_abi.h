/* OpenEPL support-library ABI + SDK — v1 (Phase 2).
 *
 * The single documented contract between the OpenEPL runtime/compiler and a
 * support library, a clean-room descendant of EPL's
 * `GetNewInf`/`LIB_INFO`.  A library is a shared object that
 * exports ONE function, `openepl_get_lib_info`, returning a fully-populated
 * `OpenEPL_LibInfo`.  The compiler dlopens the library at build time to read
 * command signatures; the command *implementations* are then static-linked into
 * the program (BlackMoon model — no runtime library dependency.
 *
 * Third parties include THIS header to author a library in C/C++/Rust/Zig (M5).
 *
 * Frozen: the `SDT_*` numeric values, `OpenEPL_Slot` layout, and this struct
 * layout are ABI and MUST NOT change without bumping OPENEPL_ABI_VERSION.
 */
#ifndef OPENEPL_ABI_H
#define OPENEPL_ABI_H

#include <stdint.h>
#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

#define OPENEPL_ABI_VERSION 1

/* --- Data-type tags (SDT_*) — the ABI type system. ---------
 * Numeric values are frozen.  Phase 2 uses INT/INT64/DOUBLE/TEXT; the rest are
 * reserved with fixed numbers so later phases don't renumber. */
enum {
    OE_SDT_NULL      = 0,   /* _SDT_NULL: no type (declarations only)   */
    OE_SDT_BYTE      = 1,
    OE_SDT_SHORT     = 2,
    OE_SDT_INT       = 3,   /* 32-bit int                               */
    OE_SDT_INT64     = 4,   /* 64-bit int                               */
    OE_SDT_FLOAT     = 5,
    OE_SDT_DOUBLE    = 6,   /* 64-bit IEEE-754                          */
    OE_SDT_DATE_TIME = 7,
    OE_SDT_BOOL      = 8,
    OE_SDT_TEXT      = 9,   /* char*, NUL-terminated; NULL = empty      */
    OE_SDT_BIN       = 10,  /* byte-set (Phase 3): ptr {int 1;int len;bytes} */
    OE_SDT_SUB_PTR   = 11,
    OE_SDT_STATMENT  = 12,
    OE_SDT_ALL       = 255  /* _SDT_ALL: any type (declarations only)   */
};

/* --- Slot (MDATA_INF analog) — one argument or return value. ----------
 * A tagged 16-byte cell; the 8-byte value union sits at offset 8.  This is the
 * uniform in/out currency of every command. */
typedef struct OpenEPL_Slot {
    int32_t tag;      /* one of OE_SDT_*                                     */
    int32_t _pad;     /* keep the value 8-byte aligned                      */
    union {
        int32_t  i32;
        int64_t  i64;
        double   d;
        void    *ptr; /* SDT_TEXT: char*; SDT_BIN: byte-set ptr             */
    } v;
} OpenEPL_Slot;

/* Layout is ABI — the backend hard-codes these offsets when marshaling. */
_Static_assert(sizeof(OpenEPL_Slot) == 16, "OpenEPL_Slot must be 16 bytes");
_Static_assert(offsetof(OpenEPL_Slot, v) == 8, "value union must be at offset 8");

/* --- Command implementation signature. --------------------------------
 * cdecl.  `ret` points at the return slot (its tag is set by the callee, or
 * left OE_SDT_NULL for a void command).  `argv` is an array of `argc` slots. */
typedef void (*OpenEPL_CommandFn)(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv);

/* --- Runtime↔library notification channel (NRS_*.2). -----------
 * All EPL-data heap allocation goes THROUGH the runtime so ownership stays
 * consistent across the program and its libraries.  A library asks the runtime
 * to allocate/free via oe_notify(); the runtime may also notify libraries of
 * lifecycle events over the same enum. */
enum {
    OE_NRS_MALLOC      = 1,  /* p1 = size (bytes)      -> returns void*       */
    OE_NRS_MFREE       = 2,  /* p1 = ptr                                      */
    OE_NRS_MREALLOC    = 3,  /* p1 = ptr, p2 = size    -> returns void*       */
    OE_NRS_FREE_ARY    = 4,  /* p1 = array ptr, p2 = SDT_* elem tag (Phase 3) */
    OE_NRS_RUNTIME_ERR = 5   /* p1 = const char* message (aborts)            */
};

/* The runtime's notification entry point (a global runtime symbol; libraries
 * call it directly when static-linked).  Returns a pointer for MALLOC/MREALLOC,
 * NULL otherwise. */
void *oe_notify(int32_t msg, void *p1, void *p2);

/* Convenience wrappers over the channel. */
static inline void *oe_malloc(long size)            { return oe_notify(OE_NRS_MALLOC, (void *)(size_t)size, 0); }
static inline void  oe_mfree(void *p)               { oe_notify(OE_NRS_MFREE, p, 0); }
static inline void *oe_mrealloc(void *p, long size) { return oe_notify(OE_NRS_MREALLOC, p, (void *)(size_t)size); }
static inline void  oe_runtime_error(const char *m) { oe_notify(OE_NRS_RUNTIME_ERR, (void *)m, 0); }

/* --- Library metadata (LibInfo / GetNewInf analog). -------------------
 * Design-time metadata: names, signatures, versions.  It references command
 * symbols by NAME (`symbol`), not by pointer, so it can live in a metadata-only
 * translation unit that is compiled into the introspection `.so` ONLY and never
 * into a shipped program — the compiler resolves symbols at link time.  This is
 * EPL's `.fne` (design-time) vs `.fnr` (runtime) split, and the G8 "no metadata
 * in release output" story. */
/* --- Visual components. -----------------
 * A library contributes visual components through the SAME LibInfo mechanism
 * that carries commands.  A component declares its properties and events by
 * NAME, which is what makes the Object Inspector, form streaming, and the
 * designer generic — one primitive, exactly as Delphi's `published` RTTI does
 *.
 *
 * Accessibility is part of the descriptor, not an afterthought:
 * every component states its a11y role here, and per-instance name/state travel
 * with the properties below. */

/* Accessibility roles (subset of the AccessKit/platform role vocabulary). */
enum {
    OE_ROLE_UNKNOWN = 0,
    OE_ROLE_WINDOW  = 1,
    OE_ROLE_BUTTON  = 2,
    OE_ROLE_LABEL   = 3,
    OE_ROLE_TEXTBOX = 4,
    OE_ROLE_CHECKBOX= 5,
    OE_ROLE_LIST    = 6,
    OE_ROLE_GROUP   = 7
};

typedef struct OpenEPL_PropertyDesc {
    const char *name;          /* surface property name, e.g. "text"        */
    int32_t     tag;           /* OE_SDT_* value type                       */
    const char *default_value; /* textual default; NULL = none              */
} OpenEPL_PropertyDesc;

typedef struct OpenEPL_EventDesc {
    const char *name;          /* surface event name, e.g. "click"          */
} OpenEPL_EventDesc;

typedef struct OpenEPL_ComponentDesc {
    const char                 *name;        /* surface type name, e.g. "button" */
    int32_t                     a11y_role;   /* OE_ROLE_* (D16)                  */
    int32_t                     property_count;
    const OpenEPL_PropertyDesc *properties;
    int32_t                     event_count;
    const OpenEPL_EventDesc    *events;
} OpenEPL_ComponentDesc;

typedef struct OpenEPL_CommandDesc {
    const char    *name;      /* surface command name (English)             */
    const char    *symbol;    /* link symbol of the OpenEPL_CommandFn impl   */
    int32_t        ret_tag;   /* OE_SDT_* return, OE_SDT_NULL for void       */
    int32_t        argc;      /* parameter count                            */
    const int32_t *arg_tags;  /* argc parameter tags                        */
} OpenEPL_CommandDesc;

typedef struct OpenEPL_LibInfo {
    int32_t                     abi_version;   /* must == OPENEPL_ABI_VERSION */
    const char                 *name;
    const char                 *guid;          /* stable per library          */
    int32_t                     ver_major;
    int32_t                     ver_minor;
    int32_t                     ver_build;
    int32_t                     command_count;
    const OpenEPL_CommandDesc  *commands;
    /* Visual components contributed by this library (may be 0 / NULL). */
    int32_t                       component_count;
    const OpenEPL_ComponentDesc  *components;
} OpenEPL_LibInfo;

/* THE single required export (EPL: GetNewInf). */
const OpenEPL_LibInfo *openepl_get_lib_info(void);

/* --- SDK helpers for command authors ---------------------------------- */
static inline int32_t  oe_arg_int(OpenEPL_Slot *argv, int i)    { return argv[i].v.i32; }
static inline int64_t  oe_arg_int64(OpenEPL_Slot *argv, int i)  { return argv[i].v.i64; }
static inline double   oe_arg_double(OpenEPL_Slot *argv, int i) { return argv[i].v.d; }
static inline char    *oe_arg_text(OpenEPL_Slot *argv, int i)   { return (char *)argv[i].v.ptr; }

static inline void oe_ret_int(OpenEPL_Slot *r, int32_t x)   { r->tag = OE_SDT_INT;    r->v.i32 = x; }
static inline void oe_ret_int64(OpenEPL_Slot *r, int64_t x) { r->tag = OE_SDT_INT64;  r->v.i64 = x; }
static inline void oe_ret_double(OpenEPL_Slot *r, double x) { r->tag = OE_SDT_DOUBLE; r->v.d   = x; }
static inline void oe_ret_text(OpenEPL_Slot *r, char *p)    { r->tag = OE_SDT_TEXT;   r->v.ptr = p; }

#ifdef __cplusplus
}
#endif
#endif /* OPENEPL_ABI_H */
