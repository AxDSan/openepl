# ADR 0013 — Build targets: one source, several artifacts

**Status:** accepted
**Date:** 2026-08-30
**Context:** PRD G12 ("every artifact, every platform"), §7 Phase 4

## Context

G12 says a project should produce a GUI or console executable, a dynamic
library, a static library, or a driver, as *a build-target choice over one IR
and component model*. Until now the toolchain had no target concept at all:
every build produced a Linux x64 executable. A New Project dialog offering
"DLL" would have been offering something that could not be built.

## Decision

A module declares `target console | gui | sharedlib | staticlib`, and
`openepl build --target <kind>` overrides it.

**The target changes the entry contract and nothing else.** Lowering,
type-checking and the component model are identical across targets; that is what
makes shipping a `.so` a build option rather than a rewrite.

* Executables keep `ECodeStart` (console runs `main`; GUI builds the form,
  runs `main`, then the loop).
* Libraries emit **no entry**. Each subroutine gets an exported wrapper under
  its plain name, while the body keeps its `oe_user_` mangled name, so a host
  links against `greet` and internal calls still resolve. Module variables are
  initialised by an exported `<module>_init` rather than an implicit
  constructor — a library should not run user code before its host is ready.

**Omitted, the target is inferred** — a module with a form is `gui`, otherwise
`console`. Every file written before targets existed still means what it meant,
and all existing examples and tests stayed green.

**`target` is a soft keyword**, matched only in the declaration position rather
than reserved in the lexer. Reserving it would steal `target` as a variable and
property name everywhere, which is a poor trade for one declaration.

## Consequences

* **The process-entry object must be dropped for libraries.** `oe_start.c`
  provides `main`, which calls `ECodeStart`; a library has no `ECodeStart`, so
  linking it in leaves an undefined symbol and the `.so` fails to `dlopen` —
  a file with the right extension that cannot be loaded. The file was already
  in its own TU for exactly this reason.
* **`--gc-sections` is for programs only.** A library must keep exports no host
  has linked yet; dead-stripping would drop every one of them. Executables keep
  the dead-strip that is the headline property of the BlackMoon model.
* **`-fPIC` must reach the runtime sources**, not just the generated `.ll`:
  every object entering a `-shared` link has to be position independent. Static
  archives get it too, since they are routinely linked into shared objects.
* **Static archives are built one object at a time.** `clang -c` refuses a
  single `-o` for several inputs, so the archive is assembled with `ar` from
  per-source objects rather than in one link command.
* Targets are verified by *using* the artifact: the `.so` is `dlopen`ed and its
  exports called, the `.a` is linked into a C host and run, and `nm` asserts the
  exports exist and the entry does not. A target that only produces a file with
  the right extension is the "mechanism works, surface broken" trap again.
* Windows/macOS naming and driver targets are still open; extensions are Linux
  conventions for now and land with the rest of Phase 4.
