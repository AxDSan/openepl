# ADR 0005 — UI stack, licence policy, and the decisions that follow

**Status:** ✅ **Accepted** (owner-approved 2026-08-30) · **Phase:** 2→3 boundary
**Supersedes:** the "PROPOSED" status of [ADR 0004](0004-q9-ui-substrate.md)
**Evidence:** [`spikes/q9-rmlui/RESULTS.md`](../../spikes/q9-rmlui/RESULTS.md)

Closes **Q9** (UI toolkit), **Q3** (string model) and **Q5**, and records five
further calls that follow from them.

---

## D14 — Substrate: adopt **RmlUi** (MIT), behind a swappable backend interface

RmlUi 6.3 is the widget substrate for both the runtime and the designer. It is the
only surveyed candidate clearing licence + closed static linking + **native
string-keyed component introspection** + FMX-class visuals at once, and the spike
cleared all four kill-risks: effects render through the *unmodified* reference
GL3 backend; our own component type and property registered and driven by string;
**2,576 KiB** stripped (2.4–3.3× smaller than the Rust toolkits); accessibility
reachable.

It stays behind the **D10** backend interface. The interface costs little and
buys an exit — RmlUi is a games/embedded engine with no desktop conventions, one
principal maintainer, and no accessibility, so reversibility is bought
deliberately, not assumed away.

**Known work carried forward** (from the spike): the C shim; HarfBuzz via
`FontEngineInterface` for complex scripts; the AccessKit bridge; replacing the
SDL2_image-based backend to shed unneeded codec dependencies.

## D15 — Licence policy (written once, applied to every dependency)

| | |
|---|---|
| **Accept** | MIT, BSD-2/3, **Apache-2.0**, Zlib, ISC |
| **Reject** | GPL/LGPL — *unless* an explicit static-linking exception exists (as FLTK has); non-OSI bespoke grants; proprietary/commercial-only |

PRD §9's "MIT/BSD" is read as shorthand for *permissive, no copyleft*, not a
considered exclusion of Apache-2.0 — whose patent grant is a **benefit**.
Excluding it would buy nothing while removing AccessKit's preferred dual option
and the GPUI/Xilem fallbacks. This is policy so the question stops being
re-litigated per dependency.

The rejections are load-bearing and were decisive in practice: **Slint**
(non-OSI grant forbidding apps that re-expose its APIs), **Qt** (LGPLv3 static
relink duty), **GTK4** (LGPLv2, no linking exception), **Ultralight** (static
linking contractually forbidden).

## D16 — Accessibility is a day-one structural requirement

Every component descriptor carries **role, name and state in the same
`LibInfo`-style table as its properties**, and the AccessKit bridge ships with
the first widgets in Phase 3 — not after.

Custom drawing yields **zero** free accessibility. FireMonkey is the cautionary
case: still shipping a11y as a separate add-on package 14 years in. Beyond the
ethics, it is a procurement gate in public-sector and enterprise markets. This is
the decision that gets structurally harder every week it is deferred, so it is
taken first and made non-optional.

## D17 — Data binding: data-aware components over a general binding engine

Build a general binding engine internally, but make the **default surface**
VCL-style **data-aware components** — drop a grid, point it at a data source.
FMX shipped binding-expressions-only (LiveBindings) and it is a documented,
persistent friction point; "batteries included" (§5.4) and G9 both point the
other way.

Components arrive in Phase 5, but the **architectural consequence lands now**:
the component model must carry a data-source property concept from the start.

## D18 — The IDE dogfoods RmlUi

One UI stack. The designer canvas then manipulates **real components**, which is
what D9 actually requires for WYSIWYG fidelity; Godot proves the dogfooding
pattern at scale.

**Accepted caveat:** an IDE needs native menus, file dialogs and IME far more
than a user app does, and RmlUi supplies none of them. Platform-native dialogs
and menus are budgeted separately regardless of substrate.

## D19 — Language split: C core, C++ UI layer, Rust compiler

| Layer | Language | Why |
|---|---|---|
| Core runtime (`libopenepl_core`) | **C** | No exceptions/RTTI — required by the freestanding/kernel profile (G10, §5.6) |
| UI component layer | **C++** | That is what RmlUi is |
| Compiler / IDE tooling | **Rust** | ADR 0001 |

Boundary is the **C ABI we already have** (`abi/openepl_abi.h`). Three languages
is a real cost, but the split is *forced by the goals*, not chosen: the G10
driver profile cannot link a C++ UI stack, so the core had to stay C regardless.
This also closes the runtime-language half of **D6**.

## D20 — Strings are UTF-8 everywhere (closes Q3)

No GBK semantics in the core. RmlUi, FreeType, HarfBuzz and AccessKit are all
UTF-8; a future EPL importer converts at its boundary. Q3's "EPL fidelity" side
loses because fidelity to a 32-bit Windows codepage buys nothing on the
cross-platform, English-first path (G5/G9).

## D21 — The runtime always seeds a stylesheet

Forms are instantiated into a **stylesheet-seeded document**, never a bare
`Context::CreateDocument()`. Encoded in the runtime API so it cannot be got wrong.

*Why this is an ADR and not a code comment:* the spike's first run **passed every
assertion while rendering almost nothing** — decorators silently no-op without a
stylesheet context, and `SetProperty` still returns `true`. It is a silent,
baffling failure mode, and the rule that prevents it must outlive the person who
found it.

---

## Consequences

- **Q9, Q5, Q3 closed.** Q1 and the runtime half of D6 were closed by ADRs 0001
  and D19. **Still open: Q2** (EPL importer), **Q4** (CJK localization scope),
  **Q6** (hardening dial), **Q7** (ease/power line), **Q8** (first systems target).
- **Phase 3 can start.** The substrate, component-model shape, and a11y contract
  are all fixed.
- **New risk R8** (accessibility debt) and amended **M2/M3/R7** are recorded in
  the PRD.
- Reversibility rests entirely on **D10** being honoured. If the backend
  interface is allowed to leak RmlUi types, D14 silently becomes irreversible.
