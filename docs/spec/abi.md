# OpenEPL support-library ABI — specification (v1, Phase 2)

> The single documented contract between the runtime/compiler and a support
> library (PRD G4/§5.4), a clean-room descendant of EPL's `GetNewInf`/`LIB_INFO`
> (PRD §1.2, §11). The normative header is [`abi/openepl_abi.h`](../../abi/openepl_abi.h)
> — a third party includes exactly that to author a library in C/C++/Rust/Zig.

## 1. The one export

A support library is a shared object exporting a single function:

```c
const OpenEPL_LibInfo *openepl_get_lib_info(void);   /* ~ EPL GetNewInf */
```

Everything the compiler needs — name, version, and every command's signature —
is reachable from the returned `OpenEPL_LibInfo`.

## 2. Build-time vs runtime split (the `.fne`/`.fnr` analog)

`LibInfo` is **design-time metadata**. The compiler *introspects* a library at
build time (`dlopen` + `openepl_get_lib_info`) to learn command signatures, then
**static-links the command implementations** into the program (BlackMoon model,
PRD D1). Two consequences, both deliberate:

- The `LibInfo` table (which references every command) is compiled into a
  metadata-only translation unit (`*_libinfo.c`) that goes into the
  introspection `.so` **only** — never a shipped program. If it were linked in,
  it would anchor every command and defeat `--gc-sections` dead-stripping
  (PRD D3). Ship the implementations; never the catalog.
- No command/type name strings or dispatch tables reach release output (PRD G8).

## 3. Slots (`OpenEPL_Slot`) — the value currency

Every argument and return value is a tagged 16-byte cell; the 8-byte value union
sits at offset 8 (enforced by `_Static_assert`). **This layout is frozen ABI** —
the backend hard-codes it when marshaling (`%Slot = { i32, i32, i64 }`).

```c
typedef struct { int32_t tag; int32_t _pad; union { int32_t i32; int64_t i64; double d; void *ptr; } v; } OpenEPL_Slot;
```

## 4. Data-type tags (`SDT_*`) — frozen numeric values

| Tag | Value | Phase 2 |
|---|---|---|
| `OE_SDT_NULL` | 0 | void return / no-type |
| `OE_SDT_BYTE` | 1 | reserved |
| `OE_SDT_SHORT` | 2 | reserved |
| `OE_SDT_INT` | 3 | ✓ `i32` |
| `OE_SDT_INT64` | 4 | ✓ `i64` |
| `OE_SDT_FLOAT` | 5 | reserved |
| `OE_SDT_DOUBLE` | 6 | ✓ `double` |
| `OE_SDT_DATE_TIME` | 7 | reserved (datetime carried as `int64`) |
| `OE_SDT_BOOL` | 8 | reserved |
| `OE_SDT_TEXT` | 9 | ✓ `char*`, NUL-terminated, `NULL` = empty |
| `OE_SDT_BIN` | 10 | reserved — byte-set, Phase 3 |
| `OE_SDT_SUB_PTR` | 11 | reserved |
| `OE_SDT_STATMENT` | 12 | reserved |
| `OE_SDT_ALL` | 255 | any-type (declarations only) |

Values are ABI and must not change without bumping `OPENEPL_ABI_VERSION`.

## 5. Command implementation signature

```c
void cmd(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv);   /* cdecl */
```

The callee reads `argv[i].v.*` and writes the result into `*ret` (setting
`ret->tag`); a void command leaves `ret` untouched. SDK accessors
(`oe_arg_int/…`, `oe_ret_text/…`) hide the slot mechanics.

## 6. Notification channel (`NRS_*`) + memory ownership

All EPL-data heap allocation flows **through the runtime** so ownership is
consistent across the program and its libraries (PRD §1.2/D4). A library calls:

```c
void *oe_notify(int32_t msg, void *p1, void *p2);   /* runtime entry point */
```

with `OE_NRS_MALLOC / MFREE / MREALLOC / FREE_ARY / RUNTIME_ERR`; convenience
wrappers `oe_malloc/oe_mfree/oe_mrealloc/oe_runtime_error` are inline in the
header. The runtime tracks every allocation and frees it in `E_DestroyRes`
(program exit), so text/array results are not leaked. `OE_NRS_FREE_ARY` (byte-set
/ array free) is reserved for Phase 3.

For static-linked libraries (the Phase-2 model) `oe_notify` resolves to the
runtime symbol at link time. The per-library init callback that hands a
dynamically-loaded library the runtime vtable (the `BlackMoonInitAllElib`
analog) is specified in shape but deferred (ADR 0003).

## 7. LibInfo & CommandDesc

```c
typedef struct { const char *name, *symbol; int32_t ret_tag, argc; const int32_t *arg_tags; } OpenEPL_CommandDesc;
typedef struct { int32_t abi_version; const char *name, *guid; int32_t ver_major, ver_minor, ver_build, command_count; const OpenEPL_CommandDesc *commands; } OpenEPL_LibInfo;
```

`abi_version` must equal `OPENEPL_ABI_VERSION` (1); the compiler rejects a
mismatch. Commands reference implementations **by symbol name**, not pointer, so
the table carries no code and stays metadata-only.

## 8. Using a library from IR

`use <name>` at module top (before subroutines) makes the compiler introspect
`libs/<name>/` and link it; `core` is implicit. Duplicate command names across
libraries are a compile error. See `examples/hellolib.oir` and `libs/hello/`.

## 9. Reserved for later phases

Byte-set (`SDT_BIN`) storage + `FREE_ARY`; the UI-component interface-function
contract (OnCreate/OnGet/OnSetProperty/SaveProperties, EDK digest §9) layered on
this same `LibInfo` mechanism (Phase 3, gated on the UI-toolkit decision Q9);
the dynamic-load init/notify vtable; a sidecar manifest for cross-compilation
(so introspection needn't `dlopen` a target-arch library).
