# BlackMoon (黑月) compiler — English digest

**Source:** Original design writeup by the author, **云外归鸟 ("Yúnwài Guīniǎo")**, on his studio forum
(first post 2010-10-07, last edited 2018), plus the full version changelog.
http://www.ywgn.net/forum.php?mod=viewthread&tid=8 · Raw Chinese: `raw/ywgn.txt`

BlackMoon is a third-party **compiler plug-in** for EPL. This is the definitive first-person statement
of how it works. (Comparison point in the text: **斩月 "Zhǎnyuè"**, an earlier tool with the same goal —
shrink EPL programs and free them from support libraries — but a different method.)

---

## 1. Purpose

Make EPL programs small and **free of external support-library dependencies**. Zhanyue did this with a
runtime "shell/stub" (壳). BlackMoon does **not** use a shell.

## 2. How it works (the mechanism OpenEPL copies)

> BlackMoon **analyzes and converts the EPL program into object files (`.obj`), then re-compiles/links
> them with the linker `LINK.EXE`.**

Pipeline:

```
EPL e-code ──(BlackMoon analyze+translate)──► standard COFF .obj ──► LINK.EXE ──► standard PE (EXE/DLL)
                                                     ▲
                                    reimplemented core static lib (kernel.lib)
                                    + RC-compiled PE resources + optional C/MASM32 .lib
```

Consequences the author calls out:

1. **Standard PE structure.** The output is a conventional executable. After decompilation the EPL code
   is clearly visible; assembly is analyzable and debuggable with normal tools; **no more antivirus
   false positives.**
2. **Redirects core commands to a reimplemented core library.** Instead of loading EPL's `krnln` core
   support library, BlackMoon routes core commands to a **functionally-equivalent BlackMoon core static
   library that the author rewrote from scratch** — so most original core-library commands (the
   non-window ones) still work.
3. **Static-library fragment extraction ("抽取合成编译").** Because it's real static linking, BlackMoon
   **pulls in only the code fragments for commands the program actually uses.** Unused commands aren't
   merged into the target → no redundant code, no size bloat. (The author frames this as the thing EPL
   users had long dreamed of.)
4. **Standard PE resources via RC scripts.** UIs can be built from RC scripts (compatible with C/MASM32
   RC), e.g. dialog-template windows, with a visual BlackMoon RC editor.
5. **Real DLL entry.** BlackMoon DLLs get a `DllMain`-like entry ("Dll入口函数") → module handle access,
   injection, multithreading, PE-resource access.
6. **C ABI reach EPL lacked.** Supports `__cdecl` external functions and **linking C- or MASM32-written
   static libraries (`.lib`)** at compile time — big functional-extension and C-code-reuse win. Exposes
   `LINK.EXE`'s link parameters for special builds.

Also (from the RE analysis, `epl-binary-analysis.md`): BlackMoon **optimizes away EPL's init preamble**
(including C-runtime init), jumping straight into the EPL standard entry.

## 3. Known limitations (things OpenEPL should beat)

- **No native window/UI commands** — users hand-rolled custom-drawn ("自绘") UIs, or used the RC/dialog
  route. (Window operations were the non-portable part left out of the reimplemented core.)
- **32-bit x86, Windows-only.** Ties to `LINK.EXE`, VC6/MFC-era toolchains, x86 static libs.
- **Hand-maintained parity** with the native core library, command by command.

## 4. Behavioral parity is exacting (from the changelog)

The changelog (v1.1.0 2009-07 → v3.3.0 2013-09, plus later 4.x noted elsewhere) is dominated by
**matching the original `krnln` behavior command-for-command**, including reproducing EPL's own quirks.
Representative entries:

- Rewrote RNG so the sequence matches the native core library (first random not fixed to 1; even
  distribution over a range) — repeatedly tuned.
- Fixed `取文本中间` (substring) to return empty text when position < 1, "consistent with the E library."
- Noted the native `倒找文本` (reverse-find) has a *wrong* position parameter — and matched it.
- `四舍五入` (rounding), `增减时间` (date add/subtract, incl. cross-midnight and month-boundary),
  `取时间间隔`, `到数值`, `分割文本`/`分割字节集`, `子文本替换`/`子字节集替换`, `到全角`/`到半角`,
  `取随机数`, `取硬盘特征字` (disk fingerprint) — all iterated for exact fidelity, many in multithreaded-
  safety and memory-correctness terms.
- Progressive support added for compiling more official libraries: regex, BT-download, process-comm,
  LAN-ops, OpenGL, Java, and (via an **MFC static-library mode**, v3.0.0) DirectX 2D/3D and other
  MFC-based non-window libraries — "compile using the original static libs" to keep size down.

**Lesson for OpenEPL:** exact behavioral parity (including inherited bugs programs rely on) is the long
pole. Budget a conformance suite; document intentional divergences.

## 5. Toolchain/entry details corroborated by the open-source kernel

- Loader options were tunable via `BlackMoon.ini` (`BmLoaderOpt`); a VC6-style loader was used at times
  specifically to avoid AV false positives, at the cost of larger EXEs.
- CRT choice mattered: some libraries need C-runtime init (e.g. regex); link params like
  `/ENTRY:BMEntrypoint /nodefaultlib:LIBCMT /DEFAULTLIB:MSVCRT` switched the CRT/loader.
- DLL note: the EPL "startup subroutine" (`_启动子程序`) must run before the "Dll entry function," because
  at `DLL_PROCESS_ATTACH` the EPL global/class variables aren't initialized yet — the DLL entry is only
  for grabbing the module handle.

These match `BlackMoonExe.cpp` / `BlackMoonDll.cpp` in the open-source kernel (see
`blackmoon-kernel-repo.md`).

## 6. Attribution

Original author: **云外归鸟 (Yúnwài Guīniǎo)**. Later kernel升级/optimization credited to
泪闯天涯 (邓学彬 / Deng Xuebin) and 被封七号. The core static library was later open-sourced under
BSD-3-Clause (see `blackmoon-kernel-repo.md`).
