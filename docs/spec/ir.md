# OpenEPL e-code IR — specification (v0, Phase 0)

> **Status:** frozen *subset*. v0 covers only what the "print + arithmetic"
> vertical slice needs. Sections marked **(reserved)** name where later phases
> attach without reshaping the schema. The keystone artifact is the IR (PRD G1).

## 1. Encodings

- **Text (`.oir`)** — implemented in v0. Human-writable, diff-able; the form used
  for tests and PRs.
- **Binary** — **(reserved / deferred.)** The compact, canonical, hashable
  shipping form (PRD §5.1). Not implemented in v0. The text and binary encodings
  must round-trip losslessly once binary lands.

The IR is **build-time only** and is **never embedded** in output binaries
(PRD G8/D7), in either encoding.

## 2. Text grammar (v0)

```text
module  := "module" IDENT NEWLINE item*
item    := sub                       # (reserved: form | component | const | enum | usertype)
sub     := "sub" IDENT NEWLINE stmt* "end" NEWLINE
stmt    := let | call
let     := "let" IDENT ":" type "=" expr NEWLINE
call    := "call" IDENT "(" (expr ("," expr)*)? ")" NEWLINE
type    := "int" | "text"
expr    := term (("+" | "-") term)*
term    := factor (("*" | "/") factor)*
factor  := INT | STRING | IDENT | "(" expr ")"
```

- **Comments:** `#` to end of line. **Strings:** `"..."` with escapes
  `\n \t \\ \" \0`; raw UTF-8 bytes pass through.
- `main` is the program entry (lowered to `ECodeStart`). v0 requires exactly one
  subroutine named `main`; multi-sub and parameters are **(reserved)**.
- Locals are immutable in v0 (single `let` binding; no reassignment).

## 3. Type system (`SDT_*` tags)

v0 exposes two of the ABI slot types (full set: PRD §1.2, `docs/spec/abi.md`
**(reserved)**):

| IR type | Tag       | Storage                                             |
|---------|-----------|-----------------------------------------------------|
| `int`   | `SDT_INT` | 32-bit signed integer                               |
| `text`  | `SDT_TEXT`| pointer to NUL-terminated string; `NULL` = empty    |

**(reserved)** `BYTE SHORT INT64 FLOAT DOUBLE DATE_TIME BOOL BIN(byte-set)
SUB_PTR STATMENT`, plus the aggregate storage ABI (4-byte member alignment;
byte-set `{1,len,bytes}`; array `{dims,dimSizes[],data}`; the access-length
rule) — specified but not yet modeled.

## 4. Commands

A `call CMD(args...)` is the single uniform call form (PRD §5.0). v0 resolves
commands against a hard-coded table in the backend:

| Command       | Signature            | Runtime symbol   |
|---------------|----------------------|------------------|
| `print_text`  | `(text) -> ()`       | `oe_print_text`  |
| `print_int`   | `(int) -> ()`        | `oe_print_int`   |

**(reserved)** In Phase 2 this table is replaced by signatures loaded from the
support-library ABI (`openepl_get_lib_info`), and user subroutines become
callable through the same syntax.

## 5. Semantics fixed in v0

- Integer arithmetic is 32-bit; `+ - *` map to LLVM `add/sub/mul`, `/` to
  `sdiv`. Precedence: `* /` bind tighter than `+ -`; left-associative.
- Integer-literal overflow of 32 bits is a compile error.
- `let` type must match the expression's inferred type, else a compile error.

## 6. In-memory model

See `ir/src/lib.rs`: `Module { name, items: Vec<Item> }`, `Item::Sub(Sub)`
(**reserved** sibling variants), `Sub { name, body: Vec<Stmt> }`,
`Stmt::{Let, Call}`, `Expr::{IntLit, TextLit, Var, Bin}`, `Ty::{Int, Text}`.
