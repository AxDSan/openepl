# OpenEPL

An open-source, cross-platform, **RAD-first** application development environment in the VB/Delphi/EPL
lineage — draw an app, wire behavior to events, and build it into *any* desktop artifact (executable,
`.dll`/`.so`/`.dylib`, static lib, or `.sys`/`.ko` driver) for *any* platform — on top of an openly-
specified *e-code IR*, a **BlackMoon-style native backend** (IR → object files → system linker,
dead-code-lean, no runtime unpacking, hardened non-decompilable output), a portable core runtime, and a
documented support-library ABI. A reimagining of **EPL (易语言)** + the **BlackMoon (黑月)** compiler:
English-first, radically easy, cross-platform.

**Start here:** [`PRD.md`](./PRD.md) — full product requirements & the research behind them.

## Why
EPL programs normally ship as a small EXE plus `.fne`/`.fnr` support libraries loaded/unpacked at
runtime — which trips antivirus and resists debugging. **BlackMoon** (黑月) fixed that by translating
EPL's *e-code* into standard `.obj` files and linking them with `LINK.EXE` against a reimplemented
static core (`kernel.lib`), pulling in only the commands a program actually uses. OpenEPL rebuilds that
model as clean open source, retargetable (x64/arm64, Linux/macOS/Windows) instead of 32-bit-Windows-only.

## Layout
```
PRD.md              # the product requirements document (main deliverable)
docs/
  research/         # primary-source notes gathered during research (see index below)
```
Component dirs (`ir/ frontend/ backend/ runtime/ abi/ libs/ tools/ tests/`) are defined in the PRD §5
and land as implementation begins.

## Research index
All sources were Chinese; these are **English digests** preserving the full technical substance. Raw
Chinese originals are kept under `docs/research/raw/` for provenance.

- `docs/research/edk-support-library-sdk.md` — official EPL Support-Library SDK (EDK) manual, Delphi
  edition. The authoritative ABI: `GetNewInf`/`LIB_INFO`, `SDT_*` data types + storage layout,
  `DefineLib/Command/Const`, enums, ordinary types, UI-component interface functions. Source:
  https://www.dywt.com.cn/sdk/delphi/docs/
- `docs/research/blackmoon-design.md` — BlackMoon original author (云外归鸟) design writeup + condensed
  changelog. The definitive statement of the "translate e-code → obj → LINK.EXE + reimplemented static
  core + fragment extraction" model, plus limitations. Source: http://www.ywgn.net/forum.php?mod=viewthread&tid=8
- `docs/research/epl-binary-analysis.md` — reverse-engineering analysis (看雪/PlaneJun): compile modes,
  standard entry, `GetNewSock`, `E_FuncCallBack`, library ordinal+offset calls, `krnln` offset map, junk
  instructions. Source: https://zhuanlan.zhihu.com/p/569792913
- `docs/research/blackmoon-kernel-repo.md` — the **open-source** BlackMoon core static lib (`kernel.lib`):
  build, coding rules, entry model, `NRS_*` notification channel, tree shape (290 files, 244
  `krnln/*.cpp`). Our behavior oracle for the runtime. Source:
  https://github.com/zhongjianhua163/BlackMoonKernelStaticLib (BSD-3)
- `docs/research/tree.json` — raw file tree of that repo (data, not prose).
- `docs/research/raw/` — original Chinese source captures (`*.txt`).

## Legal
Built only from publicly published specs and the BSD-3-licensed BlackMoon `kernel.lib` (as a behavior
reference, reimplemented). No proprietary EPL/BlackMoon binaries or non-open source are vendored. See
PRD §9.
