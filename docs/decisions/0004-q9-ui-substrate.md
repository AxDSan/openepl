# ADR 0004 — Q9: the UI substrate for the RAD environment

**Status:** ⚠️ **PROPOSED — owner decision required.** Nothing here is committed.
**Date:** 2026-08-30 · **Blocks:** PRD Phase 2 (component/event runtime) and Phase 3 (RAD vertical slice)

---

## 0. Recommendation in one paragraph

**Do not bet the product on a single toolkit yet.** Adopt the **Lazarus widgetset
pattern**: OpenEPL owns the component model (properties/methods/events) over a
thin, swappable **widget-backend interface** — which turns Q9 from an
irreversible bet into a reversible one. Build the Phase-3 vertical slice on
**iced** (MIT, retained-mode, proven at desktop-environment scale by COSMIC,
has a software renderer), then run a **time-boxed bake-off** — iced vs GPUI vs
makepad against a fixed "stunning + accessible + small" benchmark — before
Phase 5 hardens the component library. Reject Slint (license), GTK4 for linking
(license), and all webview stacks (G8 + no C ABI).

---

## 1. What changed: the requirement got sharper

The owner amended the target: **the RAD portion must be like Embarcadero RAD
Studio — its toolchain + component suite — and users must be able to build
visually stunning GUIs.**

RAD Studio splits two ways, and the amendment picks a side:

| | VCL | **FireMonkey (FMX)** ← our model |
|---|---|---|
| Widgets | thin wrapper over native Win32 controls | **every pixel drawn by the framework** |
| Platforms | Windows only, permanently | Windows/macOS/Linux/iOS/Android |
| Visuals | capped by the OS theme engine | **unlimited** — GPU, styles, animation, effects |
| Cost | free platform fidelity + free accessibility | **you now own fidelity and accessibility** |

