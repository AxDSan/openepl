# BlackMoon core static library (open source) — English digest

**Source:** `zhongjianhua163/BlackMoonKernelStaticLib` — "黑月核心静态库", the open-sourced core static
library (`kernel.lib`) of the BlackMoon compiler plug-in. **BSD-3-Clause.**
- GitHub: https://github.com/zhongjianhua163/BlackMoonKernelStaticLib
- Gitee mirror: https://gitee.com/zhongjianhua163/BlackMoonKernelStaticLib
- Raw README: `raw/blackmoon-kernel-readme.txt` · Full file tree: `tree.json`
- Stats at capture: **290 files, 244 `krnln/*.cpp`**, 117 commits, 159★/64 forks.

This is the single most important artifact for OpenEPL: a real, buildable, open **reimplementation of
EPL's `krnln` core** in C/C++, and our behavior oracle for `runtime/` (`libopenepl_core`). We
**reimplement from it, never vendor its source** into non-BSD code (also, it's Win32/x86-specific).

---

## 1. What it is

The C/C++ source for BlackMoon's `kernel.lib` — the reimplemented core library that BlackMoon links into
compiled programs in place of EPL's `krnln`. Building it produces `kernel.lib`, which is dropped into the
EPL install at `\BlackMoon\obj\kernel.lib` (BlackMoon 4.0+) or `\BlackMoon\lib\kernel.lib` (pre-4.0).

## 2. Build

1. Open the project file matching your installed VS version.
2. Three solutions inside: **`krnln`**, **`krnln_Obj`**, **`MFCBlackMoon`**. Normally you only build
   `krnln` (Release) — the other two are special-purpose.
3. Copy the resulting `kernel.lib` into the BlackMoon install path above.

Projects are provided for **VC6 and VS2019** (`.dsp` and `.vcxproj`), with CI workflows
(`.github/workflows/blackmoon_krnln.yml`, `_krnlnobj.yml`, `_mfc.yml`).

## 3. Coding rules (they reveal design constraints)

- Must compile on **all VS versions**; gate version-specific features with `_MSC_VER`.
- **Avoid inline assembly**; if unavoidable, avoid new instruction sets (SSE/AVX) or provide CPU-feature-
  detected fallbacks so ancient 32-bit CPUs still run. (→ their portability ceiling; OpenEPL sidesteps
  this via LLVM.)
- **Every command's params, return value, and behavior must match EPL's native core library exactly.**
- Source files must be saved as **ANSI/GB2312** (not UTF-8); git `autoCRLF=false`.

## 4. Architecture, from the source (corroborates the BlackMoon design digest)

### Entry points — `krnln/BlackMoonExe.cpp`
```c
extern "C" int ECodeStart();     // the translated USER PROGRAM, emitted as an .obj by BlackMoon
int _cdecl BMEntrypoint() { E_Init(); ECodeStart(); return 0; }        // lean entry (no CRT preamble)
int WINAPI  WinMain(...)  { E_Init(); /* save esp/ebp */ call ECodeStart; return eax; }
int         main(...)     { E_Init(); /* save esp/ebp */ call ECodeStart; return eax; }
```
`ECodeStart()` is the boundary between the reimplemented runtime and the user program. Both a GUI
(`WinMain`) and console (`main`) entry exist; `BMEntrypoint` is the lean no-CRT variant.

### Runtime init/teardown — `krnln/EyInit.cpp`
```c
void _cdecl E_Init()      { hBlackMoonHeap = GetProcessHeap(); BlackMoonInitAllElib(); }
void _cdecl E_DestroyRes(){ /* call each lib's destroy hook */ BlackMoonFreeAllElib(); }
```
`E_Init` grabs the process heap and initializes every linked support library; `E_DestroyRes` runs each
library's destroy hook and frees them. Memory is the runtime's job (`E_MAlloc/E_MFree/E_MRealloc`).

### DLL entry — `krnln/BlackMoonDll.cpp`
```c
BOOL __stdcall DllMain(hModule, reason, reserved) {
  DLL_PROCESS_ATTACH: hBlackMoonInstanceHandle = hModule; E_Init(); DestroyAddress = DllEntryFunc();
  DLL_PROCESS_DETACH: E_DestroyRes();
  return 1;
}
```

### Runtime↔library notification channel — `krnln/BlackMoonLibNotifySys.cpp`
The concrete implementation of the SDK's `pfnNotifyLib`. `BlackMoonFuncForeLibNotifySys(nMsg, dwParam1,
dwParam2)` dispatches:
- `NRS_MALLOC` → `E_MAlloc(size)` (dwParam2≠0 means "return NULL on failure" vs abort)
- `NRS_MFREE` → `E_MFree(ptr)`
- `NRS_MREALLOC` → `E_MRealloc(ptr, size)`
- `NRS_FREE_ARY` → free array data by `SDT_*` type (text/bin via `FreeAryElement`, simple via `E_MFree`)
- `NRS_RUNTIME_ERR` → raise a runtime error with a message string
- (plus more)

**This is how memory ownership and error propagation stay consistent across the EXE and every library** —
all EPL-data allocation flows through the runtime. OpenEPL's `abi/` reproduces this channel (D4).

## 5. What's in the tree (shape)

```
krnln/            244 .cpp — the reimplemented core commands, grouped by area, e.g.:
  BlackMoonExe.cpp BlackMoonDll.cpp BlackMoonDll2.cpp BlackMoonResDll.cpp BlackMoonCallUserDll.cpp
  BlackMoonCallPropertyVaule.cpp BlackMoonLibNotifySys.cpp
  EyInit.cpp EyComInit.cpp DllEntryFunc.cpp
  FileManager.cpp MyMemFile.cpp                      (file / memory-file I/O)
  DateTimeFormat.cpp GetDatePart.cpp GetTimePart.cpp GetSpecTime.cpp GetWeekDay.cpp
    GetDaysOfSpecMonth.cpp                            (date/time)
  CloneTextData.cpp CloneBinData.cpp krnln_BinLeft.cpp SDataToStr.cpp   (text / byte-set)
  NumToChinese.cpp PY.OBJ Diskid32.obj               (RMB/pinyin localization, disk-id — prebuilt objs)
  FreeAryElement.cpp GetAryElementInf.cpp            (array runtime)
  GetDataTypeType.cpp GetSysDataTypeDataSize.cpp     (type system)
  Myfunctions.cpp eHelpFunc.cpp HelpFunc12.cpp ...
