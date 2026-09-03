# Interop

OpenEPL talks to C in both directions: a program calls into native libraries,
and native code calls back into it. A shared library OpenEPL builds can go one
step further and carry a loader hook — a real `DllMain` on Windows, an ELF
constructor on Linux — so it runs the instant it is mapped into a process. This
page collects the pieces that reach across that boundary, starting with the raw
pointer everything else is built on and ending with a library that hooks a
function the moment it loads.

## Pointers and memory

A `ptr` holds a raw machine address: a 64-bit number that points at some bytes.
It is how a program hands a buffer, a struct, a handle or an out-parameter to a
C API, and how it reads one back. A `ptr` is deliberately opaque — it has no
length, no ownership, and no automatic conversion to or from `int64`. You move
bytes through it explicitly, or you do not move them at all.

`ptr_null()` is the zero address, and `ptr_is_null` tests for it:

```openepl
module nullcheck

sub main
  var p: ptr = ptr_null()
  if ptr_is_null(p)
    call print_text("nothing here yet")
  end
end
```

### Allocating a buffer

`mem_alloc` returns a block of raw bytes you own; `mem_free` releases it. These
are `malloc` and `free` — the block is yours to free, it is not swept up when
the program ends, and it is the block a C API can safely `free` or `realloc`
itself. `mem_zero` and `mem_copy` are `memset` and `memcpy`.

Write typed values into the block at a byte offset and read them back. Offsets
and sizes are `int64`, so a buffer may be larger than two gigabytes:

```openepl
module buffer

sub main
  var buf: ptr = mem_alloc(16)
  call mem_zero(buf, 16)

  call ptr_write_int(buf, 0, 42)
  call ptr_write_int64(buf, 8, 9000000000)

  call print_int(ptr_read_int(buf, 0))
  call print_int64(ptr_read_int64(buf, 8))

  call mem_free(buf)
end
```

The read and write families cover the widths a C API expects:

| Read | Write | Moves |
|------|-------|-------|
| `ptr_read_int` | `ptr_write_int` | a 32-bit int |
| `ptr_read_int64` | `ptr_write_int64` | a 64-bit int |
| `ptr_read_byte` | `ptr_write_byte` | one byte, as an int `0`..`255` |
| `ptr_read_double` | `ptr_write_double` | a 64-bit float |
| `ptr_read_ptr` | `ptr_write_ptr` | one pointer — for a pointer-to-pointer |

`ptr_offset(p, bytes)` makes a new pointer that many bytes past `p`, for when
it reads better than passing an offset to every call.

### Strings across the boundary

`ptr_of_text` hands back the `char*` behind a text, to pass to a C function that
takes a string. It is borrowed: valid only while that text is, and pointing at
read-only bytes when the text is a literal. `ptr_read_text` does the reverse,
copying a NUL-terminated C string at an address into an OpenEPL text you own:

```openepl
module strings

sub main
  let greeting: text = "hello, C"
  var borrowed: ptr = ptr_of_text(greeting)
  call print_text(ptr_read_text(borrowed))

  var owned: ptr = mem_alloc(32)
  call ptr_write_text(owned, 0, "OpenEPL")
  call print_text(ptr_read_text(owned))
  call mem_free(owned)
end
```

`ptr_read_text` is the one read that is defined at a null address: it answers
the empty text rather than dereferencing, because the result is copied out
anyway. Every other read at a bad address faults, exactly as the same C would.

### The escape hatch

When you already hold an address as a number — one a C API returned through an
out-parameter, say — `ptr_from_int` turns it into a `ptr`, and `ptr_to_int`
turns a `ptr` back into an `int64`. This is the only bridge between the two, and
it is spelled out on purpose: an address is not an integer you can accidentally
do arithmetic on.

```openepl
module escape

sub main
  var block: ptr = mem_alloc(8)
  call ptr_write_int(block, 0, 7)

  var address: int64 = ptr_to_int(block)
  var same: ptr = ptr_from_int(address)

  call print_int(ptr_read_int(same, 0))
  call mem_free(block)
end
```

Two `ptr` values that hold the same address compare equal with `=`, so
`same = block` above is true.

## Calling a DLL

A `dll` declaration names a function that lives in a shared library — a `.dll`
on Windows, a `.so` on Linux, a `.dylib` on macOS — and makes it callable like
any subroutine. It is how a program reaches the Win32 API, a Lua C API, Detours,
or any C export the language does not wrap.

The declaration sits at module level, beside `sub`:

```openepl
module messagebox

dll MessageBoxA(handle: ptr, text: text, caption: text, kind: int): int from "user32"

sub main
  let clicked: int = MessageBoxA(ptr_null(), "Built with OpenEPL.", "Hello", 0)
  call print_int(clicked)
end
```