That last row is the whole trade, and it is now consciously made:
**custom-drawn wins, native-widget wrappers are out.** wxWidgets, FLTK and libui
render host-OS controls (or, in FLTK's case, a dated custom look) and cannot be
skinned to an FMX standard — they are excluded on *visual ceiling*, not licence.

## 2. The reframing that makes this decision tractable

Three verified findings collapse into one conclusion:

1. **No candidate toolkit ships a C ABI.** Zero of nine Rust toolkits expose a
   `cdylib`/`staticlib` C surface; Slint's is C++. We write a Rust `extern "C"`
   shim **regardless of choice**.
2. **Runtime instantiate-by-name barely exists permissively.** Only Slint's
   interpreter offered it natively, and Slint is disqualified (§4). Everything
   else is compile-time-typed builders, so we build a string→constructor
   registry and string→setter dispatch **regardless of choice**.
3. **Every toolkit is custom-drawn, and every visually impressive app in
   evidence — COSMIC, Rerun, Zed — shipped its own widget and theme layer.**
   We build our look **regardless of choice**.

> **Therefore Q9 is not "which toolkit gives us a component model." We own the
> component model in every scenario — exactly as `LibInfo` already owns the
> command table. Q9 is only: _which rendering/widget substrate gives the best
> visual ceiling, permissively, at acceptable risk?_**

This is also why the decision is *deferrable*: if the component model is ours
and the backend sits behind an interface, the substrate is swappable.

## 3. Hard gates (from the PRD + the amendment)

1. **Permissive licence**, safe to **statically link** into hardened binaries (§9, N3, G8).
2. **Drivable from C ABI** by our emitted machine code (via our shim).
3. **Runtime widget creation from data** — the IR describes forms declaratively;
   compile-time-macro-only trees are disqualifying.
4. **FMX-class visuals** — styling engine, property animation, effects, GPU.
5. **Linux + macOS + Windows, x86-64 + arm64** (G5).
6. **No runtime unpacking; small output** (G8, M2).

## 4. Rejected, with reasons

| Rejected | Reason |
|---|---|
| **Slint** | **Licence — fatal.** SPDX: `GPL-3.0-only OR LicenseRef-Slint-Royalty-free-2.0 OR LicenseRef-Slint-Software-3.0` — copyleft, paid commercial, or a **non-OSI** grant. No MIT/BSD option. Royalty-Free §3 verbatim: *"The License does not permit the distribution of Application that exposes the APIs, in part or in total, of the Software"* — re-exposing widget APIs is precisely what a RAD tool does. Also forbids embedded use (vs G12). **Painful:** it was the only toolkit natively solving runtime property get/set by name. |
| **GTK4 (for linking)** | LGPLv2 with **no** linking exception → static linking triggers §6 relink duties, contradicting G8/G12. **But keep as a _design oracle_:** GObject + GtkBuilder is the most mature system doing exactly what our component model needs — study and reimplement, never link. Mirrors the PRD's existing posture toward BlackMoon's `kernel.lib` (D5). |
| **wxWidgets / FLTK / libui** | Visual ceiling. (FLTK's licence is fine — LGPL **with** a static-linking exception — and it is custom-drawn, tiny and the closest living analogue to VB6/Delphi. But its look is the antithesis of FMX, and its repo has been static for ~3 months.) |
| **Tauri / Wails / Neutralino / webview** | Ships HTML/JS assets and depends on a **system webview** (webkit2gtk-4.1 on Linux, WebView2 pre-Win11) → three different engines, inconsistent rendering, fails G8's spirit. Tauri/Wails/Neutralino also have **no plain-C drivability**. |
| **Servo embedding** | MPL-2.0 and genuinely embeddable now (crate 0.5.0, 2026-08), but pre-1.0 with **monthly breaking API changes**; **Verso archived 2025-10-08**. Embedding = bundling an engine, the opposite of small binaries. |
| **Ultralight / Sciter** | Proprietary, commercial-only. Fails §9. |

## 5. The finalists

| | Licence | Visual ceiling | Maturity / risk | A11y | Dyn-by-name |
|---|---|---|---|---|---|
| **iced** ← *proposed for Phase 3* | **MIT** | **Best _proven_ among permissive retained-mode — System76 ships the whole COSMIC desktop on it.** 0.14 added an Animation API; `shader::Pipeline` for custom effects; gradients, shadows, rounded corners | 0.14.0 (2025-12), active, **software renderer** (tiny-skia) for headless/CI/VM | ⚠️ **not an AccessKit adopter — the gap to close** | ❌ typed builders (we supply the registry) |
| **GPUI** | Apache-2.0 ⚠️ | **Best _proven_ overall — Zed ships on it**, 3 OS × 2 arch; spring physics, Oklab gradient interpolation | pre-1.0, **crates.io 10 months stale → git dependency only**; built for an editor, not general apps | ✅ AccessKit | ❌ typed structs |
| **makepad** | MIT | **Highest _capability_** — per-widget **runtime-compiled shaders** (GLSL/HLSL/Metal/WGSL), SDF 2D library, 34 easing curves. The most FMX-like ceiling here | 🔴 **mid-rewrite**: HEAD 2.0 unpublished, 15 months ahead of crates.io; single-vendor; **its visual designer was deleted at HEAD** | 🔴 **none — stubbed out** | ⚠️ partial (`LiveId` apply_over) |
| **egui** | MIT/Apache-2.0 | Weakest — utilitarian; **no gradient shape**. But `Visuals` is a plain serde struct → best runtime theme swapping | very active; text stack rebuilt 2026 (skrifa/harfrust) | ✅ AccessKit | ⚠️ immediate mode *dissolves* the problem — you interpret your own tree each frame |

**Why iced for the Phase-3 slice, not the flashiest option:** COSMIC is the only
evidence in this survey of a permissive toolkit carrying a *general-purpose
desktop UI at scale*, which is our exact shape — Zed is one very polished app by
its own authors, and makepad is a rewrite with no accessibility and no designer.
Phase 3's job (PRD §7) is a *deliberately minimal* designer proving the
edit→run loop, not a beautiful widget library. Choose the low-risk substrate to
prove the loop; choose the beautiful one once the benchmark is real.

## 6. What is decided vs. deferred

**Decide now (structural, low-risk, unblocks Phase 2/3):**
- **D10 — Lazarus-style widgetset abstraction.** OpenEPL component model +
  reflective property/event registry over a thin **widget-backend interface**.
  Validated pattern: Lazarus runs one component model over Win32/GTK/Qt/Cocoa/
  custom-drawn backends. Makes the substrate swappable.
- **D11 — the reflective component table is first-class in the IR and the ABI.**
  Delphi's *entire* RAD loop falls out of one primitive: `published` members get
  RTTI, and that single fact generates form streaming, the Object Inspector, and
  event-handler binding. Our `LibInfo` already does this for commands; components
  extend it with properties + events.
- **D12 — accessibility is designed in from day one, not bolted on.** This is
  FMX's single worst failing (a separate add-on package, with reported compile
  errors). Custom drawing gives **zero** free accessibility. Every component
  carries an a11y role/name/state in the same descriptor as its properties.
- **D13 — Embarcadero's runtime/design-time package split is our model** and we
  already have it: implementations link into the program, the metadata catalogue
  never does (`core_libinfo.c`, ADR 0003).

**Defer (with a forcing function):**
- **Final substrate** → bake-off before Phase 5, benchmark in §8.

## 7. Open items the owner must rule on

1. **Is Apache-2.0 acceptable?** PRD §9 says MIT/BSD. Apache-2.0 is permissive
   and compatible (adds a patent grant), but **GPUI and Xilem are Apache-2.0-only**
   — ruling it out removes GPUI from the bake-off. *Recommend: accept.*
2. **Data-aware controls vs binding expressions.** VCL ships data-aware controls;
   **FMX has none** and uses LiveBindings instead — a frequent user friction
   point. "Batteries included" (PRD §5.4) implies the VCL model. *Recommend:
   decide before the component library grows.*
3. **M3's "0 detections" is not reachable by architecture alone** — see §9.

## 8. The bake-off (the forcing function)

Build the *same* small styled app on each finalist behind the D10 backend
interface, and measure:

| Criterion | Pass bar |
|---|---|
| Visual | rounded cards, gradient fill, drop shadow, blur, 60fps property animation, runtime theme switch |
| Component | create by string name + set 10 property types by name + fire events |
| A11y | screen reader announces roles/names on all 3 OSes |
| Size | stripped hello-world binary (see §9) |
| Portability | Linux/macOS/Windows × x86-64/arm64; headless/software-render path |
| Effort | hours to add a *new* component type end-to-end |

## 9. Measurements taken (research found no published figures)

Measured locally, release + `opt-level="z"` + LTO + `codegen-units=1` + strip:

- **egui/eframe 0.32.3 hello-world: 6113 KiB (~6.0 MiB).**
- iced 0.14: build in progress at time of writing — **unmeasured**.
- egui 0.36.1 **requires rustc 1.95**; this machine has 1.94.1 (toolchain churn, cf. R5).

⚠️ **Context that matters:** the current OpenEPL `demo` binary is **~6 KiB of
`.text`**. A 6 MiB GUI hello-world is *three orders of magnitude* larger and sits
uneasily beside BlackMoon's headline "smaller output" value proposition (M2).
M2 remains meaningful for the *core-command* dead-strip story, but the PRD should
state plainly that GUI binaries carry a multi-MiB substrate floor.

**AV correction (affects G7/M3).** Research found the identical Defender
`Wacatac.H!ml` detections on GitHub's own `gh` CLI and other plain Go binaries
with **no webview at all**, and on an *empty* Wails app **signed with an EV
certificate**. The trigger is ML heuristics against **unsigned, low-prevalence
native binaries** — which is exactly what freshly-compiled OpenEPL output is.
BlackMoon's real AV win was **no runtime unpacking**, which we keep. **M3's
"0 detections" is not achievable by architecture alone**; it needs code signing,
reputation building, and preferring MSI over NSIS. This belongs in R-series risks.

## 10. Proposed PRD amendments

1. **§4.5 / D9 / Q9** — record the reference model (RAD Studio lineage) and the
   visual target (**FMX-class**: custom-drawn, styled, animated, GPU). State that
   native-widget fidelity is explicitly *not* a goal.
2. **New D10–D13** as in §6.
3. **M2/M3** — qualify per §9: GUI output has a multi-MiB substrate floor; AV
   cleanliness requires signing + reputation, not architecture alone.
4. **R7** — note the FMX-class bar *raises* this, already the largest execution
   risk. Mitigation unchanged and now load-bearing: Phase 3 ships a deliberately
   minimal designer; "stunning" is the trajectory the substrate must not
   foreclose, not the Phase-3 deliverable.
5. **New risk R8 — accessibility debt.** Custom drawing yields zero free a11y;
   FMX shows what deferring it costs.

## 11. Research gap (honest)

The C/C++ toolkit survey — **Qt Quick/QML licensing** (the closest commercial
analogue to FMX, and its licensing is the crux) and **Skia as a direct substrate**
(the path Embarcadero itself took for FMX) — **had not returned when this was
written.** Two consequences:
- A Skia-based "own widget layer" strategy is **not yet costed**. It is the
  maximum-ceiling option and what FMX actually did, but it means owning layout,
  text, input and a11y — directly against R7. My prior is that it is a trap at
  this team size, but that is a prior, not a finding.
- Qt Quick/QML is unassessed. Expect LGPLv3 + a commercial option; LGPL static
  linking is the issue, and 14 Qt modules are GPL-only.

**Revisit §5 when that lands.** Nothing above depends on it except the
completeness of the finalist list.
