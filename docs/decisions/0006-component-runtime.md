# ADR 0006 — Component/event runtime (Phase 2, RAD half)

**Status:** ✅ Accepted · **Date:** 2026-08-30 · **Phase:** 2 (PRD §7)

Delivers the PRD's Phase 2 RAD milestone: *"a form with a button + an event
handler, authored in IR, compiles and runs on the portable UI layer (no designer
yet)."* Spec: [`docs/spec/components.md`](../spec/components.md).

## The UI is a support library, not a built-in

`libs/ui/` contributes its components through the **same `LibInfo` mechanism**
that carries commands, and the compiler introspects it the same way (`use ui`).
The alternative — a component registry hardcoded in the Rust compiler — was
rejected because we already paid that migration once for commands (ADR 0003) and
would have paid it again. It also means third-party component libraries are the
same thing as first-party ones, which is the point of D9/D11.

Libraries needing more than plain C carry a **`lib.json`** manifest (sources,
`cxx`, include dirs, defines, pkg-config, link args, prerequisites). Generic, so
the loader special-cases nothing.

## The metadata `.so` is metadata *only*

Introspection builds **only** the `*_libinfo.c` TU into its `.so`. It references
implementations by symbol *name*, never by pointer, so it needs none of them.
This keeps introspection fast and — discovered the hard way — makes it possible
at all: a statically-linked RmlUi is not position-independent and cannot go into
a shared object. The metadata/implementation split (D12) turned out to be
load-bearing for reasons beyond dead-stripping.

## The UI stack links conditionally

The UI and its dependencies (RmlUi, SDL2, GL, FreeType, libstdc++) link **only
into modules that declare a form**; the link driver switches to `clang++` only
then. Without this, every console program would carry megabytes of widget
toolkit and the dead-strip guarantee (M2/D3) would be hollow. Guarded by
`console_programs_do_not_link_the_ui`, which fails if `libSDL2`, `libGL` or
`libfreetype` appear in a console binary's `ldd`.

Because `clang++` compiles `.c` as C++ (mangling `ECodeStart` and breaking the C
ABI the emitted IR calls), each source is marked with an explicit `-x c` / `-x c++`.

## Multiple subroutines, and the entry point

Each subroutine now lowers to its own `@oe_user_<name>` function — required,
because **an event handler is just a subroutine** and must be addressable. The
previous single-`main` restriction is gone.

Entry: **a module with a form** builds the form and runs the UI loop (`main`, if
present, runs first as start-up code); **a module without one** keeps the console
path unchanged.

## Handlers bind by pointer; identifiers never ship

Events bind to **function pointers**, never names. There is no name-based
dispatch table at run time, so no user identifier is emitted as data (G8).
Accessible names come from user-facing *text*; a component with no text gets a
role and no name rather than leaking its id.

**Deferred, with the intended answer recorded:** when code can read/write
`button1.text` (Phase 3), ids must reach the runtime. Intern them to integers at
compile time so identifiers still never ship. Not solved now because v0 does not
need it — but it is the next thing that could quietly violate G8.

## Property names use underscores

`background_color`, not `background-color`: hyphens would collide with the minus
operator in the lexer, and underscores match the rest of the language
(`print_text`, `int_to_text`). The UI backend translates to the substrate's
spelling (RCSS hyphens) at the boundary — the substrate's vocabulary stops there.

## Accessibility data exists before the bridge does

Every component descriptor carries an `a11y_role`, and `oe_ui_set_a11y(handle,
role, name)` is emitted for every widget (D16). Nothing consumes it yet — the
AccessKit bridge is Phase 3 — but the information is captured at construction,
which is the entire point: FMX's failure was that the data was never there to
retrofit.

## Headless GUI testing

`oe_ui_run` honours three environment variables — `OPENEPL_UI_EXIT_AFTER_FRAMES`,
`OPENEPL_UI_SYNTH_CLICK` (a widget **handle**, since ids do not ship), and
`OPENEPL_UI_DUMP` — so a GUI can be exercised in CI. Crude, and deliberately so:
the alternative was no GUI test at all. They cost nothing when unset. Replace
with a proper harness when one is warranted.

## Consequences

- **PRD Phase 2 is complete.** `examples/form.oir` → native GUI binary; a
  synthetic click reaches the handler and prints through a core command.
- 20 tests green, including dead-strip and the conditional-link guard.
- RmlUi is vendored via `tools/fetch-rmlui.sh` into a gitignored `vendor/`.
- **Not built:** property access from code, dynamic component creation, layout
  containers, components beyond form/button/label, the designer, the AccessKit
  bridge, data binding.
- **Untested:** arm64, macOS, Windows. Text shaping (HarfBuzz) is still absent,
  so complex scripts will not render correctly.
