# ADR 0010 — Control flow, comparisons and truth values

**Status:** ✅ Accepted · **Date:** 2026-08-30 · **Follows:** ADR 0009

With mutable state but no branching, the language could accumulate but not
decide. This adds `if`/`else if`/`else`, `while`, comparisons, `and`/`or`/`not`,
and a `bool` type.

## `=` means equality in expressions — and the C footgun is impossible

Comparison uses `=` and `<>`, the VB/EPL lineage this project descends from,
rather than `==`/`!=`. That is normally dangerous (`if (x = 5)` silently
assigning), but here it is **structurally impossible**: assignment exists only as
a *statement* (`IDENT =` / `IDENT . IDENT =`), never as an expression. Inside an
expression, `=` can only mean equality. No lookahead, no ambiguity, no footgun —
and one less piece of C ceremony (G9).

## Comparisons are non-associative

`a < b < c` is a **compile error** naming the fix ("write `a < b and b < c`"),
not `(a < b) < c`. Chained comparison is a classic silent-wrong-answer bug: the
C reading compares a truth value against a number and confidently returns
nonsense.

## `and` / `or` / `not` are words, and short-circuit

Words rather than `&& || !` (G9: reads like prose). Both `and` and `or`
**short-circuit**, lowered as branches rather than as eager `select`, so
`x <> 0 and mod_int(100, x) = 0` is safe. Precedence, loosest to tightest:
`or` → `and` → `not` → comparison → `+ -` → `* /`.

## `bool` is int-sized

`Ty::Bool` maps to `OE_SDT_BOOL` (tag 8) and lowers as `i32`, not `i1`. LLVM's
`icmp` yields `i1`, which is widened immediately. The ABI's `BOOL` is int-sized,
so this keeps slot marshaling to one integer width; only `br` needs an `i1`, and
that is an `icmp ne … 0` at the branch.

## Text equality compares content, not pointers

`"fizz" + "buzz" = "fizzbuzz"` must be true, so `=`/`<>` on text route through a
new core command `oe_text_eq` rather than comparing addresses. Beginners compare
strings immediately (G9), and pointer comparison would be wrong in a way that
*usually* looks right for literals — the worst kind of bug. Ordering (`<`) on
text is rejected for now.

## Codegen: the alloca decision from ADR 0009 paid off

Because every local is already alloca-backed, branches need **no phi nodes** —
each arm stores to the same slot. Blocks are emitted flat with a fresh label
counter, reset per function, and every arm ends with an explicit `br` to the
merge block.

**clang is the SSA verifier.** Malformed basic blocks are rejected at build time,
so a passing `control_flow_runs_correctly` proves the emitted IR is structurally
sound — worth more than any assertion I could write about the text.

## Locals are function-scoped, not block-scoped

A `let` inside an `if` is visible after it, matching the alloca-at-top lowering.
Block scoping is a later refinement; recorded because it is a deliberate
simplification, not an oversight.

## Deferred

`for` loops, `break`/`continue`, `return`, block scoping, ordering on text,
bool-typed component properties, and `else`-less `while` refinements.
`break`/`continue` are the most likely next want — a `while` whose only exit is
its condition gets awkward quickly.
