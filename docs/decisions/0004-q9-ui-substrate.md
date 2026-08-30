# ADR 0004 — Q9: the UI substrate for the RAD environment

**Status:** ⚠️ **PROPOSED — owner decision required.** Nothing here is committed.
**Spike outcome:** ✅ **RmlUi passed all four kill-risks** — see [`spikes/q9-rmlui/RESULTS.md`](../../spikes/q9-rmlui/RESULTS.md).
**Date:** 2026-08-30 · **Blocks:** PRD Phase 2 (component/event runtime) and Phase 3 (RAD vertical slice)

---

## 0. Recommendation in one paragraph

Adopt the **Lazarus widgetset pattern** — OpenEPL owns the component model over a
thin, swappable **widget-backend interface** — so the substrate stays reversible.
For the substrate itself, **RmlUi (MIT)** is the recommended primary candidate:
it is the only option clearing licence + closed static linking + **native
string-based component introspection** + FMX-class visuals simultaneously. It
wins by elimination, not comfort, and carries a real bill (render backend, C
shim, HarfBuzz, AccessKit) — so **de-risk it with a time-boxed spike (§8) before
committing**, with **iced** as the fallback if the bill proves too high. Reject
Slint and Qt (licence + G8), GTK4 for linking, Ultralight (statically forbidden),
and all webview stacks.

> ⚠️ **Revision note.** An earlier draft recommended iced. The C/C++ survey
> returned afterwards with primary-source evidence for RmlUi and for two
> independent Qt disqualifications; the recommendation changed on that evidence.
> §9's cost analysis is why: the costs that looked like RmlUi's disadvantages
> turn out to be **shared with every custom-drawn candidate, iced included**.

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

1. **No *viable* candidate gives us a C ABI for free.** Zero of nine Rust
   toolkits expose a C surface, and RmlUi's API is C++ — so a shim is required.
   Native C APIs *do* exist (LVGL, NAppGUI, IUP, Nuklear, Blend2D), but each
   fails elsewhere: LVGL moved its XML engine to a paid tier, NAppGUI and IUP
   have low visual ceilings, IUP is on hold, and Blend2D is a bare rasteriser.
   **A C shim is therefore a cost of every candidate that clears the other
   gates** — it should not decide the choice.
2. **Runtime instantiate-by-name is rare — but not extinct.** Slint's
   interpreter and Qt's meta-object system both offer it and both are
   disqualified on licence/G8 grounds (§4). **RmlUi is the one survivor that has
   it natively** (§5.1); every other permissive option is compile-time-typed
   builders, where we supply the string→constructor registry ourselves.
3. **Every toolkit is custom-drawn, and every visually impressive app in
   evidence — COSMIC, Rerun, Zed — shipped its own widget and theme layer.**
   We build our look **regardless of choice**.

