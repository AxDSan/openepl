# OpenEPL — project context for Claude

OpenEPL is an open-source, cross-platform, **RAD-first** application development environment in the
VB/Delphi/EPL lineage, with a **BlackMoon-style native compiler** backend. A reimagining of **EPL
(易语言)** + the **BlackMoon (黑月)** compiler — English-first, radically easy, cross-platform.

**Status:** Phases 0–2 (ABI half) implemented and committed; UI substrate decided.
Working toolchain: `.oir` IR → LLVM IR → native binary, with an IR validator, ~45 core commands,
proven dead-stripping, and the support-library ABI (slot calling convention + `NRS_*` notification
channel + build-time `LibInfo` introspection). Next up is the RAD/component half of Phase 2 →
Phase 3. See `docs/decisions/` for the decision log and `docs/spec/` for the frozen specs.

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

## Decisions already made (don't re-litigate — see `docs/decisions/`)
- **Compiler:** Rust, emitting textual LLVM IR; `clang` as assembler/linker driver (ADR 0001).
- **Runtime:** C core (`libopenepl_core`), slot ABI `void cmd(Slot*, i32, Slot*)`, runtime-owned
  memory via the `NRS_*` channel; `LibInfo` metadata lives in a `.so`-only TU so `--gc-sections`
  still strips unused commands (ADR 0003).
- **UI substrate:** **RmlUi** (MIT), behind a swappable backend interface (ADR 0004/0005,
  spike-verified in `spikes/q9-rmlui/`). Visual target is **FireMonkey-class** — custom-drawn,
  styled, animated; native-widget fidelity is explicitly not a goal.
- **Licence policy:** accept MIT/BSD/Apache-2.0/Zlib/ISC; reject GPL/LGPL without a static-link
  exception, non-OSI grants, and proprietary (ADR 0005/D15).
- **Languages:** C core · C++ UI layer · Rust compiler, joined by the C ABI (ADR 0005/D19).
- **Strings:** UTF-8 everywhere (ADR 0005/D20).
- **Accessibility is day-one structural**, not deferred (ADR 0005/D16).

## Next step
The RAD half of Phase 2 → Phase 3 (PRD §7): extend the `LibInfo` ABI with the component
descriptor (properties + events + **a11y role/name/state**), build the RmlUi backend behind the
D10 interface, then the minimal designer. Two rules from the spike: always instantiate forms into
a **stylesheet-seeded** document (D21), and never let RmlUi types leak through the D10 interface —
that leak is what would make the substrate choice irreversible.
