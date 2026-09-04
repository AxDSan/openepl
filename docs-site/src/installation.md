# Installation

## Download a release

Releases are self-contained: unpack one anywhere and run it. There is no
installer, nothing to add to `PATH`, and no configuration.

```sh
tar xzf openepl-<version>-linux-x86_64.tar.gz
cd openepl-<version>-linux-x86_64
bin/openepl-studio
```

On Windows, unpack the `windows-x86_64` zip and run `bin\openepl-studio.exe`.
The bundle carries Studio, the compiler, the runtime and the kits; what it
cannot carry is a linker, so install LLVM first —
[Setting up on Windows](./setup-windows.md) walks through it (it is
`SETUP-WINDOWS.md` beside the binaries too), and the one step most first runs
miss is ticking **Add LLVM to the system PATH**.

The binaries find everything they need relative to themselves, so you can move
the folder wherever you like.

## What you need installed

| For | Requirement |
| --- | --- |
| Building any program | `clang`, `ar` |
| Windowed programs and the IDE | `pkg-config`, SDL2, SDL2_image, FreeType, OpenGL |
| Cross-building for Windows | `mingw-w64` — `mingw64-gcc` on Fedora, `gcc-mingw-w64-x86-64` on Debian and Ubuntu |

The runtime ships as source and is compiled into each program you build. That
is what lets the linker drop every command your program never calls.

**Fedora**

```sh
sudo dnf install clang binutils pkgconf-pkg-config \
                 SDL2-devel SDL2_image-devel freetype-devel
```

**Debian / Ubuntu**

```sh
sudo apt install clang binutils pkg-config \
                 libsdl2-dev libsdl2-image-dev libfreetype-dev
```

## Building from source

You will also need a Rust toolchain.

```sh
git clone https://github.com/AxDSan/openepl
cd openepl

tools/fetch-rmlui.sh          # vendor the UI library
tools/fetch-accesskit.sh      # vendor the accessibility bridge

cargo build --release         # the compiler
designer/build.sh             # the IDE
cargo test                    # the test suite
```

The compiler is then `target/release/openepl` and the IDE is
`designer/openepl-designer`.

`https://` is opt-in: it needs mbedTLS vendored, and everything else builds
without it.

```sh
tools/fetch-mbedtls.sh        # once; then net_http_get speaks https
```

See [Networking](./networking.md) for what that buys and what it never does.

To produce a release bundle of your own:

```sh
tools/package.sh                                # -> dist/
tools/verify-bundle.sh dist/openepl-*.tar.gz    # prove it works unpacked elsewhere
tools/package-windows.sh                        # -> the Windows bundle, cross-built
```

## Platform support

**Linux on x86-64**, natively: the toolchain and Studio are built and run
there, and that is where OpenEPL is developed.

**Windows on x86-64**, cross-built from Linux. Programs — windowed and
console — and libraries build with `--os windows`, and
`tools/package-windows.sh` produces a bundle carrying `openepl.exe` and
`openepl-studio.exe`. Nothing is built natively *on* Windows: the toolchain
that produces the bundle runs on Linux. See
[Build targets](./build-targets.md#building-for-windows) for what a Windows
build does and does not include, and [Limitations](./limitations.md) for what
has and has not been checked on a Windows machine.

macOS and arm64 are not supported.
