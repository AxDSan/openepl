# OpenEPL — project context for Claude

OpenEPL is an open-source, cross-platform, **RAD-first** application development environment in the
VB/Delphi/EPL lineage, with a **BlackMoon-style native compiler** backend. A reimagining of **EPL
(易语言)** + the **BlackMoon (黑月)** compiler — English-first, radically easy, cross-platform.

**Status:** pre-code. The PRD and primary-source research are written; no implementation yet.

## Read first
- `PRD.md` — the full product requirements (the main artifact). Start here.
- `README.md` — orientation + research index.
- `docs/research/*.md` — English digests of the (Chinese) primary sources, each ending in a
  "Takeaways for OpenEPL" section tying it to PRD decisions. Raw Chinese originals in `docs/research/raw/`.

## The non-negotiables (from the PRD)
- **G0 — RAD-first is the identity.** Visual designer + component model (properties/methods/events) +
  event-driven authoring + integrated edit→run→debug→package loop are *core from day one*, not a
  deferred UI track. When a choice helps raw compilation but hurts the design→wire→run→ship loop, the
  loop wins.
- **G9 — radically easy, English-first.** No C/C++ syntax/ceremony, no raw pointers / manual memory / UB
  by default; one uniform call syntax; deep abstraction. Everything EPL did in Chinese is **English**
  here; other languages are an optional localization layer, never the base.
- **G8 — hardened, non-decompilable release output.** Unlike EPL/.NET, release binaries must not
  round-trip to source: IR is build-time only and never embedded; `dev` vs `release` build profiles.
- **G12 — every artifact, every platform.** One project → GUI/console exe, dynamic lib
  (`.dll`/`.so`/`.dylib`), static lib, kernel driver/module (`.sys`/`.ko`/`.kext`), service/plugin/
  bundle — a build-target choice over one IR + component model.
- **Legal hygiene (§9).** Build only from public specs + the BSD-3 BlackMoon `kernel.lib` (as a behavior
  oracle, **reimplemented, never vendored**). No proprietary EPL/BlackMoon binaries or non-open source.

## Architecture in one line
Visual designer/IR (forms+components+events) → LLVM → object files → system linker, dead-code-lean, no
runtime unpacking; portable core runtime (`libopenepl_core`) reimplementing EPL's `krnln`; support-library
ABI descended from `GetNewInf`/`LIB_INFO` (incl. the UI-component interface functions).

## Planned layout (dirs land as implementation begins; ★ = RAD-first core)
`ide/`★ `designer/`★ `components/`★ `ir/` `frontend/` `backend/` `runtime/` `abi/` `libs/` `tools/`
`docs/` `tests/`

## Next step
Phase 0 (PRD §7): freeze IR v0 (incl. component/property/event model) + ABI headers; pick the portable
UI toolkit (open question Q9) and backend language (Rust+inkwell vs C++); compile a hand-written IR
"print + arithmetic" to a native binary on Linux x64.