`MessageBoxA` is now called exactly as a subroutine is — `call MessageBoxA(...)`
for its effect, or `MessageBoxA(...)` in an expression for its result — and the
call is checked for arity and type at build time.

### The shape of a declaration

```text
dll NAME(param: type, ...): return-type from "library" as "symbol"
```

- The parameter list and the `: return-type` are the same as a subroutine's. A
  `dll` with no return type is call-only, like a `sub` without one.
- `from "library"` says where the symbol lives. A bare name is decorated for the
  platform — `from "mathdll"` looks for `libmathdll.so` (then `mathdll.so`) on
  Linux, `mathdll.dll` on Windows, `libmathdll.dylib` on macOS — and is searched
  beside the program first, then on the system path. A name with an extension or
  a slash (`"./plugins/audio.so"`, `"user32.dll"`) is used exactly as written.
- `as "symbol"` overrides the exported name, for when the symbol a library
  exports is not what the program wants to call it by. It is optional; without
  it the declaration name is the symbol name.

The types that cross the boundary are `int`, `int64`, `double`, `bool`, `text`
and `ptr`. A `text` is passed as a C `char *` and a returned `char *` is copied
into a managed text; a `ptr` is passed straight through. Passing an OpenEPL
array, dictionary or record by value is not part of this — pass a `ptr` to the
bytes instead.

### A worked example

Given a small C library `mathdll` with `int add_ints(int, int)`,
`const char *banner(void)`, `void bump(int *)` and `int times_ten(int)`, a
program calls each the way its signature reads. `bump` takes a pointer the callee
writes through, so the program hands it a `ptr` to a cell it allocated:

```openepl
module mathdll

dll add_ints(a: int, b: int): int from "mathdll"
dll banner(): text from "mathdll"
dll bump(cell: ptr) from "mathdll"
dll tentimes(x: int): int from "mathdll" as "times_ten"

sub main
  call print_int(add_ints(40, 2))
  call print_text(banner())

  let cell: ptr = mem_alloc(4)
  call ptr_write_int(cell, 0, 41)
  call bump(cell)
  call print_int(ptr_read_int(cell, 0))
  call mem_free(cell)

  call print_int(tentimes(5))
end
```

### Loading is lazy

A library is opened at the first call to one of its functions, not at start-up.
A program may declare a `dll` from a library that is not present and still build
and run — it reaches out only when it actually makes the call. When that call
comes and the library or the symbol cannot be found, the program stops with a
message naming both, so a foreign call that cannot be made is a visible failure
rather than a silent zero.

## Callbacks: passing a sub to C

`dll` lets a program call C. `address of` is the other direction: it hands C the
address of a subroutine, so C can call back into OpenEPL. This is what a hook
detour, a `CreateThread` ThreadProc, an `EnumWindows` callback or a Lua C
function all need — "here is my function, you call it".

```text
address of NAME
```

The result is a `ptr`: a real function pointer a C API invokes with the C
calling convention. The named subroutine must have a C-representable signature —
every parameter and the return in `int`, `int64`, `double`, `bool`, `text` or
`ptr`, or no return at all. A sub that takes or returns an array, a dictionary
or a record has no address you can hand across, and `address of` on it is a
compile error that names the sub and the type that does not fit. A `bool` is
C's `int`; a callback shaped like a Win32 `WNDPROC` uses `int64` for the
pointer-width `WPARAM`, `LPARAM` and `LRESULT`.

Given a C library `cb` that calls back through a function pointer:

```c
int apply(int (*fn)(int, int), int a, int b) { return fn(a, b); }
void each(void (*fn)(int), int n) { for (int i = 1; i <= n; i++) fn(i); }
```

a program declares each as a `dll` taking a `ptr` where C takes the function
pointer, and passes `address of` a matching sub:

```openepl
module callbacks

dll apply(fn: ptr, a: int, b: int): int from "cb"
dll each(fn: ptr, n: int) from "cb"

sub summer(a: int, b: int): int
  return a + b
end

sub announce(n: int)
  call print_int(n)
end

sub main
  var add: ptr = address of summer
  call print_int(apply(add, 40, 2))

  call each(address of announce, 3)
end
```

`apply` calls `summer` for its result and the program prints `42`; `each` calls
`announce` with 1, 2 and 3 from inside its own loop, and the program prints that
sequence. C is driving OpenEPL code in both.

The subroutine runs on whatever thread and stack C calls it from, with C's
calling convention, and it makes no assumption about an event loop — a ThreadProc
handed to `CreateThread` runs on the new thread, and coordinating that with the
rest of the program is the program's own affair. A `text` parameter arrives as
the C `char *` the caller passed, read for the duration of the call; a `text` the
callback returns is storage the OpenEPL runtime owns and frees.

