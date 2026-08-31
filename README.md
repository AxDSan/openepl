<div align="center">

<img src="assets/openepl-wordmark.png" alt="OpenEPL" width="440">

**Draw an app. Wire an event. Ship a native binary.**

A cross-platform, visual-first development environment — with a compiler that
produces clean native executables, no runtime to install and nothing to unpack.

**[Documentation](https://axdsan.github.io/openepl/)** · [Quick start](#quick-start) ·
[Build targets](#one-project-every-artifact) · [Editor support](#editor-support) ·
[Building from source](#building-from-source) · [Status](#status)

</div>

---

## What it is

OpenEPL is an IDE and a compiler that belong together. You lay a form out
visually, set properties in an inspector, wire a button's click to a
subroutine, and press **Run** — and what comes out the other side is an
ordinary native binary you can hand to someone.

The language is English-first and deliberately small: one uniform call syntax,
no pointers, no manual memory management, no ceremony. Assignment is a
statement rather than an expression, so `if x = 5` cannot silently assign.

<div align="center">
<img src="assets/screenshot-designer.png" alt="The OpenEPL Studio visual designer" width="860">
</div>

## Quick start

Download a release, unpack it anywhere, and run it — there is no installer and
nothing to configure:

```sh
tar xzf openepl-0.1.0-linux-x86_64.tar.gz
cd openepl-0.1.0-linux-x86_64
bin/openepl-studio
```

Studio opens on a welcome screen. Pick a project kind and it is created and
opened for you.

<div align="center">
<img src="assets/screenshot-welcome.png" alt="Choosing a project template" width="760">
</div>

From the command line:

```sh
bin/openepl templates                 # what you can create
bin/openepl new gui-app my-app        # create a project
bin/openepl run my-app/main.oir       # build it and run it
```

A first program is as short as it looks:

```
module hello
target console

sub main
  call print_text("Hello from OpenEPL.")

  let answer: int = 6 * 7
  call print_text(concat("six times seven is ", int_to_text(answer)))
end
```

## One project, every artifact

The same source builds as any of these. It is a build option, not a rewrite:

| `target` | Produces |
| --- | --- |
| `console` | a terminal program |
| `gui` | a windowed program |
| `sharedlib` | `.so` (`.dll` / `.dylib` elsewhere) |
| `staticlib` | `.a` archive |

Declare it in the module, or override it per build with `--target`. Left out,
a module with a form is a GUI program and anything else is a console one.

```sh
openepl build lib.oir --target sharedlib -o libgreet.so
```

Libraries export their subroutines under their own names, so a C host — or
anything that can call C — links against them directly.

## Editing

Studio's editor gives you syntax highlighting and live diagnostics as you
type, backed by the same language server any other editor can use.

<div align="center">
<img src="assets/screenshot-editor.png" alt="The code editor with syntax highlighting" width="860">
</div>

### Editor support

`openepl lsp` is a Language Server Protocol server: diagnostics, completion,
hover, go-to-definition and find-references. Point any LSP-capable editor at
it — [`docs/editors.md`](docs/editors.md) has ready-made configuration for
Neovim, VS Code, Helix and Zed, and [`editors/vscode/`](editors/vscode) is a
working extension with syntax highlighting.

## How it builds

Your project is compiled to LLVM IR, assembled, and linked with the system
linker against a runtime that ships as source and is compiled into your
program. The linker then drops every command your program never calls.

The result is a single ordinary executable. Nothing is unpacked at startup, no
support libraries are loaded at run time, and there is no interpreter inside —
which keeps programs small, quick to start, and unremarkable to antivirus
software.

## Building from source

You need a Rust toolchain, `clang`, and — for GUI programs and the IDE —
`pkg-config`, SDL2, SDL2_image and FreeType.

```sh
tools/fetch-rmlui.sh          # vendor the UI library
tools/fetch-accesskit.sh      # vendor the accessibility bridge
cargo build --release         # the compiler
designer/build.sh             # the IDE
cargo test                    # the test suite
```

To produce a release bundle of your own:

```sh
tools/package.sh                                    # -> dist/
tools/verify-bundle.sh dist/openepl-*.tar.gz        # prove it works unpacked elsewhere
```

## Accessibility

The component model carries an accessibility role and name for every control,
and Studio publishes a live accessibility tree. This is part of the component
model rather than something added later.

## Status

OpenEPL is young, and honest about it:

- **Linux x86-64 only.** Windows, macOS and arm64 are not supported yet.
- **No hardened release profile yet.** Programs you build are ordinary native
  binaries; stripped, non-decompilable output is still to come.
- **No debugger.** You get a program's output and its exit code in the IDE
  console, not breakpoints or stepping.
- The component library is small, and the language is still growing.

What does work, end to end: design a form, wire an event, build it, run it,
and ship the binary — on Linux, today.

## Documentation

Full documentation — installation, a language guide, the visual designer, and
generated references for every command and component — is at
**[axdsan.github.io/openepl](https://axdsan.github.io/openepl/)**.

It is built from `docs-site/`. To work on it locally:

```sh
cargo install mdbook
tools/gen-docs.sh          # regenerate the reference pages from the toolchain
mdbook serve docs-site     # http://localhost:3000
```

## Licence

MIT OR BSD-3-Clause, at your option. See [`LICENSE`](LICENSE), and
[`THIRD-PARTY.md`](THIRD-PARTY.md) for the components OpenEPL bundles.
