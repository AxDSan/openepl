# Build targets

The same source builds as any of these. It is a build option, not a rewrite:
the language, the type checking and the component model are identical across
all of them. Only the entry contract changes.

| `target` | Produces | Entry |
| --- | --- | --- |
| `console` | a terminal program | `main` |
| `gui` | a windowed program | the form, then `main`, then the event loop |
| `sharedlib` | `.so` (`.dll` on Windows) | none — subroutines are exported |
| `staticlib` | `.a` archive | none — subroutines are exported |

Declare it in the module:

```
module greet
target sharedlib

sub greet
  call print_text("Hello from a shared library.")
end
```

…or override it for one build, without touching the source:

```sh
openepl build greet.oir --target sharedlib -o libgreet.so
```

Left out entirely, the target is inferred: a module with a form is `gui`,
anything else is `console`.

## Programs

Console and windowed programs are ordinary executables. The linker drops every
command the program never calls, so a small program stays small, and there is
nothing to unpack at start-up.

## Release builds

Every build is a debug build unless you ask otherwise. `--release` is the other
profile, on `build` and on `run`:

```sh
openepl build hello.oir --release -o hello
```

It compiles at `-O2` and adds the hardening a shipped program wants: `_FORTIFY_SOURCE`,
`-fstack-protector-strong`, position-independent code, read-only relocations
with every symbol bound at load time, and no symbol table — the names of your
subroutines, module variables and the runtime commands you linked are gone from
the file. Dead-stripping still applies, so the program is smaller than the debug
one as well as faster.

Each flag is offered to the local `clang` before it is used, and one the
compiler will not take is dropped with a line saying so. A flag accepted in
silence and ignored would leave you believing in hardening that is not there.

The default stays a debug build because most builds are the one you are about to
run and delete, and hardening costs compile time. Nothing else changes: the same
source, the same output, the same behaviour.

A windowed program is the one exception to position independence — the vendored
UI stack is not built `-fPIC` — and the build tells you when it links without it.

## Libraries

A library has no entry point. Every subroutine is exported under its own name,
so a C host — or anything that can call C — links against it directly.

```
module greet
target sharedlib

sub greet
  call print_text("Hello from a shared library.")
end
```

```sh
openepl build greet.oir -o libgreet.so
```

```c
/* host.c */
void greet_init(void);   /* initialises module variables */
void greet(void);

int main(void) {
    greet_init();
    greet();
    return 0;
}
```

```sh
clang host.c ./libgreet.so -lm -o host && ./host
```

`<module>_init` initialises the module's variables. It is exported rather than
run automatically, because a library should not run your code before the host
is ready for it.

A static library is the same, built as an archive:

```sh
openepl build greet.oir --target staticlib -o libgreet.a
clang host.c libgreet.a -lm -o host
```

## Building for Windows

A console program or a library cross-builds for Windows x86-64 from Linux:

```sh
openepl build hello.oir --os windows          # hello.exe
openepl build greet.oir --os windows --target sharedlib   # greet.dll
openepl build greet.oir --os windows --target staticlib   # libgreet.a
```

`--os linux` is the default and means the machine you are on. `--os windows`
needs the mingw-w64 cross compiler on the build machine — `mingw64-gcc` on
Fedora, `gcc-mingw-w64-x86-64` on Debian and Ubuntu — and the build says so in
one line when it is missing. The IR still goes through `clang`, retargeted;
mingw's `gcc` does the link.

What comes out is the same program: the same source, the same commands, the
same dead-stripping. `--release` applies too, with the hardening PE has in
place of what ELF has — ASLR (`--dynamicbase`, `--high-entropy-va`) and DEP
(`--nxcompat`) stand in for PIE and RELRO, and the symbol table is stripped
the same way.

The limits, stated plainly:

- **`gui` is not available for Windows yet.** The UI stack is vendored for
  Linux only, and a form has nothing to link against; the build refuses
  rather than producing a program that cannot start.
- **Nothing is built natively on Windows yet.** This is a cross build from
  Linux; there is no Windows build of the toolchain or of Studio.
- **`https://` is off in a Windows build.** The vendored mbedTLS was built
  for Linux, so a cross build leaves it out and `net_http_get` says so at run
  time; `http://` works.
- `openepl run --os windows` refuses: the machine you are on cannot run the
  result. Run it under `wine`, or on Windows.

## Naming

Without `-o`, an output is named after the source: a program takes the file's
stem, and libraries follow the platform convention (`libgreet.so`,
`libgreet.a`) so a linker finds them by the name it expects. Built for
Windows, a program is `hello.exe`, a shared library `greet.dll`, and a static
library keeps mingw's `libgreet.a`. A Windows program given `-o hello` gets
its `.exe` added — Windows will not run a file without one.

## What a library may not do

A library cannot declare a form — a window belongs to a program. Building one
that does is an error that says so, as is a library that exports nothing.
