<div align="center">
  <img src="./assets/openepl-wordmark.png" alt="OpenEPL" width="380">
</div>

# Introduction

OpenEPL is an IDE and a compiler that belong together. You lay a window out
visually, set properties in an inspector, wire a button's click to a
subroutine, and press **Run** — and what comes out is an ordinary native
binary you can hand to someone.

![The OpenEPL Studio visual designer](./assets/screenshot-designer.png)

The language is English-first and deliberately small. There is one way to call
things, no pointers, no manual memory management, and no ceremony:

```
module hello
target console

sub main
  call print_text("Hello from OpenEPL.")

  let answer: int = 6 * 7
  call print_text(concat("six times seven is ", int_to_text(answer)))
end
```

Assignment is a statement rather than an expression, so `if x = 5` compares —
it cannot silently assign.

## What makes it different

**The visual designer is not a separate tool.** The form you draw and the file
you edit are the same thing. Drag a button in the designer and the source
changes; edit the source and the canvas follows.

**Programs are ordinary native binaries.** Your project is compiled to machine
code and linked with the system linker. Nothing is unpacked at startup, no
support libraries are loaded at run time, and there is no interpreter inside.
Programs stay small, start immediately, and look unremarkable to antivirus
software.

**One project builds every artifact.** The same source can become a console
program, a windowed program, a shared library or a static library — a build
option rather than a rewrite. See [Build targets](./build-targets.md).

## Where to start

- [Installation](./installation.md) — download or build it
- [Quick start](./quick-start.md) — a program in about a minute
- [Your first GUI app](./first-gui-app.md) — draw a window and wire a button

## What is not here yet

OpenEPL is young. [Limitations](./limitations.md) lists what does not exist
yet, plainly — it is worth reading before you plan anything around it.
