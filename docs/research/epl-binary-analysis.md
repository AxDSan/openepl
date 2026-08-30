# EPL binary analysis (reverse-engineering notes) — English digest

**Source:** "易语言程序分析笔记" (EPL program analysis notes), Kanxue (看雪) forum author **PlaneJun**,
reposted on Zhihu. https://zhuanlan.zhihu.com/p/569792913 · Raw Chinese: `raw/zhihu.txt`

RE notes on how compiled EPL binaries look on disk/in a debugger. Useful to OpenEPL two ways: it
confirms the compile-mode mechanics from the *binary* side, and it's the reference for an EPL-binary
importer/analyzer (PRD G6) and for what our hardened output must *not* look like (PRD G8).

---

## 1. Identifying an EPL program

- File properties / resources (Resource Hacker) often reveal it.
- **Library files**: EPL depends on self-implemented libraries (`.fnr`) at runtime.
- **String fingerprints**: EPL is a self-written framework (MFC-like), so unprocessed binaries contain
  fixed telltale strings.

## 2. The four compile modes (from the binary side)

| Mode | Where libraries live at runtime |
|---|---|
| **Compile (编译)** | `.fnr` library files emitted **next to** the EXE; must ship together. |
| **Non-static (非静态编译)** | Libraries **embedded in the EXE image**; at `WinMain` the program reads its own image and **drops the libraries out**, then loads them. |
| **Standalone (独立编译)** | Only the EXE ships (like static), but at run time libraries are **written to a temp dir** and loaded. |
| **Static (静态编译)** | Static libs **merged into the EXE** (like a normal C program). Nothing dropped. Static addresses → analyzable by existing tools. |

## 3. Standard entry & how library calls are made

- **Non-static** standard entry: at `WinMain` the program reads its own content, drops out the library
  files, loads them, resolves a fixed function **`GetNewSock`**, then eventually does `call eax` where
  `eax` is the entry.
- **BlackMoon** entry: **optimizes away the early init code (including C-runtime init) and jumps
  straight to the EPL standard entry.** (Its binaries show two entry functions; a distinctive code
  pattern / string identifies the second.)
- **Library-function call ABI** — a variadic callback:
  ```
  E_FuncCallBack(argCount, arg1Value, arg1Ignorable, arg1Type, arg2Value, arg2Ignorable, arg2Type, ...)
  ```
  First param = number of args; then `(value, ignorable, type)` triples. `ignorable` is a bool
  (0 = not ignorable, 1 = ignorable). Types are the `SDT_*` tags.
- **Static vs non-static call difference:** static builds call by fixed static address. **Non-static
  builds pass a (library-ordinal, function-offset) pair in registers and call indirectly.** The
  **library ordinal** is the load order (0-based): the first library loaded (always `krnln`) is ordinal
  0 and is passed **implicitly** (so `krnln` calls show no explicit 0). This ordinal+offset indirection
  is a big part of why EPL binaries look odd to AV and are annoying to analyze.

## 4. Function/segment types

- **Message functions** (control/event handlers): reached via a fixed code stub; located by a signature
  pattern ending in a `call <local var>`; map to `case <id>:` in a big dispatch (e.g. `case 2008:`).
- **User-defined subroutines**: called like ordinary functions.
- **Library functions**: as in §3.
- **Component property functions**: two kinds — property set (`parent, child, propIndex, ~-1, value`)
  and the component's own methods (like message functions: `argCount, parentWindowID, id, argType`).
  Because controls live in `krnln`, the library ordinal is again implicit in non-static builds.

## 5. Basic data type on the wire (byte-set)

```c
struct ByteSet {   // 字节集
  int  unknown;    // constant == 1
  int  length;
  char* bytes;
};
```
(Matches the SDK's `SDT_BIN` layout `{INT 1; INT length; bytes}`.)

## 6. Analysis tooling mentioned

- Plugins that only work on **static-compiled** programs (static addresses).
- OD / IDA "EPL decompiler" plugins auto-identify functions and add an "EPL" toolbar that recognizes
  resource constants, forms, and message/control functions.
- **EDebug** can generate `.sig` signature files from EPL support libraries (drag the `.fne` in) to feed
  the analysis plugins.

## 7. Anti-analysis: junk instructions (花指令)

EPL can insert **junk/obfuscation instructions** at configurable levels (Tools → System Config →
Security; default level 1) to hinder reversing. But **each level's junk pattern is fixed**, so it's
scriptable to strip (IDA scripts, OD plugins) — feed the start address and range and remove it.

## 8. A cracking example (why hardening matters)

Loading a window uses the command `载入(,,)` with window type `0x10001`; you can find window loads by
searching `push 0x10001`, read the window ID, and **swap it to redirect which window opens** — trivially
defeating a window-gated flow. This is exactly the kind of thing OpenEPL's hardened, no-embedded-metadata
release output (PRD G8) is meant to prevent.

## 9. `krnln` command-offset map (excerpt)

The notes include a long table mapping `krnln` function offsets → command. It's a mechanical map (useful
for an importer that labels calls). A representative slice:

```
0x00 find-file        0x130 text-length       0x1EC datetime→text-part
0x02 add              0x134 text-left         0x210 current-time (get)
0x58 get-sign         0x138 text-right        0x214 current-time (set)
0x5C abs              0x13C text-mid          0x244 delete-file
0x60 trunc/floor      0x148 find-text         0x268 read-file
0x94 random           0x17C text-replace      0x26C write-to-file
0xC4 bitwise-and      0x190 split-text        0x270 open-file
0x100 command-line    0x194 byteset-length    0x288 lock-file
0x104 run-dir         0x198 to-byteset        0x28C seek-to-start
```
(Full offset list + component-ID appendix is in the raw file; reproduce it in the importer, not here.)

## Takeaways for OpenEPL

- The **ordinal+offset indirect call** and the **runtime library-drop** are the two "malware-shaped"
  behaviors BlackMoon removes and OpenEPL must never emit.
- The `E_FuncCallBack` `(count; value,ignorable,type)...` shape and the byte-set layout corroborate the
  SDK ABI and inform our slot design.
- The junk-instruction feature is dual-use: OpenEPL ships the **stripper/analyzer** (defensive) and
  treats *emitting* obfuscation as opt-in (PRD R3).
