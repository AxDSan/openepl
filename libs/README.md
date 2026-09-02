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

**The types are** `int`, `int64`, `double`, `bool`, `text`, arrays (`T[]`) and
`bytes`, plus void. There is no record and no dictionary. Read an argument of
an aggregate type straight out of the slot's pointer — `argv[i].v.ptr`, cast to
`OpenEPL_Array *` or `OpenEPL_Bin *` — and return one by writing that pointer
back with the matching tag; the layouts are stated in `abi/openepl_abi.h`, and
both are allocated by the runtime (`oe_ary_new` / `oe_bin_new`).

Where a collection is a *view* of something the library owns rather than a
value it hands over, it is still exposed as a count plus an indexed accessor,
because a snapshot the program can hold is not what a live directory is:

```
thing_count(x) -> int        # -1 on failure
thing_at(x, i) -> text       # "" when out of range
```

**Positions count from 1.** `thing_at(x, 1)` is the first element and
`thing_at(x, thing_count(x))` is the last, so a loop runs `1` to the count with
no `- 1` anywhere. This holds everywhere without exception: arrays, bytes, text
positions, every indexed accessor, and JSON paths — which means `items[1]` here
is what JSONPath would call `items[0]`.

It also means **0 is free to mean "not found"**, which is why `index_of` and
`find` return 0 for absent rather than the -1 a 0-based language needs. A
position and a failure can never be confused.

EPL counts from 1 and so does the BASIC lineage this sits in; a 0-based array
in a language aimed at people who are not systems programmers is an inherited
C accident, not a design.

## Reporting failure

There are no exceptions and no out-parameters. A command that can fail returns
a sentinel and leaves the detail in the error slot:

| Result type | Sentinel on failure |
|---|---|
| handle (int) | `0` |
| count, size, position, timestamp | `-1` |
| text | `""` |
| bytes | an EMPTY byte-set — never a null pointer |
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
| `ui` | `grid_` `datasource_` |

Core already owns `text_eq`, `text_to_int` and `text_to_double`; the `text`
library must not redefine them. A collision is a hard build error naming the
later library, so it fails loudly — but only for someone who happens to `use`
both.

Implementation symbols share one link line too, so prefix those as well
(`file_read_text`, not `read_text`).

## Portability

Everything under `libs/` must compile for Windows as well as for POSIX. The
line is `#ifdef _WIN32`, and the rule is that the branch is a *thin shim*: one
place in the file knows the platform, and the command bodies read the same on
both. `libs/file` shows the shape — a block of wrappers (`file_fopen`,
`file_stat`, `file_unlink`) with two implementations, and no `#ifdef` in any
command below it.

Three things about Windows that are easy to get wrong, and that every library
here already gets right:

- **A path is UTF-16.** OpenEPL text is UTF-8, so the *wide* entry points are
  used and the results converted. The ANSI ones go through the machine's
  codepage and mangle any name outside it, which for a path is a common case,
  not an exotic one.
- **A Win32 or Winsock status is not an errno value.** It reaches the error
  slot through `oe_error_set`, never `oe_error_set_errno` — running it through
  `strerror` produces a confident wrong sentence, and comparing it against
  `ECONNREFUSED` compares two unrelated numbering schemes.
- **Both separators are separators**, and a root may be `C:\` or
  `\\server\share`. `libs/file` asks `file_is_sep` and `file_root_len`
  rather than comparing against `'/'`, so `..` cannot rewind past a drive.

Verify a change with the mingw-w64 cross compiler, from Linux, in one command:

```sh
x86_64-w64-mingw32-gcc -fsyntax-only -Wall -Wextra -I abi -I runtime libs/<name>/<name>_cmds.c
```

And then actually RUN it — link a small driver that stands in for the runtime
against the library's `_cmds.c`, cross-compile it, and put it under Wine. A
branch that only ever compiled is a branch nobody has read carefully enough,
and the difference costs about twenty minutes. `net` needs `-lws2_32` on the
link line, which `lib.json` has no platform-conditional key to express; MSVC
picks it up from a `#pragma comment(lib, ...)` in the source, mingw does not.

If a library genuinely cannot be ported, leave the Windows branch as an
`#error` saying so. Code that looks portable and is not costs more than an
honest gap.

## Checklist

- [ ] `libs/<name>/<name>_libinfo.c` with a unique `guid` and every command
- [ ] `libs/<name>/<name>_cmds.c` (or several) with the implementations
- [ ] compiles with `clang -I abi -I runtime` alone — the metadata TU is built
      with no pkg-config flags
- [ ] `openepl commands --use <name>` lists what you expect
- [ ] an `examples/<name>lib.oir` that exercises it
- [ ] it cross-compiles for Windows (see **Portability** above)
