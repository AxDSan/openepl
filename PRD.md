# OpenEPL — Product Requirements Document

An open-source, cross-platform reimplementation of **EPL (易语言 / E-Programming-Language)** and a
**BlackMoon-style static native compiler** for it.

- **Status:** Draft v1.0
- **Date:** 2026-08-30
- **Owner:** abdi.moya@gmail.com
- **Repo home:** `/home/aj/Documents/DevStuff/openepl`

---

## 0. TL;DR

EPL is a Chinese "programming in your native language" system: an all-Chinese, table-driven,
event-oriented RAD IDE + a runtime built on a plug-in **support-library** (支持库) model. Programs
are authored as a structured **e-code** object model (not free text), and normally ship as a small
EXE plus `.fne`/`.fnr` support-library files loaded at runtime.

**BlackMoon (黑月)** is a third-party compiler plug-in for EPL that does something different from the
official compiler: instead of embedding+extracting the support libraries at runtime, it **translates
the e-code into standard COFF object files (`.obj`), then links them with Microsoft `LINK.EXE`
against a from-scratch C/C++ reimplementation of the core runtime (`kernel.lib`)** — pulling in only
the code fragments for commands the program actually uses. Result: a clean, standard PE binary with
no external support-library dependency, smaller size, far fewer antivirus false positives, and normal
debuggability.

**OpenEPL** rebuilds this whole stack as MIT/BSD open source. **The defining idea is RAD-first** —
OpenEPL is a Rapid Application Development environment (draw the app, wire behavior to events, run) that
happens to compile to clean native binaries; everything below serves that loop (see §4.5). Goals:
1. **RAD-first (the product identity).** A visual designer + component model + event-driven authoring +
   integrated edit→run→debug→package loop, in the VB/Delphi/EPL lineage — core from day one, not a
   deferred UI track.
2. A **radically easy, English-first language** — EPL's table-driven, footgun-free ease taken
   seriously, but with English (not Chinese) as the canonical language of syntax, command names, and
   tooling: no C/C++ syntax or ceremony, deep abstraction, so even hard things (up to kernel-mode
   drivers) become approachable. Ease is a hard constraint, not a slogan.
3. A documented, stable **e-code IR** (modeling forms/components/events) and support-library ABI (the
   thing EPL never fully opened).
4. A **native backend** in the BlackMoon spirit — IR → object code → system linker, dead-code-lean,
   no runtime unpacking — with **hardened, non-decompilable release output** (unlike EPL/.NET).
5. **Cross-platform** from day one (Linux/macOS/Windows, x64/arm64), where EPL/BlackMoon were 32-bit
   Windows-only.

---

## 1. Background & Research

