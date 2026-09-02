# Installation

## Download a release

Releases are self-contained: unpack one anywhere and run it. There is no
installer, nothing to add to `PATH`, and no configuration.

```sh
tar xzf openepl-<version>-linux-x86_64.tar.gz
cd openepl-<version>-linux-x86_64
bin/openepl-studio
```

The binaries find everything they need relative to themselves, so you can move
the folder wherever you like.

## What you need installed

| For | Requirement |
| --- | --- |
| Building any program | `clang`, `ar` |
| Windowed programs and the IDE | `pkg-config`, SDL2, SDL2_image, FreeType, OpenGL |

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
```

## Platform support

Linux on x86-64. Windows, macOS and arm64 are not supported yet.
