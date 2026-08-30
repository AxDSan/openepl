# ADR 0009 — Mutable variables and module state

**Status:** ✅ Accepted · **Date:** 2026-08-30 · **Follows:** ADR 0008

Until now the language had no mutable storage at all, so `examples/counter.oir`
kept its count *in a label's text* — reading the UI to find your state. That was
expedient, not good, and it was going to distort every example built on it.

## `var` mutable, `let` immutable

Both keywords, at module level and inside subroutines. Immutability is the
default; reassigning a `let` is a compile error that names the fix:

```
`x` is immutable — declare it with `var` instead of `let` to allow assignment
```

This is G9's footgun-free rule applied to the smallest case: the error tells the
user what to type, rather than what they did wrong.

## Every local is alloca-backed — including `let`

One lowering path, not two. Locals were previously SSA values held in a map,
which cannot represent reassignment; rather than keep an SSA/alloca hybrid, all
locals became `alloca` + `load`/`store`. At `-O0` this costs nothing that
matters, and when optimisation is turned on `mem2reg` reconstructs SSA for free.

## Module variables are `internal` LLVM globals

`var count: int = 0` emits `@oe_g_count = internal global i32 0`. `internal`
linkage means it is not exported, and `strip` removes the name in release (G8).

**Initialization rule:** the global is zero-initialised statically, and its
declared initializer runs at program entry, *before* the form is built and
before `main`. That ordering is what allows an initializer to call a command
(`var started: int64 = now()`), which a static initializer could not express.

**A module variable's initializer may not read another module variable** —
order-dependent global initialisation is a swamp, and the validator says so
plainly instead of letting it half-work.

## One namespace for module variables, component ids and subroutines

A `var count` and a component `count` would make `count = 5` and
`count.text = "x"` refer to different things under one name. Collisions are a
compile error now, while that is still a cheap rule to add rather than a
breaking change.

## Text variables are never freed on reassignment

A `text` variable holds a runtime-owned pointer. Assigning a new value stores a
new pointer and leaves the old allocation to be freed at exit, consistent with
the existing leak-until-exit story (D4). **Freeing on reassignment would be
wrong**, for two concrete reasons:

1. `oe_ui_get` and the text commands hand out pointers the program may still
   hold; freeing on reassign would dangle them.
2. `var s: text = "literal"` points at a static `.str` constant, not
   runtime-owned memory — freeing it would crash.

Proper reclamation needs ownership tracking or refcounting; that is a later
decision, and this note exists so the "obvious optimisation" is not attempted
without it.

## Deferred

Compound assignment (`+=`), shadowing, mutable component-typed variables,
arrays, and freeing text on reassignment (above). Control flow — `if`/`while` —
is now the most conspicuous gap: with mutable state but no branching, the
language can accumulate but not decide.