> Full primary-source notes are in `docs/research/`. Key sources:
> - Official EPL Support-Library SDK (EDK) manual, Delphi edition — `edk-support-library-sdk.md`
>   (https://www.dywt.com.cn/sdk/delphi/docs/)
> - BlackMoon original author's design writeup (云外归鸟) — `blackmoon-design.md`
>   (http://www.ywgn.net/forum.php?mod=viewthread&tid=8)
> - Reverse-engineering analysis of EPL binaries (看雪/PlaneJun) — `epl-binary-analysis.md`
>   (https://zhuanlan.zhihu.com/p/569792913)
> - BlackMoon core static-library **open source** (`kernel.lib`) — `blackmoon-kernel-repo.md` (+ `tree.json`)
>   (https://github.com/zhongjianhua163/BlackMoonKernelStaticLib, BSD-3-Clause)

### 1.1 What EPL actually is

EPL ("易语言", *easy language*), by Dalian Dayou Wutao (大连大有吴涛), is a RAD environment whose
distinguishing traits are:

- **Fully Chinese, keyword-free syntax.** All program *definition* (variables, params, types) is done
  through **table/form filling**, not text keywords — the user never memorizes declaration syntax.
  Command call syntax is uniform across every command. Source code has an enforced canonical format,
  so any two authors' code looks identical ("代码即文档").
- **Event-driven, optionally fully OO.** Forms + controls + event subroutines, MFC-like.
- **e-code is a structured object model, not text.** A program is a tree of definitions and statements
  with numeric type tags and library/command indices — closer to a serialized AST than to a `.c` file.
  This is why EPL source lives in its own `.e` container. It is **also EPL's weakness**: its
  static-compiled binaries still carry enough of that structure + metadata that decompilers reconstruct
  clean Chinese source (much like .NET IL round-trips to C#). **OpenEPL treats this as a defect to fix,
  not a feature** — the e-code IR is a *build-time* artifact that must not survive into shipped
  binaries; release output should resist decompilation, not invite it (see G8, D7).
- **Runtime = core support library + plug-in support libraries.** Almost nothing is "built in." Even
  arithmetic, text, file, and date/time commands live in the **core library `krnln`**. Everything else
  (internet, database, DirectX, regex, …) is an additional support library.
- **Small-DB + localization batteries included:** a bundled lightweight database, Chinese date/pinyin/
  full-width-half-width/RMB-amount/lunar-calendar handling, and interop with OCX/TypeLib/Win32 API/Java.

### 1.2 Support libraries: `.fne` / `.fnr` / `.lib` and the ABI

Support libraries are the extension mechanism and the core ABI. File kinds:

| Extension | Meaning |
|---|---|
| `.fne` | Support library, *design-time + runtime* (carries IDE metadata: names, help, categories, param pickers). |
| `.fnr` | Support library, *runtime-only* (stripped of IDE metadata; what ships with a program). |
| `.fnl` | Static-link variant naming used by some versions. |
| `..._static.lib` in `static_lib/` | **Static** support library for EPL 5.0+ static compilation — linked into the EXE, nothing dropped to disk. |
| `.fne`/`.fnr` in `lib/` | Ordinary (dynamic) support library files. |

**The entire contract is a single exported function.** A `.fne`/`.fnr` is a Windows DLL that exports
**`GetNewInf`**, returning a pointer to a fully-populated **`LIB_INFO`** struct. Everything the IDE and
runtime know about a library is reachable from that struct. From the official EDK manual:

> 易语言对支持库的要求很简单，只要导出 `GetNewInf` 函数并返回填充完整的 `LIB_INFO` 结构体的内存首地址即可。

`LIB_INFO` (via the `DefineLib()` helper) carries: name, GUID (stable per library), description,
major/minor/build version, language, state flags, author/homepage, **command categories**, an
add-in/plug-in entry, a **system-notification callback (`pfnNotifyLib`)**, dependency file list, and an
unload handler.

**Commands (子程序 / library functions).** Each command is registered with `DefineCommand(impl,
args, cnName, enName, help, returnType, state, level, category)`. Its implementation has a fixed C
signature:

```c
// cdecl. pRetData points at the return slot; pArgInf is an array of nArgCount arg slots.
void Command(pMDATA_INF pRetData, int nArgCount, pMDATA_INF pArgInf);
```

`ARG_INFO` describes each parameter (name, help, bitmap index, **data-type tag**, default, state
flags — e.g. by-reference, array-allowed, receive-array).

**Data-type tags (`SDT_*`)** — the type system carried in every slot:

| Tag | C type / storage |
|---|---|
| `SDT_BYTE` | `BYTE` |
| `SDT_SHORT` | `SHORT` |
| `SDT_INT` | `INT` (32-bit) |
| `SDT_INT64` | `INT64` |
| `SDT_FLOAT` | `FLOAT` (32-bit) |
| `SDT_DOUBLE` | `DOUBLE` |
| `SDT_DATE_TIME` | OLE `DATE` |
| `SDT_BOOL` | `BOOL` |
| `SDT_TEXT` | pointer to NUL-terminated string; `NULL` = empty string |
| `SDT_BIN` (字节集 / byte-set) | pointer to `{ INT constant==1; INT length; length bytes }`; `NULL` = empty |
| `SDT_SUB_PTR` | subroutine pointer |
| `SDT_STATMENT` | conditional-statement type |
| `_SDT_NULL` / `_SDT_ALL` | no-type / any-type (return/param declarations only) |

Special aggregate storage rules that the IR and codegen **must** honor exactly (they are the ABI):
- **Struct members are aligned to 4 bytes.** `{byte A; short B; int C}` occupies 12 bytes (A@0, B@4, C@8).
- **Arrays** are a pointer to `{ INT dimensionCount; INT dimSizes[dimensionCount]; data }`.
  - `SDT_TEXT`/`SDT_BIN`/composite arrays: data is a pointer array (each may be `NULL`).
  - Simple + window-unit/menu arrays: data is the elements laid out sequentially.
- **Access-length rule:** you may only touch a datum's *actual* (non-aligned) length, because a
  by-ref array element is passed as a bare pointer with only the element's real bytes valid.

**System-notification callback (the runtime↔library back-channel).** Libraries receive events from
the runtime via `pfnNotifyLib(nMsg, dwParam1, dwParam2)`. The BlackMoon open source implements the
core side of this as `BlackMoonFuncForeLibNotifySys`, dispatching messages like:
`NRS_MALLOC`, `NRS_MFREE`, `NRS_MREALLOC` (all heap allocation for EPL data goes through the runtime),
`NRS_FREE_ARY` (free array data by `SDT_*`), `NRS_RUNTIME_ERR` (raise a runtime error), etc. This is
how memory ownership and error propagation stay consistent across the EXE and every library.

Constants (`DefineConst`), enums (`DefineEnumDatatype`), user/UI data types (`DefineDatatype`,
`DefineUIDatatype`) round out the metadata surface. A library may reference the **core** library's
types (`DTP_*`) and its **own** types (index+1) but **not** another library's types.

### 1.3 How the official EPL compiler ships a program

Reverse-engineering notes (`epl-binary-analysis.md`) confirm four modes, differing only in *where the libraries live*:

1. **编译 / Compile (dynamic):** support-library files (`.fnr`) are emitted next to the EXE; must ship together.
2. **非静态编译 / Non-static:** libraries are embedded in the EXE and **written into the EXE image**;
   at run time the loader reads its own image, and calls libraries **by (load-ordinal, function-offset)**
   passed in registers — library ordinal 0 is `krnln`, loaded first, passed implicitly.
3. **独立编译 / Standalone:** like static in that only the EXE ships, but at run time it **drops the
   libraries to a temp dir** and loads them.
4. **静态编译 / Static (EPL 5.0+):** static `*_static.lib` files are linked into the EXE (like a normal
   C program). Nothing is dropped; fewer AV false positives; addresses are static (hence analyzable).

Standard-entry mechanics observed in binaries: non-static builds locate `krnln`, resolve **`GetNewSock`**,
then `call eax` into the fixed EPL entry. Library calls go through a **variadic `E_FuncCallBack`** whose
first arg is the parameter count, followed by `(value, ignorable, type)` triples per argument. EPL can
also emit **junk/obfuscation instructions (花指令)** at configurable levels to frustrate reversing (but
each level's pattern is fixed → scriptable to strip).

**Takeaway for OpenEPL:** the "library ordinal + function offset" indirection and the runtime-unpacking
are exactly what makes EPL binaries look like malware to AV and hard to debug. BlackMoon removes both.

### 1.4 How BlackMoon works (the model we copy)

From the original author (云外归鸟, `blackmoon-design.md`), plus the open-source `kernel.lib`
(`blackmoon-kernel-repo.md`; 290 files, 244 `krnln/*.cpp`):

> 黑月……不需要类似的斩月壳，而是**分析并转化易程序为对象文件 obj，再用链接器 LINK.EXE 重新编译链接**。

Pipeline:

```
EPL e-code  ──(BlackMoon translate)──►  standard COFF .obj  ──►  LINK.EXE  ──►  standard PE (EXE/DLL)
                                              ▲
                                     kernel.lib (C/C++ reimpl of krnln)
                            + RC-compiled PE resources + optional C/MASM32 .lib
```

Defining properties (all reproduced by OpenEPL):

1. **Real object-file emission + system linker.** Not an interpreter, not a runtime unpacker. Output is
   a conventional PE with a normal import table, normal resources, normal debug info.
2. **Reimplemented core runtime as a static library.** `kernel.lib` (open source, BSD-3-Clause) is a
   ground-up C/C++ rewrite of `krnln`'s non-UI commands, **behavior-for-behavior compatible** — even
   replicating original bugs where programs depend on them. The repo's own coding rules demand exact
   parity with the native core lib for every command's params/return/behavior.
3. **Fragment extraction / dead-code-lean linking.** "用到的命令函数才提取相应部分代码。不用到命令
   不合成到目标程序" — because it's real static linking, only the object fragments for commands the
   program references are pulled in. Unused commands cost zero bytes. This is why BlackMoon output is
   *smaller* than embed-everything modes.
4. **Standard PE resources via RC.** Windows/dialogs can be built from RC scripts (compatible with C/
   MASM32 RC), enabling a real resource tree instead of EPL's private format.
5. **C ABI extensions.** Supports `__cdecl` external functions and **linking third-party C or MASM32
   `.lib`** — EPL itself couldn't. DLLs get a real `DllMain`-style "Dll入口函数".
6. **Optimizes away EPL's init preamble.** BlackMoon "直接会优化掉前期的初始化代码，包括 CLR (C
   library runtime) 初始化，直接进入到易语言的标准入口" — a lean entry straight into e-code.

Entry model, from the open source (`krnln/BlackMoonExe.cpp`):

```c
extern "C" int ECodeStart();            // the translated user program (emitted as obj)
int _cdecl BMEntrypoint()  { E_Init(); ECodeStart(); return 0; }   // lean entry
int WINAPI  WinMain(...)   { E_Init(); /* save esp/ebp */ call ECodeStart; return eax; }
int         main(...)      { E_Init(); /* save esp/ebp */ call ECodeStart; return eax; }
// DLL: DllMain → DLL_PROCESS_ATTACH: E_Init(); DestroyAddress = DllEntryFunc();
//               DLL_PROCESS_DETACH: E_DestroyRes();
```

`E_Init()` grabs the process heap and runs `BlackMoonInitAllElib()` (init every linked support lib);
`E_DestroyRes()` runs each library's destroy hook and `BlackMoonFreeAllElib()`. Memory is the runtime's
job (`E_MAlloc/E_MFree/E_MRealloc`), surfaced to libraries through the `NRS_*` notifications above.

**Known BlackMoon limitations we should beat:** no native window/UI commands (users hand-rolled "自绘"
custom-drawn UIs); 32-bit x86 only; Windows only; parity maintained by hand.

---

## 2. Problem & Opportunity

- EPL's IR and library ABI are **de-facto but undocumented and closed**; the ecosystem is 32-bit
  Windows and Chinese-only; official binaries trip antivirus and resist debugging.
- BlackMoon proved the **"translate to obj + link a reimplemented static core"** model works and is
  superior for distribution — but it's closed, 32-bit-x86/Windows-only, UI-less, and maintained by hand.
- **The gap:** the beloved RAD experience (VB6/Delphi/EPL — draw an app, wire events, ship) has no
  modern, open, cross-platform heir that also produces *clean native* binaries. Electron/web stacks
  aren't RAD-native-binary; surviving native RAD tools are proprietary, legacy, or platform-locked.
- **Opportunity:** a **RAD-first environment** — visual designer + component model + event-driven
  authoring — sitting on an openly-specified e-code IR, a BlackMoon-style native backend, and a portable
  runtime = "draw it, wire it, ship a small hardened native binary," cross-platform and open. The RAD
  loop is the differentiator; the IR + backend + ABI are what make its output clean, portable, and hard
  to reverse.

## 3. Goals / Non-Goals

### Goals
> **G0 (headline, overrides priority ties): RAD-first is the product identity.** OpenEPL is a Rapid
> Application Development environment before it is a compiler — a visual designer, a component model with
> properties/methods/events, event-driven authoring, and one integrated edit→run→debug→package loop, all
> core from day one (see §4.5, D9, R7). Every other goal serves the design→wire→run→ship loop; when a
> choice helps raw compilation but hurts that loop, the loop wins.
- G1. **Open, versioned e-code IR** (serialized, stable, documented — **models forms, components,
  properties, and events**, not just statements) — the artifact EPL never published.
- G2. **BlackMoon-style native backend:** IR → object code (via LLVM) → system linker; dead-code-lean;
  no runtime unpacking; standard executables. Two build profiles — `dev` (debug symbols) and `release`
  (stripped + hardened, see G8).
- G3. **Portable core runtime** (`libopenepl_core`) reimplementing EPL's core commands with defined
  semantics, incl. the byte-set/array/struct storage ABI and the runtime↔library notification channel.
- G4. **Documented support-library ABI** (`GetNewInf`/`LibInfo` analog) with a clean SDK so libraries
  can be written in C, C++, Rust, or Zig.
- G5. **Cross-platform:** Linux, macOS, Windows; x86-64 and arm64.
- G12. **One project → every desktop artifact, on every platform.** From the same RAD project you select
  the output kind — GUI/console **executable**, **dynamic library** (`.dll`/`.so`/`.dylib`), **static
  library**, **kernel driver/module** (`.sys`/`.ko`/`.kext`, via the systems track), and service/plugin/
  bundle variants — and target any supported OS/arch, without rewriting the project. It's a build-target
  choice over one IR + component model; the backend profile supplies the right entry/ABI/section/runtime
  (§5.2 matrix, §5.6). The RAD environment is a general desktop toolsmith, not just an app builder.
- G6. **Interop:** import/analyze existing static-compiled EPL binaries' command set; optional EPL `.e`
  source importer (stretch).
- G7. **AV-clean output** (the original BlackMoon value proposition), verifiable in CI. Debug symbols
  are a **dev-build** convenience, not a property of release output (see G8).
- G8. **Hardened release output — resist decompilation/reversing.** Unlike EPL/.NET, a release build
  must **not** round-trip back to source. The e-code IR is build-time only and is **never embedded** in
  the binary; no rich command/type metadata, no source-language (English or localized) name strings, no interpreter-style dispatch
  table ships. Native machine code (§5.2) already destroys the AST; on top of that, release builds strip
  all symbols/debug info and offer opt-in hardening (string/constant obfuscation, control-flow
  flattening, anti-tamper). Goal: decompilers recover, at best, unlabeled low-level pseudo-C — never
  the original structured program. Debuggability (G7/M4) is explicitly a **dev-build-only** trait.
- G9. **Radical ease of use — the language must be genuinely easy, not "easy for programmers."**
  This is the EPL premise and a hard, non-negotiable design constraint (see Language Design Principles,
  §5.0). No pointer arithmetic, no manual memory management, no header/declaration ceremony, no
  C/C++-style syntax or footguns, no undefined behavior surfaced to the user. Uniform command-call
  syntax, table/form-driven definitions, sensible defaults, read-like-prose code. A beginner should
  accomplish real work on day one; the abstraction — not the user — absorbs the complexity.
  **English-first:** everything EPL expresses in Chinese (syntax, keywords, command/type names,
  identifiers, IR labels, SDK, errors, docs) is **English** in OpenEPL; Chinese and other languages are
  an optional localization layer over the English canon, never the base (§5.0).
- G10. **"Hard things made easy," up to and including systems/kernel-mode code.** The ambition: writing
  a kernel-mode driver (or other low-level/systems code) should be *as approachable as a normal
  program* — the ring-0 hardship (IRP/WDF/DDI plumbing, freestanding/no-CRT constraints, callback
  lifetimes, IRQL rules) is absorbed by a **driver/systems framework + support libraries + a
  freestanding backend profile**, not exposed as raw syntax. The user writes simple, high-level command
  calls; the framework generates the correct, safe low-level scaffolding. (Scope/feasibility: this is a
  later track — see §5.6, D8, N5, R6 — but the language and IR are designed from day one so it's
  reachable, not bolted on.)

### Non-Goals (v1)
- N1. Bit-exact binary compatibility with EPL's private e-code container or `.fne` binary format
  (we define our own; a converter can come later).
- N2. **Cloning EPL's exact IDE, forms format, or control set.** RAD/visual authoring is *core* (G0,
  §4.5) — but we build a modern, portable RAD environment on our own IR + component ABI, not a
  bit-compatible EPL IDE reimplementation. (v1 ships a *minimal but real* designer, not a mature one.)
- N3. Reusing EPL/BlackMoon proprietary binaries or non-open code. We build from the open `kernel.lib`
  (BSD-3) and public specs only. **License hygiene is a hard requirement (§9).**
- N4. Running unmodified proprietary third-party `.fnr` libraries (Win32/x86-specific) as-is.
- N5. **Shipping the kernel/systems track in v1.** G10/§5.6 is a design *constraint on the foundations*
  now, but the freestanding backend profile and driver framework are a later track — v1 must not block
  it, but doesn't deliver it. No claim of production driver-signing/WHQL support in v1.

## 4. Users & Use Cases

- **U1. RAD app builders (primary).** VB6/Delphi/EPL-style developers and citizen developers who want to
  *draw* a desktop app, wire events, and ship a small native binary — cross-platform, open, no Electron.
- **U2. EPL developers** wanting clean, portable, debuggable native binaries without AV grief.
- **U3. Toolchain/PL hackers** wanting a small, real "frontend-IR-backend + system linker" codebase to
  study or retarget.
- **U4. Educators / native-language-programming advocates** (the EPL premise) beyond 32-bit Windows.
- **U5. Reverse engineers / security researchers** analyzing EPL malware who want an open reference for
  the IR, the ABI, and the junk-instruction patterns.

## 4.5 The RAD-First Thesis (product identity — G0)

OpenEPL is, first and foremost, a **Rapid Application Development** environment — that is the whole point,
not a feature. EPL, like Delphi and Visual Basic before it, won its users by letting a person *draw* an
application and wire behavior to events in minutes. Everything else in this document — the IR, the
BlackMoon-style native backend, the hardening, the ease mandate — exists to **serve a RAD workflow that
ends in a clean native binary.** If a decision helps raw compilation but hurts the design→wire→run→ship
loop, the loop wins.

Concretely, RAD-first means these are **core, day-one concerns**, not a deferred "UI track":

- **Visual-first authoring.** A visual designer — component toolbox, drag-and-drop layout, a properties
  inspector, an event list — is the primary way you build an app. Hand-written code is fully supported
  but never *required* to stand up a working UI.
- **Component model at the center.** Reusable components with **properties, methods, and events** are
  first-class in the IR *and* the support-library ABI. EPL already specifies the UI-component
  interface-function contract (OnCreate / OnGetProperty / OnSetProperty / SaveProperties — see
  `docs/research/edk-support-library-sdk.md` §9); OpenEPL adopts that shape on a portable widget layer.
  The designer and the runtime share **one** component model.
- **Event-driven by default.** Double-click a button → an event-handler subroutine appears, wired
  automatically; the uniform, easy call syntax (§5.0) fills the body.
- **One integrated environment.** Design, code, run, debug, and package in a single tool with a tight
  edit→run loop and live preview — not a compiler you feed from a separate editor.
- **Batteries included.** A stock library of widgets and data-bound components, plus the standard support
  libraries (text/file/net/db), so common apps need almost no plumbing.
- **From canvas to hardened native binary.** The same project the designer produces compiles through the
  BlackMoon-style backend (§5.2) to a small, AV-clean, hardened native executable — VB/Delphi RAD
  ergonomics with the distribution profile BlackMoon proved out.

This makes the **designer, component model, and integrated IDE part of the core product**, developed
alongside the backend rather than after it (see revised Milestones, §7). The non-goal (N2) is only
*cloning EPL's exact IDE/forms format* — not RAD itself.

**RAD here is a general desktop toolsmith, not just an app builder (G12).** The same visual project can
be built into *any* desktop artifact, for *any* supported platform — a GUI or console **executable**, a
**dynamic library** (`.dll`/`.so`/`.dylib`), a **static library**, a **kernel driver/module**
(`.sys`/`.ko`/`.kext`, via the systems track), or service/plugin/bundle variants — by choosing the
output kind in the project settings (§5.2 matrix). You draw and wire once; the backend profile supplies
the correct entry point, calling conventions, section layout, and runtime subset per artifact and OS.

## 5. Product Scope — Components

```
openepl/
├─ ide/           # ★ integrated RAD environment: project mgmt, edit→run→debug→package loop, live preview
├─ designer/      # ★ visual form/UI designer: component toolbox, drag-drop layout, properties inspector,
│                 #   event wiring — produces IR (forms + components + event handlers)
├─ components/    # ★ first-party visual component library (widgets, data-bound controls) + the shared
│                 #   component model (properties/methods/events) used by designer AND runtime
├─ ir/            # e-code IR: schema (statements + forms/components/properties/events), (de)serializer,
│                 #   validator, text ↔ binary forms
├─ frontend/      # non-visual authoring surfaces that PRODUCE IR
│   ├─ importer-epl/   # (stretch) EPL .e / static-EXE command-set importer
│   └─ lang/           # textual surface syntax for IR (English-first; other langs via localization)
├─ backend/       # BlackMoon-style: IR → LLVM IR → obj → system linker
├─ runtime/       # libopenepl_core (portable reimpl of core commands) + component/event runtime
├─ abi/           # support-library ABI headers + SDK (GetNewInf/LibInfo analog, SDT_* types, and the
│                 #   UI-component interface-function contract: OnCreate/OnGet/OnSetProperty/Save…)
├─ libs/          # first-party support libraries (text, file, datetime, math, net, db, …)
├─ tools/         # CLI (openepl build/run/inspect), disasm/strip-junk, AV-false-positive harness
├─ docs/          # spec + research  (research/ already populated)
└─ tests/         # golden IR, ABI conformance, semantic parity vs documented EPL behavior
```
★ = RAD-first core (G0): built alongside the backend, not after it.

### 5.0 Language Design Principles (the ease mandate — G9/G10)

These are constraints on every surface syntax and library API, not aspirations. If a feature can't be
made easy, the *abstraction* changes — never the user's burden.

- **No ceremony.** No headers, forward declarations, manual prototypes, build boilerplate, or
  keyword-declaration syntax. Definitions (vars, params, types) are table/form-driven (EPL's model);
  the tool fills in what other languages make you type.
- **No footguns.** No raw pointers or pointer arithmetic exposed by default, no manual malloc/free
  (runtime owns memory, §5.3/D4), no null-terminator/buffer bookkeeping, no undefined behavior reaching
  the user, no header/macro/template metaprogramming. Memory-unsafe operations exist only behind an
  explicit, clearly-marked "unsafe" escape hatch for experts.
- **One uniform call syntax.** Every command — core, library, user subroutine, even a driver operation —
  is invoked identically. Learn it once.
- **English-first — everything EPL did in Chinese is English here.** EPL's defining trait is that its
  syntax, keywords, command names, categories, identifiers, and IR labels are all Chinese. OpenEPL
  **inverts that**: the canonical, primary language of the whole system — surface syntax, command/type
  names, the SDK, error messages, IR text form, docs — is **English**. Chinese (and any other language)
  is available only as an *optional localization layer* over the English canon, never the base. No
  Chinese identifiers or name strings anywhere in the default toolchain or output.
- **Reads like prose, not like C.** Plain-English command names, sensible defaults so common cases need
  few args, guided parameter entry.
- **Errors are legible.** Plain-language diagnostics and runtime errors (via the `NRS_RUNTIME_ERR`
  channel), never a wall of template/linker noise.
- **Abstraction absorbs difficulty, then gets out of the way.** High-level by default; a graduated set
  of escape hatches (raw API calls, C/asm static-lib linking per D5/BlackMoon, the unsafe tier) so
  power is *available* without being *mandatory*. Ease is the default path, not the ceiling.
- **The hard target proves the principle.** If kernel-mode driver code (§5.6) can be made this easy,
  ordinary application code trivially is. G10 is the north-star stress test for the whole design.

### 5.1 e-code IR (the keystone — G1)
- A typed, tree-structured IR: modules → (constants, enums, user types, globals, subroutines) →
  statements/expressions, every slot carrying an `SDT_*`/typeref tag.
- Two encodings: **binary** (compact, canonical, hashable — the shipping form) and **text** (diff-able,
  for tests/PRs). Round-trip lossless.
- Explicit, documented **storage ABI**: 4-byte member alignment; byte-set `{1,len,bytes}`; array
  `{dims, dimSizes[], data}`; text/bin/composite arrays as pointer arrays; the access-length rule.
- Versioned with a magic + semver; a validator rejects malformed/ABI-violating IR.

### 5.2 Native backend (BlackMoon model — G2)
- Lower IR → **LLVM IR** → object files; invoke the platform linker (lld/link.exe/ld) — the direct
  analog of "translate to obj, link with LINK.EXE."
- **Dead-code-lean:** runtime shipped as a static archive of per-command objects (or LLVM
  `--gc-sections`/function-sections) so only referenced commands link in (BlackMoon's fragment extraction).
- Emit a **lean entry** (`E_Init(); ECodeStart();`) with no runtime-unpacking, no library ordinal
  indirection, and standard imports.
- **Build profiles:** `dev` emits DWARF/CodeView debug info and keeps symbols. `release` (G8) embeds
  **no** e-code/IR, drops all command/type name strings (English or localized) and rich metadata, strips symbols/debug
  info, and applies opt-in hardening passes (string/constant encryption, control-flow flattening,
  anti-tamper/anti-debug) at selectable levels. The IR never ships in either profile.
- Targets: `x86_64-linux`, `aarch64-linux`, `x86_64-windows`, `aarch64-macos`, `x86_64-macos`.
  (32-bit x86 optional, for closest EPL/BlackMoon fidelity.)
- **Full desktop-artifact matrix (G12) — every kind, every platform, chosen in the RAD project settings
  and built from the *same* project:**

  | Artifact | Windows | Linux | macOS |
  |---|---|---|---|
  | Executable (GUI/console) | `.exe` | ELF exe | Mach-O / `.app` |
  | Dynamic/shared library | `.dll` (real `DllMain`) | `.so` (`.init`/`.fini`) | `.dylib` |
  | Static library | `.lib` | `.a` | `.a` |
  | Kernel driver / module *(systems track, Phase 6)* | `.sys` (WDM/WDF) | `.ko` | `.kext`/DriverKit |
  | Other | service, COM server, plugin | daemon, systemd unit | LaunchAgent, bundle |

  Each is a build-target selection over the same IR + component model; the correct entry, calling
  conventions, section layout, and (for drivers) the freestanding runtime subset (§5.6) are chosen by the
  backend profile, not by the user rewriting anything. UEFI/bare-metal are freestanding variants of the
  same machinery.

### 5.3 Core runtime (`libopenepl_core` — G3)
- Portable C/C++ (or Rust) reimplementation of core commands, organized **one command per object** for
  gc-friendliness. Directly informed by the open BlackMoon `krnln/*.cpp` (244 files) as a behavior
  reference — reimplemented, not copied, and cross-platform (no inline x86 asm; no Win32 assumptions).
- Owns all EPL-data heap allocation (`E_MAlloc/Free/Realloc`) and exposes the **notification channel**
  (`NRS_*` analog) to libraries.
- Command coverage baseline (from the `krnln` offset map in `epl-binary-analysis.md`): math; text
  (substring, find, replace, split/trim, case/width); byte-set ops; datetime (format, parts, add/subtract,
  intervals); file/dir I/O; memory files; env/commandline; RNG; bit ops; and CJK localization (pinyin,
  RMB-amount, full/half-width) behind an optional module.

### 5.4 Support-library ABI + SDK (G4)
- One documented export (`openepl_get_lib_info`) → `OpenEPL_LibInfo` (name, GUID, semver, categories,
  commands, constants, enums, types, notify-callback, deps, unload). Direct descendant of `GetNewInf`/
  `LIB_INFO`.
- Command impl signature mirrors EPL: `void cmd(RetSlot*, int argc, ArgSlot* argv)` with `SDT_*` slots;
  a method carries the object as arg 0 (`self`), per the EDK contract.
- **Visual components are ABI-first (G0/D9).** A library can contribute **visual components** by
  registering the UI interface-function set (OnCreate / OnGetProperty / OnSetProperty / SaveProperties +
  event forwarding — EDK digest §9) against the portable widget layer, so third-party components appear
  in the designer toolbox exactly like first-party ones.
- SDK: headers + a codegen'd binding layer so libraries (and components) can be authored in C/C++/Rust/
  Zig. A "hello" library template and a "hello component" template (the EDK `myfne`/UI analogs).

### 5.5 Tooling (G7)
- `openepl build|run|inspect|disasm|strip-junk`.
- **AV false-positive harness** in CI (submit to a multi-engine scanner or run local ClamAV/YARA) to
  prove clean output — this is the headline EPL/BlackMoon pain point, so we measure it.

### 5.6 Systems / kernel-mode track (the G10 stress test)

Making ring-0 code *easy* is a distinct, later track — but the IR/ABI are designed so it's reachable,
not retrofitted. Three pieces:

- **Freestanding backend profile.** A `kernel`/`freestanding` build profile: no CRT, no userland
  imports, correct calling conventions and section layout, target-appropriate object output
  (Windows kernel PE driver, Linux `.ko`-compatible object, UEFI, or bare-metal). LLVM already supports
  freestanding codegen; this profile constrains the IR lowering accordingly (no heap-by-default, no
  disallowed intrinsics, IRQL/context-safe runtime subset).
- **Driver/systems framework as support libraries.** The hard, error-prone plumbing — WDF/WDM IRP
  dispatch, callbacks and their lifetimes, IRQL discipline, IOCTL decoding, device/registry/PnP
  boilerplate — is packaged as high-level commands. The user writes "on device read → do X" in the
  uniform call syntax; the framework emits the correct, verified scaffolding. Same model on Linux
  (module init/exit, file-ops, sysfs) behind the same easy surface.
- **A safe runtime subset** usable in ring-0 (no userland assumptions, allocation via the kernel's
  allocator through the notification channel, no forbidden calls). The `unsafe` tier stays available
  for genuine low-level needs.

Honest constraints (see R6/N5): kernel code can crash the machine, needs code signing on Windows,
can't use the full userland runtime, and must obey strict context rules. "Easy" here means *the
framework absorbs the plumbing and guards the rules* — not that ring-0 stops being ring-0.

## 6. Architecture Decisions

| # | Decision | Rationale |
|---|---|---|
| D1 | **LLVM** as the backend, not hand-rolled x86 + `LINK.EXE`. | Retargetable (x64/arm64, 3 OSes), free optimizer + debug info + `gc-sections`; keeps the exact BlackMoon *model* (obj → system linker, lean static core) without its 32-bit-x86/Windows lock-in. |
| D2 | **Define our own IR & ABI**, don't clone EPL's private formats bit-for-bit. | EPL formats are closed, 32-bit, and undocumented; a clean spec is the whole value-add. Converters are additive later. |
| D3 | **One-object-per-command** static runtime. | Reproduces BlackMoon fragment extraction via standard linker dead-stripping. |
| D4 | Runtime **owns all EPL-data allocation**; libraries call back via notifications. | Matches EPL's memory-ownership model (`NRS_MALLOC/FREE/REALLOC`), which is what keeps array/text/bin ownership consistent across EXE + libs. |
| D5 | Study the **BSD-3 BlackMoon `kernel.lib`** as behavior oracle; **reimplement**, never vendor its source into non-BSD code. | Legal cleanliness + cross-platform (its code is Win32/x86-specific). |
| D6 | Runtime & backend language: **Rust** (safety, cross-compilation, LLVM via `inkwell`) with a C ABI, OR C++ if faster to reach parity. Decide in Phase 0 spike. | Both interop with LLVM & C ABI; pick on spike results. |
| D7 | **IR is build-time only; never embedded. Two build profiles: `dev` (symbols/debug) vs `release` (stripped + hardened).** | Native LLVM codegen already erases the AST; not shipping e-code/metadata + stripping + opt-in obfuscation is what stops the EPL/.NET clean-decompile. Debuggability and hardening are opposite ends of one knob, chosen per profile — never both in one artifact. |
| D8 | **Difficulty lives in libraries + backend profiles + optimizer, never in the surface language.** The language stays small, uniform, and abstract (G9); hard capabilities (kernel/systems, C/asm interop, SIMD) arrive as support libraries and a `freestanding` backend profile behind the *same* easy call syntax. | Keeps "easy for everyone" and "can do hard things" from fighting: the user never pays syntax cost for power they aren't using. This is exactly BlackMoon's proven move (link C/MASM32 `.lib` behind EPL commands, D5) generalized. |
| D9 | **One component model (properties/methods/events), first-class in both IR and ABI, shared by designer and runtime. Build the designer/widgets on a portable retained-mode UI layer, not per-OS native forms or an EPL-forms clone.** | RAD-first (G0) needs the *same* component the designer manipulates to be the one the runtime instantiates — a single model in the IR + the EPL-style UI interface-function ABI (EDK digest §9). A portable widget layer is what makes "draw once, run on Linux/macOS/Windows" real; native per-OS forms would fracture the RAD promise. Ties to Q5/Q9. |

## 7. Milestones

Two tracks advance together — the **native toolchain** and, because of G0, the **RAD environment**. The
first end-to-end demo that matters is *design a form, wire a button event, run it, ship a native binary*
(the "RAD vertical slice," Phase 3).

- **Phase 0 — Spike & Spec (3–4 wks).** Freeze IR v0 (text+binary) **including the component/property/
  event model**, ABI headers (commands + the UI-component interface functions), `SDT_*` + storage rules.
  Choose the **portable UI layer** (Q9) and backend language (Rust+inkwell vs C++). Deliver:
  `docs/spec/ir.md`, `docs/spec/abi.md`, `docs/spec/components.md`, and a hand-written IR that compiles
  "print + arithmetic" to a native binary on Linux x64.
- **Phase 1 — Compile core loop (4–6 wks).** IR validator; backend to obj+link on Linux x64; runtime
  with ~30 core commands (math/text/byteset/datetime); one-object-per-command dead-stripping proven.
  CLI `build/run`.
- **Phase 2 — Component & event runtime + ABI (5 wks).** Support-library ABI + SDK + "hello" library;
  `text`/`file` libraries via the ABI; notification channel end-to-end. **Component model live in the
  runtime:** a form with a button + an event handler, authored *in IR*, compiles and runs on the
  portable UI layer (no designer yet).
- **Phase 3 — RAD vertical slice (6 wks) ★.** The MVP designer: component toolbox, drag-drop layout,
  properties inspector, event wiring → emits IR; integrated **edit→run** loop and live preview. A user
  builds a small GUI app visually and produces a running native binary. This is the product's first true
  proof.
- **Phase 4 — Cross-platform + hardening (4 wks).** Windows (link.exe/lld) + macOS + arm64; DLL/shared
  output with real entry hooks; `release` profile (stripped + no-embedded-IR + opt-in obfuscation);
  AV false-positive CI harness green. RAD output runs on all targets.
- **Phase 5 — Broaden RAD (ongoing).** Larger component library, data binding, packaging/installers,
  debugger integration in the IDE. Textual surface syntax (English-first, localizable) → IR. Stretch:
  EPL importer + junk-instruction stripper.
- **Phase 6 — Systems track (later).** Freestanding backend profile + driver framework (§5.6).

## 8. Success Metrics
- **M0 (the RAD metric — headline).** A newcomer builds a working GUI app — drag components onto a form,
  set properties, wire a button's click event, hit run — and gets a running, then a shipped hardened
  native binary, in **minutes**, writing little or no code by hand. Measured as time-to-first-running-app
  and lines-hand-written (target: near zero for the UI).
- M1. A non-trivial program builds to a standard native binary on all 5 targets, runs identically.
- M2. Unused-command program is **measurably smaller** than all-commands build (proves fragment extraction).
- M3. **0 detections** on a clean "hello" and a mid-size sample across a multi-engine AV scan.
- M4. **Dev-profile** output loads in a standard debugger with source-level symbols (gdb/lldb/WinDbg).
- M4b. **Release-profile** output ships **no** e-code/IR, no debug symbols, and no command/type
  name strings; a decompiler (Ghidra/IDA/.NET-style tools) recovers no structured source — verified in CI.
- M5. A third party writes a working support library in C **and** Rust using only `docs/spec` + SDK.
- M6. Documented semantic parity: ≥95% of a defined core-command conformance suite passes.

## 9. Risks & Legal

- **R1 (Legal, highest).** EPL and proprietary support libraries are closed/commercial. **Mitigation:**
  build only from (a) the BSD-3-licensed BlackMoon `kernel.lib` as a behavior reference,
  reimplemented; (b) the publicly published EDK spec; (c) our own clean IR/ABI. No proprietary binaries,
  no non-open source vendored. Track provenance per module.
- **R2.** Behavior parity is a long tail (EPL replicates its own bugs; programs depend on them).
  **Mitigation:** conformance suite; document intentional divergences; parity is best-effort, not bit-exact.
- **R3.** Hardening/obfuscation (G8) is dual-use and could be seen as malware-enabling; it also fights
  the AV-clean goal (M3) if overdone. **Mitigation:** (a) baseline hardening in release is passive —
  *omission* (no embedded IR/metadata/symbols), which is legitimate and AV-neutral; (b) active
  obfuscation (control-flow flattening, packing, anti-debug) is opt-in, documented, and CI-checked
  against the AV harness so it doesn't reintroduce false positives; (c) also ship the
  stripper/analyzer (defensive) for studying EPL's junk-instruction patterns.
- **R4.** Scope creep into a full RAD IDE. **Mitigation:** UI is Phase 5+/stretch; v1 is IR+backend+runtime+ABI.
- **R5.** LLVM version churn / build weight. **Mitigation:** pin LLVM; provide prebuilt toolchain images.
- **R7 (RAD scope — largest execution risk).** A visual designer + component model + integrated IDE is a
  big surface; done naively it dwarfs the compiler and never ships. **Mitigation:** (a) build on an
  existing portable retained-mode UI toolkit (Q9), not a from-scratch widget set; (b) one shared
  component model (D9) so designer and runtime aren't built twice; (c) Phase 3 ships a deliberately
  *minimal* designer (toolbox, drag-drop, properties, event wiring) — real but small — then grows;
  (d) the RAD vertical slice is the forcing function that keeps the toolchain honest and integrated.
- **R6 (Ease vs power tension — central).** "Insanely easy" and "can write kernel drivers" pull against
  each other; over-abstracting hides needed control, under-abstracting leaks C-level complexity.
  **Mitigation:** D8 — the surface language stays easy and uniform; power arrives via libraries + a
  freestanding profile + a marked `unsafe` escape hatch, so ease is the default and control is
  opt-in. Validate the extremes with two proof points: a day-one beginner task and a minimal driver via
  the framework. **Also:** kernel code is inherently unsafe (can crash/brick, needs signing, strict
  context rules); "easy" means the framework absorbs plumbing and enforces rules, not that the danger
  disappears — documented plainly, not hand-waved.

## 10. Open Questions
- Q1. Backend host language — Rust+inkwell vs C++? (Phase 0 spike decides.)
- Q2. Do we ship an EPL `.e`/binary importer in v1 or defer? (Big interop win, big effort.)
- Q3. Runtime string model — keep GBK-era single-byte `char*` semantics, or move to UTF-8 with a compat
  shim? (Portability vs EPL fidelity.)
- Q4. How much Chinese localization (pinyin/lunar/RMB) is core vs an optional module?
- Q5. *(now core, not "when we get there")* Component/UI strategy — resolved in direction by D9 (one
  shared model, portable layer); open detail is exactly which layer (see Q9).
- Q9. **Which portable UI toolkit underpins the designer + runtime widgets** (the RAD foundation, Phase
  0 decision)? Candidates span native-wrapping toolkits and portable retained-mode/GPU layers; the pick
  drives look-and-feel, binary size, licensing, and how cleanly the EPL-style component ABI maps onto it.
- Q6. How far does release hardening go by default vs opt-in — is stripping + no-embedded-IR the floor,
  and where do control-flow flattening / packing / anti-debug sit on the level dial without tripping AV (M3)?
- Q7. Where exactly is the ease/power line drawn — what stays in the core easy language vs a library vs
  the `unsafe` tier? What's the smallest surface that still lets §5.6 work?
- Q8. First systems target for the G10 proof — Windows kernel driver (signing burden), a Linux kernel
  module, or UEFI/bare-metal (no signing, simpler to demo)?

## 11. Appendix — condensed ABI cheat-sheet

```
Library export:      OpenEPL_LibInfo* openepl_get_lib_info(void);   // ~ GetNewInf → LIB_INFO
Command impl:        void cmd(Slot* ret, int argc, Slot* argv);     // cdecl, EPL-compatible
Slot (MDATA_INF):    { DataType tag; union value; ... }
Types (SDT_*):       BYTE SHORT INT INT64 FLOAT DOUBLE DATE_TIME BOOL
                     TEXT(char*,NUL-term,NULL=empty)
                     BIN/byte-set(ptr→{INT 1; INT len; bytes}; NULL=empty)
                     SUB_PTR  STATMENT   (_NULL/_ALL for decls only)
Struct layout:       members aligned to 4 bytes.
Array layout:        ptr→{INT dims; INT dimSizes[dims]; data}
                       TEXT/BIN/composite: pointer array (elems may be NULL)
                       simple/window/menu: sequential elements
Access rule:         only the datum's real (unaligned) length is valid.
Runtime notifies:    NRS_MALLOC / NRS_MFREE / NRS_MREALLOC / NRS_FREE_ARY / NRS_RUNTIME_ERR / ...
Entry (exe):         E_Init(); ECodeStart();      Entry (dll): DllMain→E_Init/DllEntryFunc/E_DestroyRes
Runtime alloc:       E_MAlloc / E_MFree / E_MRealloc  (all EPL data owned by runtime)
```

---
*Primary sources archived in `docs/research/`. This PRD paraphrases public specs and BSD-3 open source;
it vendors no proprietary code.*
