# Limitations

OpenEPL is young. This page is the honest list, kept current against the
toolchain — it is more useful than discovering these one at a time, and a
limitation that has since been solved is removed rather than left to teach
you to ignore the page.

## Platforms

**Linux on x86-64 only.** Windows, macOS and arm64 are not supported yet. The
compiler is built to retarget, the support libraries compile under mingw-w64,
and the artifact names already follow each platform's convention — but no
other platform is tested or shipped.

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

**TLS is opt-in, and only on the client side.** `net_http_get` speaks
`https://` once `tools/fetch-mbedtls.sh` has vendored mbedTLS; without it the
call fails rather than downgrading. The `httpserver` component never terminates
TLS — put a reverse proxy in front of it. See [Networking](./networking.md).

**Memory is reclaimed at exit.** Every value the runtime allocates — a text
result, an array, a record, a dictionary — lives until the program ends, and is
freed then. A program that runs and stops never notices; a window that runs all
day, or a server that answers requests for weeks, grows with the work it has
done. Restart it, or put it behind something that will.

**No debugger.** You get a program's output and its exit code in the console,
not breakpoints, stepping or variable inspection.

## The language

- No user-defined types beyond `record`: there is no enum, no interface and
  no way to attach a subroutine to a type.
- A dictionary's keys are text, and only text; asking with an `int` is a
  compile error.
- No named constants: a module-level `let` does not parse, only `var`.
- Local variables are visible for the whole subroutine, not just the block
  they were declared in. A `for` loop's variable follows the same rule, so two
  loops in one subroutine need two different variable names.
- No mixing of numeric types in one expression: `d + 1` where `d` is a
  `double` is an error, not an implicit conversion. Write
  `d + int_to_double(1)`.
- No `for` over a collection. Loop `1` to `count(xs)` and index; a
  dictionary is walked through `dict_keys`.
- One form per module.
- A form or component property value must be a literal. A component's
  properties can be set again from code, but the form's own cannot — the form
  is not a component — so a window title cannot be computed or translated at
  start-up.
- A support library cannot take or return a record or a dictionary. The ABI
  has a tag for each, and core's `dict_*` commands use it, but the layouts are
  the runtime's own; a library sees `int`, `int64`, `double`, `bool`, `text`,
  arrays and `bytes`.

## The component library

The components each library provides are listed in
[Components](./reference-components.md), and what they are for is in
[Components](./components.md). The designer's toolbox lists four more greyed
out — a tab control, a splitter, a file dialog and a tray icon: they are part
of the intended shape but are not implemented, and adding one tells you so
rather than placing something that does not work.

The `form` declares a `load` event, and a handler bound to it compiles — and
is never called. Nothing dispatches it yet. Put start-up work in `main`, which
runs after the window and its components exist and before the event loop
starts.

A `grid` shows what a `datasource` holds, and a datasource holds text —
rows separated by newlines, cells by tabs. There is no database kit yet to
fill one from a query.

## The IDE

- The console shows the tail of the output rather than a full scrollback.
- No project-wide search, no refactoring, no version-control integration.
- Completion, hover and go-to-definition work in external editors through the
  language server; inside Studio, the editor has highlighting and diagnostics
  so far.

## What does work

Design a form, wire an event, build it, run it, and ship the binary — plus
console programs, servers and libraries, on Linux, today. Everything above is
what is missing from that, not a warning that the core does not function.
