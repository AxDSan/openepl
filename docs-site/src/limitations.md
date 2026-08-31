# Limitations

OpenEPL is young. This page is the honest list, kept current — it is more
useful than discovering these one at a time.

## Platforms

**Linux on x86-64 only.** Windows, macOS and arm64 are not supported yet. The
compiler is built to retarget, and the artifact names already follow each
platform's convention, but no other platform is tested or shipped.

## Programs you build

**No hardened release profile.** Programs are ordinary native binaries. The
stripped, hard-to-decompile release output is not implemented yet, so do not
rely on a built program to keep its source shape private.

**No debugger.** You get a program's output and its exit code in the console,
not breakpoints, stepping or variable inspection.

## The language

- Subroutines take no parameters, return nothing, and cannot be called from
  other code — they are entry points (`main`) and event handlers. Handlers
  share work through module variables.
- No user-defined types, arrays or collections.
- One form per module.
- Local variables are visible for the whole subroutine, not just the block
  they were declared in.

## The component library

Eight components — see [Components](./reference-components.md). The toolbox
lists a few more (list boxes, tab controls, timers) greyed out: they are part
of the intended shape but are not implemented, and adding one tells you so
rather than placing something that does not work.

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
