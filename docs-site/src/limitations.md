# Limitations

OpenEPL is young. This page is the honest list, kept current — it is more
useful than discovering these one at a time.

## Platforms

**Linux on x86-64 only.** Windows, macOS and arm64 are not supported yet. The
compiler is built to retarget, and the artifact names already follow each
platform's convention, but no other platform is tested or shipped.

## Programs you build

**A release build is hardened, not hidden.** `openepl build --release`
compiles at `-O2` with `_FORTIFY_SOURCE`, a stack protector, full RELRO with
symbols bound at load time, position independence, and no symbol table — see
[Build targets](./build-targets.md). That is a smaller, faster, harder-to-attack
program; it is not obfuscation. A native binary can always be disassembled, so
do not rely on one to keep an algorithm secret.

**A windowed release build is not position independent.** The vendored UI stack
is compiled without `-fPIC` and cannot go into a PIE, so a `gui` release says so
and links without it. Console programs and libraries are unaffected.

**No TLS, in either direction.** `net_http_get` refuses an `https://` URL
rather than downgrading it, and the `httpserver` component does not terminate
TLS — put a reverse proxy in front of it. See [Networking](./networking.md).

**No debugger.** You get a program's output and its exit code in the console,
not breakpoints, stepping or variable inspection.

## The language

- No user-defined types beyond `record`: there is no enum, no interface and
  no way to attach a subroutine to a type.
- A dictionary's keys are text, and only text.
- One form per module.
- Local variables are visible for the whole subroutine, not just the block
  they were declared in. A `for` loop's variable follows the same rule, so two
  loops in one subroutine need two different variable names.
- No named constants: a module-level `let` does not parse, only `var`.
- No mixing of numeric types in one expression: `d + 1` where `d` is a
  `double` is an error, not an implicit conversion. Write
  `d + int_to_double(1)`.
- A form or component property value must be a literal, so a window title
  cannot be computed or translated at start-up.
- Memory taken by text results — `concat`, `int_to_text`, `substr`,
  `file_read_text` — is reclaimed when the program exits, not before, so a
  window that runs all day grows.

## The component library

The components each library provides are listed in
[Components](./reference-components.md). The designer's toolbox
lists a few more (list boxes, tab controls) greyed out: they are part of the
intended shape but are not implemented, and adding one tells you so rather
than placing something that does not work.

## The IDE

- The console shows recent output rather than a full scrollback.
- No project-wide search, no refactoring, no version-control integration.
- Completion, hover and go-to-definition work in external editors through the
  language server; inside Studio, the editor has highlighting and diagnostics
  so far.

## What does work

Design a form, wire an event, build it, run it, and ship the binary — plus
console programs and libraries, on Linux, today. Everything above is what is
missing from that, not a warning that the core does not function.
