# Limitations

OpenEPL is young. This page is the honest list, kept current against the
toolchain — it is more useful than discovering these one at a time, and a
limitation that has since been solved is removed rather than left to teach
you to ignore the page.

## Platforms

**Linux on x86-64, plus a cross build for Windows.** Programs — windowed
and console — and libraries cross-build for Windows x86-64 from Linux with
`--os windows` (see [Build targets](./build-targets.md)). A console program
is tested under wine and runs; a windowed one is tested under wine as far as
a machine without a display allows — it loads with its DLLs and reaches the
UI library — and its drawn window has not been checked under wine or on a
Windows machine. Accessibility is off in a Windows build: the a11y bridge
has no AccessKit Windows adapter yet. Studio itself is Linux-only, nothing is
built natively *on* Windows — the toolchain runs on Linux — and macOS and
arm64 are not supported at all.

**The `win` kit is declarations, and it is Windows-only.** `use win` brings in
the Win32 API — user32, gdi32, kernel32 and advapi32 — and a program that uses
it builds only with `--os windows`; on any other target it is refused by name.
It is written and tested on Linux: the examples in `examples/win/` cross-build
with mingw and run under wine, which is where a wrong struct offset or a
misspelled export shows up. Under wine the display is off, so a window is
created and its WNDPROC really is called back with `WM_PAINT` — that is
checked — but there is no framebuffer to read: **the drawn window has not been
looked at**, under wine or on a Windows machine. The kit binds the ANSI
(`...A`) entry points only, so a string outside the process's code page does
not survive; a program written against it alone builds for the **console**
subsystem, because `--target gui` is the UI stack rather than a subsystem
switch and refuses a module with no form — so a Win32 window made this way has
a console window beside it; it has no COM, and a struct with a union, a
bitfield or non-natural packing — `BITMAPFILEHEADER`, say — has no c-record
and must be laid out by hand.

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
- Local variables are visible for the whole subroutine, not just the block
  they were declared in. A `for` loop's variable follows the same rule, so two
  loops in one subroutine need two different variable names.
- A `ptr` that holds a function's address cannot be called. `address of`
  makes one and a `dll` can take one, but there is no indirect call, so the
  pointer `GetProcAddress` answers can be stored and passed on and not
  invoked — and COM, whose every method call goes through a vtable, is out of
  reach for the same reason.
- No bitwise operators and no bitwise commands: there is no `and`, `or`,
  `xor`, `not` or shift on an integer. Combining flags works because flag bits
  are disjoint, so `PROCESS_VM_READ + PROCESS_VM_WRITE` is the number C's `|`
  gives — but testing one bit out of a word has no spelling beyond arithmetic
  with `%` and `/`.
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
console programs, servers and libraries, on Linux, today — and the same
program or library cross-built for Windows. Everything above is what is
missing from that, not a warning that the core does not function.