> **Therefore Q9 is not "which toolkit hands us a component model." We own that
> model in every scenario — exactly as `LibInfo` already owns the command table.
> Q9 is: _which substrate gives the best visual ceiling, permissively, at
> acceptable risk — and how much of the registry does it save us?_** RmlUi is
> recommended because it is the only survivor that answers both halves.

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
| **Qt (Widgets and Quick)** | **Fails on two independent grounds.** (1) **LGPLv3 static linking:** Qt's own guidance states that with static linking "the application itself may no longer be 'work that uses the library' and thus become subject to LGPL," and users must be able to "change and re-link the library… including reverse engineering." Complying while statically linked means shipping source **or relinkable object files** — the exact opposite of a hardened non-decompilable binary (G8). (2) **QML output is not opaque:** the open-source `qmlcachegen` emits a compilation unit retaining "document structure, compact byte code, and native code," while `qmlsc` — which "can generate more efficient C++ code" — is a **commercial-only** add-on. So Qt Quick **fails G8 independently of the licence question**: the free toolchain does not produce opaque output. Painful, because Qt is best-in-class on introspection (`QUiLoader` + `QMetaObject`) and accessibility, and Qt Quick is the closest true FMX analogue. |
| **Ultralight** | **Static linking explicitly forbidden** — free-licence §4.2: *"Licensee agrees not to access the Ultralight.dll using static-linking tools or other methods that hide or conceal any of the Ultralight software."* Also a US$100k turnover/funding eligibility ceiling (§2.3). |
| **Slint** | **Licence — fatal.** SPDX: `GPL-3.0-only OR LicenseRef-Slint-Royalty-free-2.0 OR LicenseRef-Slint-Software-3.0` — copyleft, paid commercial, or a **non-OSI** grant. No MIT/BSD option. Royalty-Free §3 verbatim: *"The License does not permit the distribution of Application that exposes the APIs, in part or in total, of the Software"* — re-exposing widget APIs is precisely what a RAD tool does. Also forbids embedded use (vs G12). **Painful:** it was the only toolkit natively solving runtime property get/set by name. |
| **GTK4 (for linking)** | LGPLv2 with **no** linking exception → static linking triggers §6 relink duties, contradicting G8/G12. **But keep as a _design oracle_:** GObject + GtkBuilder is the most mature system doing exactly what our component model needs — study and reimplement, never link. Mirrors the PRD's existing posture toward BlackMoon's `kernel.lib` (D5). |
| **wxWidgets / FLTK / libui** | Visual ceiling. (FLTK's licence is fine — LGPL **with** a static-linking exception — and it is custom-drawn, tiny and the closest living analogue to VB6/Delphi. But its look is the antithesis of FMX, and its repo has been static for ~3 months.) |
| **Tauri / Wails / Neutralino / webview** | Ships HTML/JS assets and depends on a **system webview** (webkit2gtk-4.1 on Linux, WebView2 pre-Win11) → three different engines, inconsistent rendering, fails G8's spirit. Tauri/Wails/Neutralino also have **no plain-C drivability**. |
| **Servo embedding** | MPL-2.0 and genuinely embeddable now (crate 0.5.0, 2026-08), but pre-1.0 with **monthly breaking API changes**; **Verso archived 2025-10-08**. Embedding = bundling an engine, the opposite of small binaries. |
| **Ultralight / Sciter** | Proprietary, commercial-only. Fails §9. |

## 5. The finalists

### 5.1 RmlUi — recommended primary

**What it is:** an embeddable HTML/CSS-style UI engine with a **retained DOM** —
`.rml` (HTML-like) + `.rcss` (CSS-like) parsed into a persistent tree of
`Element` objects. A real retained component tree, which is what a visual
designer needs. **MIT** (verified from `LICENSE.txt`); no copyleft, no
static-linking condition, no attribution-in-UI requirement. Active: 6.3 released
2026-08-22, commits through 2026-08-29.

**Why it wins — verified public API:**

```cpp
ElementPtr CreateElement(const String& name);              // create by tag name
bool       SetProperty(const String& name, const String& value);
const Property* GetProperty(const String& name);
void       SetAttribute(const String& name, const T& value);
static void Factory::RegisterElementInstancer(const String& name, ElementInstancer*);
```

`RegisterElementInstancer` lets us **register our own component types under our
own string names and instantiate them by name** — a direct structural analogue of
EPL's `GetNewInf`/`LIB_INFO` component registration. Create-by-string,
get/set-property-by-string, and register-custom-type-by-string are all first-class.
This is precisely the capability that made Slint uniquely attractive, available
here under MIT.

**Visual ceiling — genuinely FMX-class.** RCSS provides `box-shadow`,
**`filter` and `backdrop-filter`** (blur, drop-shadow, hue-rotate, brightness,
contrast, grayscale, invert, opacity, sepia), `mask-image`, `border-radius`,
decorators (image, tiled, **ninepatch**, linear/radial/**conic** gradients, a
custom **shader** decorator), full 2D **and 3D** transforms with `perspective`,
and `@keyframes`/`animation`/`transition` with **10 tweening families × in/out/
in-out**. Animatable: numbers, lengths, angles, colours, transforms, decorators,
filters. (Known limit: box-shadows are not animatable.)

**Bonus that matters structurally:** RML/RCSS are **text formats**. The designer
can serialise forms to them, giving us the `.dfm`/`.fmx` form-file analogue for
free, diffable in git.

**The adversarial case (it wins by elimination, not by fit):**
- It is a **games/embedded UI engine, not a desktop toolkit** — its shipped-app
  list is overwhelmingly games. No native menus, platform dialogs, IME handling,
  or clipboard/drag-drop conventions.
- **Renderer-agnostic**: we implement `RenderInterface`/`SystemInterface`.
  Mitigating: it ships reference backends, and the **OpenGL 3 one implements the
  full effects set** (DX11/DX12 added in 6.3) — so this is *adapt*, not
  *write from zero*. Still budget it as a real subsystem: if a backend omits a
  feature, that feature silently does nothing.
- **C++ API** → we write and maintain a C shim (bounded: create, get/set
  property, append child, register instancer, event binding).
- **No HarfBuzz** → no complex-script shaping. Arabic/Hebrew/Devanagari will not
  render correctly. FreeType is the default engine and `FontEngineInterface` is
  designed to be replaced, so this is a known integration — but for our planned
  localization layer it is **required work, not polish**. (CJK basic rendering is
  far less shaping-dependent, so the Chinese-localization path is less exposed.)
- **Zero accessibility.** No screen-reader support, no platform bridge.
- **No visual designer** exists — we build it.
- **Bus factor ~1** (predominantly one maintainer). MIT means we *can* fork, but
  it would be forking a large codebase.
- **"HTML/CSS-like" is not HTML/CSS** — RCSS is CSS2-based with parts removed or
  altered; web knowledge transfers imperfectly and will surprise users.
- ~~Binary size unknown~~ → **measured: 2,576 KiB** stripped, the smallest of any
  candidate (§9).

### 5.2 iced — recommended fallback

MIT, retained-mode, the only permissive toolkit with evidence of carrying a
**general-purpose desktop at scale** (System76's COSMIC). 0.14 added an Animation
API and `shader::Pipeline`; has a software renderer (tiny-skia) for headless/CI.
Gives us windowing, input, IME, clipboard and a real widget set out of the box —
the things RmlUi does not. Costs: **no string introspection** (we supply the
registry), **not an AccessKit adopter**, and **8610 KiB** hello-world (§9).

### 5.3 Also considered

| | Licence | Verdict |
|---|---|---|
| **GPUI** | Apache-2.0 | Best *proven* visual quality (Zed, 3 OS × 2 arch), AccessKit. But pre-1.0, **crates.io 10 months stale → git-only dependency**, typed-only, built for an editor. |
| **makepad** | MIT | Highest *capability* (per-widget runtime-compiled shaders, 34 easing curves). But **mid-rewrite** (HEAD 15 months ahead of crates.io), **designer deleted at HEAD**, **zero accessibility**, single-vendor. Too risky as a foundation. |
| **egui** | MIT/Apache-2.0 | Immediate mode dissolves the reflection problem and `Visuals` is plain serde data; but weakest visual ceiling (**no gradient shape**) and immediate mode fits a retained component model poorly. **6113 KiB** (§9). |
| **LVGL** 9.6 | MIT, **native C API** | Structurally attractive — but its **XML engine was removed in 9.5 and moved to the paid Pro tier**. Viable only pinned ≤9.4.0. **Treat as a governance red flag**: the same class of risk as Slint's licence, realised. |
| **NAppGUI** | MIT | Best C ABI of any candidate (C90), built for static linking — but native-widget, low visual ceiling. |
| **wxWidgets** | wxWindows Licence — genuinely permissive (§2: "you may … distribute under your own terms, binary object code versions") | Licence ✓, visuals ✗. Signal worth heeding: **Audacity, its flagship app, is migrating to Qt6/QML** citing HiDPI, accessibility and ARM64. |
| **FLTK** | LGPL + explicit static-link exception (quoted §4) | Licence ✓, tiny (~1.1 MB), but **lowest visual ceiling** — the antithesis of FMX. |
| **IUP** | MIT | **`IupCreate(class)` + `IupSetAttribute(name, value)` is exactly the right API shape** — but **"on hold" since 2023**. **Use as an API blueprint, not a dependency.** |
| Dear ImGui / Nuklear / raylib | permissive | Immediate-mode, structurally wrong for a designer. ImGui's own FAQ disclaims application UI use. |

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
3. **Appetite for infrastructure — the decisive question.** RmlUi (§5.1) buys us
   the component-registration model and an FMX-class visual ceiling, but bills us
   a render backend, a C shim, HarfBuzz, and AccessKit. **Two of those four
   (HarfBuzz, AccessKit) are owed with *any* custom-drawn substrate, iced
   included**, and the render backend is an *adaptation* of a full-featured
   reference backend rather than new work — so the true RmlUi-specific premium is
   smaller than it first appears. Still, it is four subsystems before the first
   designer feature. *Recommend: fund the §8 spike rather than deciding blind.*
4. **M3's "0 detections" is not reachable by architecture alone** — see §9.

## 8. The forcing function: a time-boxed spike, then the bake-off

RmlUi is recommended but **not yet proven for this use**. Before committing, run a
**two-week spike** that attacks its four risks in the order they can kill it:

1. ~~Adapt the reference OpenGL 3 backend and confirm the full effects set.~~
   ✅ **DONE — passed with no backend modification at all.** Gradients,
   `backdrop-filter: blur(22px)`, drop-shadow, box-shadow, transforms and font
   glow all render. **Found a trap:** decorators silently no-op unless the
   document has a stylesheet context — seed one always.
2. ✅ **DONE (in C++; the shim itself remains).** Registering our own property
   name and our own element tag, creating by string, setting 14 properties by
   string, reading one back, and dispatching to a string-registered listener all
   verified working.
3. ✅ **DONE — 2,576 KiB**, 2.4–3.3× smaller than the Rust toolkits.
4. ✅ **DONE (reachability confirmed).** RmlUi exposes tree walk, bounds and
   focus state; AccessKit 0.25.0 ships C bindings. Real work, no blocker.

If any of 1–4 fails or overruns, fall back to **iced** and re-run the same
benchmark. Either way, hold both to this bar:

| Criterion | Pass bar |
|---|---|
| Visual | rounded cards, gradient fill, drop shadow, blur, 60fps property animation, runtime theme switch |
| Component | create by string name + set 10 property types by name + fire events |
| A11y | screen reader announces roles/names on all 3 OSes |
| Size | stripped hello-world binary |
| Portability | Linux/macOS/Windows × x86-64/arm64; headless/software-render path |
| Effort | hours to add a *new* component type end-to-end |

## 9. Measurements taken (research found no published figures)

Measured locally, release + `opt-level="z"` + LTO + `codegen-units=1` + strip:

| Toolkit | Stripped hello-world binary |
|---|---|
| **egui/eframe 0.32.3** | **6113 KiB** (~6.0 MiB) |
| **iced 0.14.0** | **8610 KiB** (~8.4 MiB) |

Published figures for comparison (the only candidates with real numbers):
**FLTK ~1.1 MB**, **wxWidgets ~600 KB–1 MB**. RmlUi and Qt have **no published
figure**. (egui 0.36.1 **requires rustc 1.95**; this machine has 1.94.1 —
toolchain churn, cf. R5.)

**The C++ substrates are roughly 6–8× smaller than the Rust ones measured.** That
is a real point in RmlUi's favour for M2, though unverified for RmlUi itself —
hence spike step 3.

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

## 11. Substrate-path analysis (gap now closed)

The "own widget layer on a 2D renderer" path — what Embarcadero actually did for
FMX — was costed after the first draft. Verdict: **do not take it now.**

| | Skia | Blend2D |
|---|---|---|
| Licence | BSD-3-Clause ✓ | Zlib ✓ (explicitly permits static linking, open or commercial) |
| C API | 🔴 **Google removed it upstream.** Now maintained by the **SkiaSharp** project for its C# bindings, with the caveat that it "really is at the whim of the underlying C++ API." | ✅ **C API is the *primary* API** — the C++ layer is inline wrappers over it. Our emitted code could call it with **no shim**. |
| Rendering | GPU (Vulkan/Metal/D3D/GL) + CPU; powers Chrome, Android, Flutter | CPU only, JIT-compiled SIMD pipelines |
| Maturity | Enormous, battle-tested | 🔴 **pre-1.0 (0.21.2)**, single maintainer, site carries a funding appeal |

Skia is the stronger *renderer*; Blend2D is the better *C-ABI citizen*. But both
give **only a rasteriser** — identical to each other in what they omit. Taking
this path means owning: windowing and input (Wayland/X11/Win32/Cocoa or
SDL3/GLFW), the **entire widget set** (button…tree, grid, combo, tabs,
scrollbars, menus, dialogs, each with focus and keyboard navigation), a layout
engine with HiDPI, the full text stack (line breaking, bidi, HarfBuzz shaping,
IME for CJK, selection/editing), the event/property system, the accessibility
tree, and platform conventions (clipboard, drag-and-drop, native file/print
dialogs, macOS menu bar).

**That is multiple engineer-years to reach parity with what FLTK gives free**, and
it is R7 — the PRD's largest execution risk — taken deliberately and at maximum
dose. Revisit only if RmlUi and iced both fail §8.

## 12. Design oracles (study, never link)

Three systems solve our exact problem and cannot be dependencies. Reimplement
their *ideas*, mirroring the PRD's posture toward BlackMoon's `kernel.lib` (D5):

- **GObject + GtkBuilder** — the most mature runtime type/property system:
  resolve GType by class name, convert textual property values, connect signals
  by name. Licence forbids linking; the design is the prize.
- **IUP** — `IupCreate(class)` + `IupSetAttribute(name, value)` is almost exactly
  the C API our component ABI wants. MIT, but on hold since 2023.
- **Delphi RTTI + `.dfm` streaming** — one primitive (`published` ⇒ RTTI)
  generating form streaming, the Object Inspector, and event binding.

## 13. Sources

All licence facts verified first-hand from `LICENSE` files, official licensing
pages, or public headers; API signatures quoted from public headers. Key
uncertainties flagged inline. Unverified: RmlUi arm64 tier (architectural
inference), RmlUi binary size, Blend2D 2025–26 activity, shipped-app lists.
