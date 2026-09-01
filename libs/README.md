# Writing a support library

A support library is a directory under `libs/`. `use <name>` in a program finds
it — there is no registration list, no build-system entry, and no `lib.json`
unless the library needs C++, pkg-config, or a vendored dependency.

Two kinds of file, distinguished only by name:

- `<name>_libinfo.c` — the metadata table. Compiled into the introspection
  `.so` the compiler dlopens at build time, and **never** into a shipped
  program: it names every command, so linking it would anchor all of them and
  defeat `--gc-sections`.
- everything else `.c` — the implementations, static-linked into the program.

## The command shape

```c
void mylib_thing(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv);
```

Read arguments with `oe_arg_int` / `oe_arg_int64` / `oe_arg_double` /
`oe_arg_bool` / `oe_arg_text`; write the result with the matching `oe_ret_*`.
Heap results go through `oe_malloc`, never `malloc` — the runtime owns program
data and frees it at exit.

**Only five types exist**: `int`, `int64`, `double`, `bool`, `text`, plus void.
There is no array, no record, and no byte buffer. A collection is exposed as a
count plus an indexed accessor:

```
thing_count(x) -> int        # -1 on failure
thing_at(x, i) -> text       # "" when out of range
```

## Reporting failure

There are no exceptions and no out-parameters. A command that can fail returns
a sentinel and leaves the detail in the error slot:

| Result type | Sentinel on failure |
|---|---|
| handle (int) | `0` |
| count, size, position, timestamp | `-1` |
| text | `""` |
| bool | `false` |

Every exit path of a fallible command calls exactly one of `oe_error_clear()`
(success) or `oe_error_set()` / `oe_error_set_errno()` (failure). Infallible
commands never touch the slot, so an error survives intervening arithmetic.

That last rule is what makes `false` readable: **false with code 0 is a genuine
"no", false with a non-zero code is a failure.** A program checks with
`last_error_code()`.

`errno` must be copied to a local on the line immediately after the failing
call — `fclose` and `free` clobber it — and the slot written last:

```c
FILE *f = fopen(path, "rb");
int e = errno;                    /* nothing may intervene */
if (!f) { oe_error_set_errno(e, "open"); oe_ret_int(ret, 0); return; }
```

Where a sentinel is ambiguous, ship the predicate beside it: `file_at_end`
next to `file_read_line`, because a blank line and end-of-input otherwise look
identical.

## Resources

Anything held across commands (an open file, a connection) is a handle from
`oe_handle_new(kind, payload, close_fn)`, resolved with `oe_handle_resolve`.
The program sees a small positive int, never an address. Kinds are assigned in
`abi/openepl_abi.h`, not here.

Every family that opens something ships a close and a close-all, and passes a
close function to `oe_handle_new` so exit cleanup works even when the program
forgets.

## Naming

There is one flat, global command namespace shared with core's 51 commands and
every other library. Each library owns a **prefix**, and every command it
exports must start with one of them:

| Library | Prefixes |
|---|---|
| `file` | `file_` `dir_` `path_` |
| `system` | `env_` `os_` `sys_` |
| `text` | `text_` |
| `time` | `time_` |
| `random` | `random_` |
| `hash` | `hash_` `base64_` `hex_` |
| `config` | `config_` |
| `process` | `process_` |
| `json` | `json_` |
| `net` | `net_` |
| `math` | `math_` |

Core already owns `text_eq`, `text_to_int` and `text_to_double`; the `text`
library must not redefine them. A collision is a hard build error naming the
later library, so it fails loudly — but only for someone who happens to `use`
both.

Implementation symbols share one link line too, so prefix those as well
(`file_read_text`, not `read_text`).

## Checklist

- [ ] `libs/<name>/<name>_libinfo.c` with a unique `guid` and every command
- [ ] `libs/<name>/<name>_cmds.c` (or several) with the implementations
- [ ] compiles with `clang -I abi -I runtime` alone — the metadata TU is built
      with no pkg-config flags
- [ ] `openepl commands --use <name>` lists what you expect
- [ ] an `examples/<name>lib.oir` that exercises it
