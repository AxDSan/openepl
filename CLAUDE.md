# OpenEPL — working notes for contributors

OpenEPL is an open implementation of Easy Programming Language (易语言, EPL): a
RAD environment where you draw a form, wire its events, and compile to a native
binary. English-first, cross-platform, open source.

## Layout

```
ir/         parser, type checker, validator          (Rust)
backend/    lowering to LLVM IR                      (Rust)
cli/        the `openepl` binary, language server    (Rust)
runtime/    the core runtime and its commands        (C)
libs/       the bundled kits — `use <name>`          (C / C++)
abi/        the C ABI shared by all of the above
designer/   OpenEPL Studio, the IDE                  (C++ / RmlUi)
templates/  project templates for `openepl new`
kits/       a project-tier kit (`units`), the worked example of one
editors/    the VS Code extension
docs-site/  the documentation site (mdBook + landing page)
docs/       what the release bundle ships beside the binaries
tools/      fetch, package, and check scripts
```

## Building

```sh
tools/fetch-rmlui.sh && tools/fetch-accesskit.sh   # vendored UI dependencies
cargo build --release        # the compiler
designer/build.sh            # the IDE
cargo test                   # 260+ tests
designer/build.sh test       # the designer's own tests
tools/fetch-mbedtls.sh       # optional: https in `net`
```

## Conventions that matter

- **The CLI is the only reader of a project file.** The designer and the
  documentation call `openepl inspect`, `openepl commands` and
  `openepl templates` rather than parsing `.oir` themselves. Two parsers would
  drift.
- **Reference documentation is generated**, never hand-written:
  `tools/gen-docs.sh` builds it from the toolchain, and `tools/check-docs.sh`
  compiles every sample in the docs — the landing page's included — and holds
  the landing page's kit, command and component counts to what the toolchain
  reports. `docs-site/src/limitations.md` is checked against the toolchain the
  same way, by hand: a limitation that has been solved is removed, not left.
- **Two pages have one source.** `docs/editors.md` ships in the bundle and is
  what the README links; `docs/SETUP-WINDOWS.md` ships at the root of the
  Windows bundle. `docs-site/src/editors.md` and `docs-site/src/setup-windows.md`
  are mdBook includes of them. Edit the ones in `docs/`.
- **Verify UI work by rendering it.** Studio bugs pass tests and fail on screen.
  `OPENEPL_DESIGNER_DUMP=x.ppm OPENEPL_DESIGNER_SCRIPT='view:code' ...` writes a
  frame you can look at; the scripted verbs are in `designer/main.cpp`.
  A headless run (those variables, or `OPENEPL_UI_EXIT_AFTER_FRAMES` /
  `OPENEPL_UI_DUMP` for a built app) defaults to `SDL_VIDEODRIVER=offscreen`, so
  it opens no window and steals no focus. Set `SDL_VIDEODRIVER` yourself to
  override. Never run a UI test or a scripted session without one of them.
- **Never open a tracked example in Studio.** It saves on exit, and the change
  lands in your next commit.
- **One word: kit.** Whether it sits in `libs/` (bundled), `~/.openepl/kits/`
  (installed) or a project's `kits/`, it is a kit. "Library" means only what
  `target sharedlib` and `target staticlib` build.
- Kits are plain C: add `libs/<name>/<name>_libinfo.c` (metadata) and
  `<name>_cmds.c` (implementations), and `use <name>` finds it — there is no
  registration list.

## Adding a command

Declare it in the library's `_libinfo.c` table (name, symbol, return type,
parameter types), implement it against the slot ABI in `abi/openepl_abi.h`, and
it appears in `openepl commands`, in the language server's completion, and in
the generated reference.
