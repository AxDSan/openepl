# ADR 0001 — Phase 0 spike outcomes

**Status:** Accepted · **Date:** 2026-08-30 · **Phase:** 0 (PRD §7)

Records the decisions taken while building the first vertical slice
(`examples/hello.oir` → native binary printing arithmetic results).

## D6 / Q1 — Backend host language & LLVM binding

**Decision:** the compiler is written in **Rust**, and the backend emits
**textual LLVM IR (`.ll`)**, with **`clang` as the assembler + linker driver**.

**Why:** the host toolchain has `rustc`/`clang` but **no `llvm-config`, `llc`,
or `opt`** — so `inkwell` (which links LLVM's dev libraries) is not buildable
today without a heavier setup. Emitting `.ll` text and shelling out to `clang`
keeps the exact BlackMoon model (IR → object → system linker, PRD D1) while
deferring the in-process LLVM binding. The `.ll` string boundary in
`backend::lower_module` is precisely where an `inkwell` implementation drops in
later, behind the same interface — so this is a reversible spike call, not a
lock-in.

## D6 (runtime language) — **still open**

The *core runtime* (`libopenepl_core`) is written in **C** for the spike, purely
to keep the `clang` link step free of std/runtime baggage. Whether the shipping
runtime is C, C++, or Rust remains the open Phase-0 question; nothing in the ABI
sketch assumes C.

## Q9 — Portable UI toolkit — **deferred, not decided**

The print+arithmetic slice needs no UI, and the toolkit choice is a
product-level call (licensing, look-and-feel, binary size) for the owner to
weigh. Left pending; no toolkit research done this session.

## Pipeline as built

```
.oir text ──parse──► IR (openepl-ir) ──lower──► out.ll ──clang──► native ELF
                                                   ▲
                                        runtime/*.c (one command per object)
                                        + --gc-sections dead-strip (PRD D3)
```

- Entry: backend emits `ECodeStart`; `runtime/oe_start.c` provides `main`,
  which runs `E_Init(); ECodeStart(); E_DestroyRes();` (PRD §1.4 lean entry).
- Opaque pointers (`ptr`) are used throughout — LLVM 21 removed typed pointers.
- Target: **`x86_64-linux` only** this phase.

## Explicitly NOT built this session

Designer, component model, support-library ABI headers (beyond the runtime's own
signatures), binary IR encoding, and any second platform. These are the
remaining Phase 0 / Phase 1+ items (PRD §7).
