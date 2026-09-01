# OpenEPL __VERSION__

An open implementation of **Easy Programming Language** (易语言, EPL): a
cross-platform RAD environment where you draw a form, wire an event, and get a
clean native binary. English-first, and a fresh implementation rather than a
clone — it does not run existing EPL programs.

    bin/openepl-studio          the IDE — run it with no arguments
    bin/openepl                 the command-line toolchain

Nothing here needs installing. The bundle is relocatable: move it anywhere and
run the binaries from `bin/`. They locate the runtime and templates relative to
themselves, not through environment variables.

## Requirements

| For | You need |
| --- | --- |
| Any build | `clang`, `ar` |
| GUI programs and the IDE | `pkg-config`, SDL2, SDL2_image, FreeType, OpenGL |

The runtime and support libraries ship as **source** and are compiled into each
program you build. That is what lets the linker drop every command your program
does not call, so a hello-world stays small and contains no unpacking stub.

Fedora: `sudo dnf install clang binutils pkgconf-pkg-config SDL2-devel SDL2_image-devel freetype-devel`
Debian/Ubuntu: `sudo apt install clang binutils pkg-config libsdl2-dev libsdl2-image-dev libfreetype-dev`

## Start here

    bin/openepl-studio

Studio opens a welcome screen: pick a project kind, and it is created and opened
for you. Drag components onto the form, set properties in the inspector, wire a
button's click to a subroutine, and press **Run**.

From the command line:

    bin/openepl templates                    # what you can create
    bin/openepl new gui-app my-app           # create a project
    bin/openepl run my-app/main.oir          # build and run it

## Build targets

One source builds as any of these — a choice, not a rewrite:

| `target` | Produces |
| --- | --- |
| `console` | a terminal program |
| `gui` | a windowed program |
| `sharedlib` | `.so` (`.dll` / `.dylib` on other platforms) |
| `staticlib` | `.a` archive |

Declare it in the module (`target sharedlib`) or override it per build
(`--target sharedlib`). Omitted, a module with a form is a GUI program and
anything else is a console one.

## Editing

`bin/openepl lsp` is a language server: diagnostics as you type, completion,
hover, go-to-definition and find-references. Point any LSP-capable editor at it
— `docs/editors.md` has ready-made configuration for Neovim, VS Code, Helix and
Zed, and `editors/vscode/` is a working extension with syntax highlighting.

## What this is not, yet

* **Linux x86-64 only.** Windows, macOS and arm64 are not supported yet.
* **A release build is hardened, not hidden.** `openepl build --release`
  optimises, hardens and strips what you build. A native binary can still be
  disassembled — the flag buys a smaller, harder-to-attack program, not secrecy.
* **No debugger.** You get a program's output and its exit code in the IDE
  console, not breakpoints or stepping.

The project's design notes, specifications and decision log live in the source
repository, not in this download.

Licensed MIT OR BSD-3-Clause. See `LICENSE` and `THIRD-PARTY.md`.
