# Build targets

The same source builds as any of these. It is a build option, not a rewrite:
the language, the type checking and the component model are identical across
all of them. Only the entry contract changes.

| `target` | Produces | Entry |
| --- | --- | --- |
| `console` | a terminal program | `main` |
| `gui` | a windowed program | the form, then `main`, then the event loop |
| `sharedlib` | `.so` (`.dll` + `.lib` on Windows), and a C header | none — subroutines are exported |
| `staticlib` | `.a` archive, and a C header | none — subroutines are exported |

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
so a C host — or anything that can call C — links against it directly, and
the build writes the header that declares them beside the artifact.

```
module greet
target sharedlib

var greetings: int = 0

sub greet
  call print_text("Hello from a shared library.")
end

sub add(a: int, b: int): int
  return a + b
end

sub greeting(name: text): text
  greetings = greetings + 1
  return "Hello, " + name + "!"
end
```

```sh
openepl build greet.oir -o libgreet.so     # writes libgreet.so and greet.h
```

### The header

`greet.h` is generated from the same code the library was lowered from, so
what it declares is what was linked. It is named after the module, not the
output — `-o libgreet.so` still gives `greet.h` — and `--header <path>` puts
it elsewhere. It compiles as C and as C++, and it looks like the header a
Windows DLL author would have written by hand:

```c
#pragma once
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

#if defined(GREET_STATIC)
#  define GREET_API
#elif defined(_WIN32)
#  ifdef GREET_EXPORTS /* defined when building the DLL itself */
#    define GREET_API __declspec(dllexport)
#  else
#    define GREET_API __declspec(dllimport)
#  endif
#else
#  define GREET_API __attribute__((visibility("default")))
#endif

GREET_API void greet_init(void); /* initialises module variables; call once, first */
GREET_API void greet(void);
GREET_API int32_t add(int32_t a, int32_t b);
GREET_API const char *greeting(const char *name);

#ifdef __cplusplus
}
#endif
```

The macro prefix is the module name in capitals: `GREET_API` marks each
export, `GREET_EXPORTS` is what the DLL's own build would define (nothing
you write needs it — the library's objects come from the compiler, not from
this header — so a consumer always sees `dllimport`), and `GREET_STATIC`
switches the imports off. A static library's header defines `GREET_STATIC`
itself, because its symbols are linked in, not imported, and a `dllimport`
on them would send a Windows link looking for an import library that does
not exist.

The types are the ones the exported wrappers actually take:

| OpenEPL | C |
| --- | --- |
| `int` | `int32_t` |
| `int64` | `int64_t` |
| `double` | `double` |
| `bool` | `int32_t` — 0 or 1; never a C `bool`, which is one byte |
| `text` | `const char *` — NUL-terminated, `NULL` means empty |

Text a subroutine returns belongs to the library: copy it if you keep it,
and never free it. A subroutine that takes or returns a byte-set, an array,
a record or a dictionary is a pointer to a runtime-owned object C cannot
build or read, so it gets no prototype; the header lists it in a comment
instead, so the omission is a fact you can see rather than a name that is
simply missing.

`<module>_init` initialises the module's variables. It is exported rather
than run automatically, because a library should not run your code before
the host is ready for it.

### A consumer

`openepl new shared-library` writes a `consumer.cpp` beside the source; this
is it, for a module named `greet`:

```cpp
#include <stdio.h>
#include "greet.h"

#if defined(_WIN32) && defined(_MSC_VER)
#  pragma comment(lib, "greet.lib")
#endif

int main(void) {
    greet_init();                          /* module variables, once, first */
    greet();
    printf("%d\n", (int)add(2, 3));
    printf("%s\n", greeting("world"));      /* the text belongs to the library */
    return 0;
}
```

On Linux, with clang:

```sh
openepl build greet.oir -o libgreet.so
clang++ consumer.cpp -I. -L. -lgreet -Wl,-rpath,. -o consumer && ./consumer
```

The same file compiles as C — `clang -x c consumer.cpp ...` — because the
header only wraps its prototypes in `extern "C"` when a C++ compiler reads
it.

A static library is the same, built as an archive and linked in:

```sh
openepl build greet.oir --target staticlib -o libgreet.a    # and greet.h
clang++ consumer.cpp -I. libgreet.a -lm -o consumer
```

## Building for Windows

A console program or a library cross-builds for Windows x86-64 from Linux:

```sh
openepl build hello.oir --os windows          # hello.exe
openepl build greet.oir --os windows --target sharedlib   # greet.dll
openepl build greet.oir --os windows --target staticlib   # libgreet.a
```

A shared library built for Windows comes as three files: `greet.dll`,
`greet.h`, and the import library `greet.lib` that a Windows link goes
through — the one the consumer's `#pragma comment(lib, "greet.lib")` names.
The consumer above builds against them with either Windows toolchain:

```sh
openepl build greet.oir --os windows -o greet.dll     # greet.dll, greet.lib, greet.h
cl /EHsc consumer.cpp greet.lib                        # MSVC, x64
x86_64-w64-mingw32-g++ consumer.cpp -L. -lgreet -o consumer.exe   # MinGW
```

The import library is written by mingw's linker from the DLL's export table.
It is an ordinary COFF import archive, the format `link.exe` accepts for
plain C names — the exports carry no decoration to disagree about. It is
produced and checked on Linux, not under MSVC; should a Windows link reject
it, `x86_64-w64-mingw32-dlltool -l greet.lib -d greet.def` writes the
short-import form from the same export list. A static
library cross-built for Windows is a mingw archive, and links with mingw —
`x86_64-w64-mingw32-g++ consumer.cpp libgreet.a -lws2_32` — rather than with
MSVC, whose linker wants an archive its own toolchain produced.

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
its `.exe` added — Windows will not run a file without one. A library's
header is `<module>.h` beside it whatever the artifact was called, and a
Windows DLL's import library takes the DLL's name with `.lib` in place of
`.dll`.

## What a library may not do

A library cannot declare a form — a window belongs to a program. Building one
that does is an error that says so, as is a library that exports nothing.