MFCObj/           MFC-based build (BlackMoonMFCdll.cpp, EyMFCComInit.*, MFCBlackMoon.*) for MFC libs
Project/          VC6 + VS2019 project files for krnln, krnln_Obj, MFCBlackMoon
```
Note `PY.OBJ`, `Diskid32.obj` ship as prebuilt objects (pinyin table, disk fingerprint) — the parts
awkward to open-source as source.

## 6. Attribution / license

Original author: **云外归鸟 (Yúnwài Guīniǎo)**; later upgrades **泪闯天涯 (邓学彬)**, optimization
**被封七号**. **BSD-3-Clause** — the compiled `kernel.lib` may be linked into commercial works; on
redistribution the source must credit the repo URLs. Topics tagged `blackmoon`, `c-plus-plus`,
`e-program-language`, `epl`.

## Takeaways for OpenEPL

- Use this as the **behavior oracle** for `libopenepl_core`: read it to learn what each core command
  must do (including inherited quirks), then **reimplement** cross-platform (no inline x86 asm, no Win32
  assumptions, UTF-8-capable), organized **one command per object** for linker dead-stripping.
- The entry model (`E_Init(); ECodeStart();`), the DLL hooks, and the `NRS_*` notification channel port
  directly into our design (PRD D4, §5.2, §5.3).
- Keep provenance clean: reimplement, don't copy; our runtime can be MIT/BSD without vendoring GB2312/
  Win32 source.
