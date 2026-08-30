# OpenEPL core command reference (Phase 1)

The built-in command set the compiler knows how to lower, from
`openepl_ir::registry::Registry::core()`. Every command uses the one uniform
call syntax (PRD §5.0); a non-void command may be used as an expression value,
a void command only as a `call` statement.

**No implicit conversion:** arithmetic operands must match; convert explicitly.

## I/O (void)

| Command | Signature |
|---|---|
| `print_int`    | `(int) -> ()` |
| `print_int64`  | `(int64) -> ()` |
| `print_double` | `(double) -> ()` |
| `print_text`   | `(text) -> ()` |

## Integer math

| Command | Signature |
|---|---|
| `abs_int` | `(int) -> int` |
| `min_int` | `(int, int) -> int` |
| `max_int` | `(int, int) -> int` |
| `mod_int` | `(int, int) -> int` (`b == 0` yields 0) |
| `pow_int` | `(int, int) -> int` (negative exponent yields 0) |

## Floating-point math

`sqrt sin cos tan exp ln log10 floor ceil round abs_double` : `(double) -> double`
· `pow min_double max_double` : `(double, double) -> double`

## Conversions

| Command | Signature |
|---|---|
| `int_to_double` | `(int) -> double` |
| `double_to_int` | `(double) -> int` (truncates toward zero) |
| `int_to_int64`  | `(int) -> int64` |
| `int64_to_int`  | `(int64) -> int` |
| `int_to_text`   | `(int) -> text` |
| `int64_to_text` | `(int64) -> text` |
| `double_to_text`| `(double) -> text` |
| `text_to_int`   | `(text) -> int` |
| `text_to_double`| `(text) -> double` |

## Text

Results are runtime-owned (allocated via `oe_alloc`). `NULL`/absent input is
treated as the empty string (ABI text-slot rule, PRD §1.2). Offsets are byte
offsets (a UTF-8-aware model is a later refinement, PRD Q3).

| Command | Signature |
|---|---|
| `length`    | `(text) -> int` |
| `uppercase` | `(text) -> text` |
| `lowercase` | `(text) -> text` |
| `trim`      | `(text) -> text` |
| `substr`    | `(text, int start, int count) -> text` (clamped) |
| `find`      | `(text haystack, text needle) -> int` (index or −1; empty needle → 0) |
| `replace`   | `(text, text from, text to) -> text` (all occurrences) |
| `concat`    | `(text, text) -> text` |
| `repeat`    | `(text, int times) -> text` |
| `reverse`   | `(text) -> text` (byte-wise) |

## Date / time

Timestamps are `int64` Unix seconds, UTC.

| Command | Signature |
|---|---|
| `now`         | `() -> int64` |
| `year`        | `(int64) -> int` |
| `format_time` | `(int64, text fmt) -> text` (`strftime`; default `%Y-%m-%d %H:%M:%S`) |

## Deferred (Phase 2+)

Byte-set (`SDT_BIN`) commands and the array/struct storage ABI; user-defined
subroutine calls (need control flow); commands loaded from support libraries via
`openepl_get_lib_info` (this hard-coded registry is the stand-in). See ADR 0002.
