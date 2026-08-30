# OpenEPL e-code IR — specification (v0.1, Phase 1)

> **Status:** frozen *subset*, growing per phase. v0.1 (Phase 1) adds the
> `int64`/`double` types, **command return types + call-expressions**, and the
> validator. Sections marked **(reserved)** name where later phases attach
> without reshaping the schema. The keystone artifact is the IR (PRD G1).
> Command signatures live in `docs/spec/commands.md`.

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
stmt    := let | callstmt
let      := "let" IDENT ":" type "=" expr NEWLINE
callstmt := "call" IDENT "(" args? ")" NEWLINE     # non-void return discarded
type     := "int" | "int64" | "double" | "text"
expr     := term (("+" | "-") term)*
term     := factor (("*" | "/") factor)*
factor   := INT | FLOAT | STRING | callexpr | IDENT | "(" expr ")"
callexpr := IDENT "(" args? ")"                    # value of a non-void command
args     := expr ("," expr)*
```

- **Comments:** `#` to end of line. **Strings:** `"..."` with escapes
  `\n \t \\ \" \0`; raw UTF-8 bytes pass through.
- `main` is the program entry (lowered to `ECodeStart`). v0 requires exactly one
  subroutine named `main`; multi-sub and parameters are **(reserved)**.
- Locals are immutable in v0 (single `let` binding; no reassignment).

## 3. Type system (`SDT_*` tags)

v0.1 exposes the numeric + text core (full set: PRD §1.2, `docs/spec/abi.md`
**(reserved)**):

| IR type  | Tag         | Storage / LLVM                                    |
|----------|-------------|---------------------------------------------------|
| `int`    | `SDT_INT`   | 32-bit signed integer (`i32`)                     |
| `int64`  | `SDT_INT64` | 64-bit signed integer (`i64`)                     |
| `double` | `SDT_DOUBLE`| 64-bit IEEE-754 float (`double`)                  |
| `text`   | `SDT_TEXT`  | pointer to NUL-terminated string (`ptr`); `NULL` = empty |

**(reserved)** `BYTE SHORT FLOAT DATE_TIME BOOL BIN(byte-set) SUB_PTR STATMENT`,
plus the aggregate storage ABI (4-byte member alignment; byte-set
`{1,len,bytes}`; array `{dims,dimSizes[],data}`; the access-length rule) —
specified but not yet modeled. Byte-set is Phase 2 (rides the memory-ownership
notification channel, PRD D4). Datetime is carried as `int64` Unix seconds.

## 4. Commands

A call is the single uniform form (PRD §5.0), in statement position (`call
f(..)`) or expression position (`f(..)`, for non-void commands). Commands
resolve against the shared registry `openepl_ir::registry` (signatures + runtime
symbols); the full list is in `docs/spec/commands.md` (~40 commands across
math / conversions / text / datetime / io).

A command has a `Signature { params: [Ty], ret: Option<Ty> }`; `ret: None` is a
void command (statement-only). Arguments are type-checked positionally.

**(reserved)** In Phase 2 the registry is replaced by signatures loaded from the
support-library ABI (`openepl_get_lib_info`); user subroutines become callable
through the same syntax once control flow lands.

## 5. Semantics fixed in v0.1

- Arithmetic operators require both operands to share one numeric type
  (`int`/`int64`/`double`) — **no implicit conversion** (PRD G9); convert with
  the conversion commands. Integer ops lower to `add/sub/mul/sdiv`, double ops to
  `fadd/fsub/fmul/fdiv`. Precedence: `* /` over `+ -`, left-associative.
- An integer literal is typed `int` if it fits in 32 bits, else `int64`.
- A `let`'s declared type must equal the expression's inferred type.
- A void command used as a value, an unknown command, wrong arity/arg types, an
  undefined variable, a redefinition, or a missing `main` are all validator
  errors (`openepl_ir::validate`, PRD §5.1). The validator reports every error
  in one pass; the backend may then assume well-formed IR.

## 6. In-memory model

See `ir/src/lib.rs`: `Module { name, items: Vec<Item> }`, `Item::Sub(Sub)`
(**reserved** sibling variants), `Sub { name, body: Vec<Stmt> }`,
`Stmt::{Let, Call}`, `Expr::{IntLit, DoubleLit, TextLit, Var, Bin, Call}`,
`Ty::{Int, Int64, Double, Text}`. Type checking: `openepl_ir::sema`; command
table: `openepl_ir::registry`; validation: `openepl_ir::validate`.