Every target OpenEPL builds is 64-bit — x86-64 Linux, x64 Windows, and 64-bit
macOS — and each has a single C calling convention, so `cdecl`, `stdcall` and
`fastcall` all name the same one and a callback needs no convention marker. (A
32-bit `stdcall` callback would; OpenEPL emits no 32-bit target, so it does not
arise.)

## A library the loader runs: DllMain

A `sharedlib` normally has no start-up moment. It exports its subroutines and
waits; a host calls `<module>_init` once to set up module variables (see
[Build targets](./build-targets.md)) and then calls whatever it needs. Nothing
runs on its own, because a library should not run your code before the host is
ready for it.

A library that is *loaded for effect* — injected into a process, or brought in
with `LoadLibrary`/`dlopen` for what it does rather than what it exports — has no
host willing to make that first call. It has to run the instant it is mapped.
Two specially-named subroutines give it that moment:

```text
sub dll_attach   # runs when the library is mapped into a process
sub dll_detach   # runs when it is unmapped
```

Each takes no parameters and returns nothing — the loader calls them with
neither. Define one or both in a `target sharedlib` module and the compiler
wires it to the platform's loader entry:

| | `dll_attach` | `dll_detach` |
| --- | --- | --- |
| **Windows** | `DllMain`, `DLL_PROCESS_ATTACH` | `DllMain`, `DLL_PROCESS_DETACH` |
| **Linux** | `__attribute__((constructor))` | `__attribute__((destructor))` |

`<module>_init` runs first, before `dll_attach`, so any module variable the hook
touches is already set up. A sharedlib that defines neither hook is unchanged:
it gets no loader entry, and the host calls `<module>_init` itself as before.

`dll_attach` runs under the OS loader — on Windows, while the loader lock is
held. That is the right place to install a hook or record that the library
loaded, and the wrong place for anything slow or anything that loads another
library: work heavier than a few assignments belongs on a thread the hook
spawns (through a `dll` to `CreateThread` or `pthread_create`), which runs once
the loader has let go.

### A worked hook

The three pieces above compose into a real, self-contained hook: a library that,
the moment it loads, redirects a function another part of the program is calling
— in the same process, with nothing patched by hand.

The function being hooked lives in a small C library, `hookrt`. It dispatches
every call through a slot, so redirecting the slot redirects every future call;
installing a detour keeps the old target as a trampoline the detour can still
reach:

```c
static int base_target(int x) { return x * 2; }        /* the original */
static int (*slot)(int) = base_target;                 /* what call() dispatches to */
static int (*trampoline)(int) = base_target;           /* the saved original */

int  hookrt_call(int x)     { return slot(x); }        /* the application's call site */
int  hookrt_original(int x) { return trampoline(x); }  /* the detour reaches the original */
void hookrt_install(void *detour) {                    /* redirect the slot */
    trampoline = slot;
    slot = (int (*)(int))detour;
}
```

The OpenEPL library declares the two functions it needs as `dll`, writes the
detour as an ordinary subroutine, and installs it from `dll_attach` with
`address of`:

```openepl
module hook
target sharedlib

dll hookrt_original(x: int): int from "hookrt"
dll hookrt_install(detour: ptr) from "hookrt"

sub detour(x: int): int
  return hookrt_original(x) + 1
end

sub dll_attach
  call hookrt_install(address of detour)
end
```

A host program links `hookrt` and calls `hookrt_call` directly — it is the
application whose function gets hooked. It calls once, loads the OpenEPL library,
and calls again:

```c
extern int hookrt_call(int x);

#ifdef _WIN32
#include <windows.h>
static int load_hook(void) { return LoadLibraryA("hook.dll") != NULL; }
#else
#include <dlfcn.h>
static int load_hook(void) { return dlopen("./libhook.so", RTLD_NOW | RTLD_LOCAL) != NULL; }
#endif

int main(void) {
    printf("before %d\n", hookrt_call(10));   /* 20: the original, x*2 */
    load_hook();                              /* fires dll_attach, installs the detour */
    printf("after %d\n", hookrt_call(10));    /* 21: the detour, original + 1 */
    return 0;
}
```

Loading the library is the whole of it: `dll_attach` runs under the loader,
installs the detour into the one loaded `hookrt`, and the host's next call —
made through the same slot — lands in the OpenEPL `detour`, which reaches the
original through the trampoline and adds one. The program prints `before 20`
then `after 21`, on Linux through the constructor and on Windows through
`DllMain`. The complete, buildable example is in
[`examples/hook/`](https://github.com/axdsan/openepl/tree/main/examples/hook).
