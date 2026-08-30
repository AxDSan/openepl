# EPL Support-Library SDK (EDK) — English digest

**Source:** Official EPL Support-Library Development Kit manual, Delphi edition, v1.0 (2008-05), by
Dalian Dayou Wutao E-Language Software (大连大有吴涛易语言软件开发有限公司).
https://www.dywt.com.cn/sdk/delphi/docs/ · Raw Chinese: `raw/official-edk-sdk.txt`

This is the authoritative description of the EPL support-library (支持库) ABI — the contract OpenEPL's
`abi/` mirrors. Written for Delphi, but the binary contract is language-neutral.

---

## 1. What a support library is

Support libraries are *the* extension mechanism of EPL — almost all functionality (even core math/text/
file/date commands) lives in them, not in the language. A library integrates other languages' and the
OS's capabilities into EPL. The Delphi EDK exists because filling the `LIB_INFO` struct by hand in C/C++
is tedious; the EDK wraps it in helper procedures.

Caveat noted in the manual: Delphi/VCL can't fully fuse into EPL, so there are limits when building
libraries in Delphi.

## 2. The one required export

A support library is a Windows DLL (extension `.fne` design-time / `.fnr` runtime) whose **only hard
requirement** is to export **`GetNewInf()`**, returning a pointer to a fully-populated **`LIB_INFO`**
record:

```pascal
function GetNewInf() : pLIB_INFO; stdcall; export;
begin
  result := GetLibInfo();   // GetLibInfo() returns the struct the EDK helpers built up
end;
exports GetNewInf;
```

A minimal, empty-but-valid library is just `DefineLib(...)` in the project's `begin` block — it loads in
EPL but has no commands. The `{$E fne}` directive sets the output extension.

## 3. `DefineLib()` — library metadata

```pascal
DefineLib(
  'myfne', '{5CAFDDB6-22E7-4B27-823A-A80A3919189F}',   // szName, szGuid (stable, unique per library)
  'A simple demo EPL support library written in Delphi.', // szExplain
  1, 0, 1, __GBK_LANG_VER,   // major, minor, build, language
  0,                          // dwState (0 default; _LIB_OS(__OS_WIN) preset)
  'author', 'www.example.com', '',  // szAuthor, szHomePage, szOther
  2, '0000Category1'#0'0000Category2'#0,  // category count, category names (see below)
  nil, nil,   // pfnRunAddInFn, szzAddInFnInfo   (add-in/plugin entry)
  nil,        // pfnNotifyLib  (system-notification callback; nil = default handler)
  nil,        // szzDependFiles (other files this library depends on)
  nil );      // pfnFreeLibData (called on unload; "may be temporarily ineffective")
```

- **GUID**: one stable GUID per library, never changed across versions.
- **Version**: shown as `a.b#c` (major.minor#build); every release bumps at least one; a change that
  alters help docs bumps at least the minor.
- **Categories**: commands are shown grouped (like the core lib's "Flow control", "Arithmetic", "Text
  ops"). Each category name's **first 4 chars must be 4 digits** = the icon index (`0000` = default).
  The list is a `#0`-separated, double-`#0`-terminated string. Commands reference a category by 1-based
  index.
- The last five params can all be `nil`.

## 4. Data types (`SDT_*`) and their storage — the core ABI

Every value slot carries a numeric type tag. Basic types:

| EPL type | Tag | Delphi/C storage |
|---|---|---|
| byte 字节型 | `SDT_BYTE` | BYTE |
| short 短整数型 | `SDT_SHORT` | SHORT |
| int 整数型 | `SDT_INT` | INT (32-bit) |
| long 长整数型 | `SDT_INT64` | INT64 |
| float 小数型 | `SDT_FLOAT` | FLOAT |
| double 双精度小数型 | `SDT_DOUBLE` | DOUBLE |
| bool 逻辑型 | `SDT_BOOL` | BOOL |
| datetime 日期时间型 | `SDT_DATE_TIME` | OLE DATE |
| text 文本型 | `SDT_TEXT` | pointer to NUL-terminated string; `NULL` = empty string |
| byte-set 字节集 | `SDT_BIN` | pointer to `{INT const=1; INT length; <length> bytes}`; `NULL` = empty |
| sub pointer 子程序指针 | `SDT_SUB_PTR` | subroutine pointer |
| statement 条件语句型 | `SDT_STATMENT` | conditional-statement type |

Two special tags — `_SDT_NULL` (no type) and `_SDT_ALL` (any type) — are used only when declaring a
command's return/param types. The core library's own types are referenced via `DTP_*` constants; a
library's own types are referenced as `typeIndex + 1`. **A library cannot reference another (non-core)
library's types.**

### Storage layout (must be honored exactly by IR + codegen)

**Non-array values:** simple types stored as their C type; `SDT_TEXT`/`SDT_BIN` as pointers (see above);
window-unit/menu types = `{DWORD windowTemplateID; DWORD unit/menuItemID}`.

**Composite (user/library struct, non-window):** a non-NULL pointer to members laid out in order,
**each member aligned to 4 bytes**. Example `{byte A; short B; int C}` → 12 bytes: A@0, B@4, C@8.

**Arrays:** a non-NULL pointer to:
1. one INT = dimension count (>0),
2. that many INTs = per-dimension element counts (each >0),
3. the data:
   - `SDT_TEXT`/`SDT_BIN`/composite → a **pointer array** (each element pointer may be `NULL`),
   - simple types + window-unit/menu → elements laid out **sequentially** (as in the non-array case).

**Access-length rule:** when accessing any datum, only touch its *actual* (non-aligned) length — because
when an array element is passed by reference to another subroutine, only the element's real bytes are
valid (array elements are **not** 4-byte-aligned). E.g. reading a byte member: assume only that one byte
is accessible.

## 5. Constants — `DefineConst()`

```pascal
DefineConst('Const1', 'const1', 'a text constant', CT_TEXT, 0, 'the value');
DefineConst('Const2', 'const2', 'a numeric constant', CT_NUM, 1999, nil);
DefineConst('Const3', 'const3', 'a bool constant', CT_BOOL, 1, nil);
```
Params: name, English name, description, type (`CT_TEXT`/`CT_NUM`/`CT_BOOL`), numeric value, text value.
Used in EPL as `#ConstName`. The manual advises preferring **enums** over loose constants (constants
pollute the global namespace).

## 6. Commands (子程序 / library functions) — `DefineCommand()`

```pascal
DefineCommand(Command1, Command1_Args, 'Command1', 'command1', 'returns sum of two args',
              SDT_INT, 0, LVL_SIMPLE, 1);
DefineCommand(Command2, [], 'Command2', 'command2', 'returns true',
              SDT_BOOL, 0, LVL_SIMPLE, 1);
```
Params: implementation fn, argument-info array, name, English name, description, **return type**, state,
difficulty **level** (`LVL_SIMPLE`/`LVL_SECONDARY`/`LVL_HIGH`), **category** (1-based).

State flags include `CT_IS_HIDED`, `CT_IS_ERROR`, `CT_DISABLED_IN_RELEASE`,
`CT_ALLOW_APPEND_NEW_ARG` (variadic-style extra args), `CT_RETRUN_ARY_TYPE_DATA`. `_CMD_OS(__OS_WIN)`
is preset.

### Argument info — `ARG_INFO`

```pascal
const Command1_Args: array[0..1] of ARG_INFO =
( (m_szName:'arg1'; m_szExplain:''; m_shtBitmapIndex:0; m_shtBitmapCount:0;
   m_dtDataType:SDT_INT; m_nDefault:0; m_dwState:0),
  (m_szName:'arg2'; ...; m_dtDataType:SDT_INT; m_nDefault:0; m_dwState:0) );
```
Members: name, description, bitmap index, bitmap count, **data type**, default value, state.
Default value is only honored when state has `AS_HAS_DEFAULT_VALUE`. Argument state flags:
`AS_HAS_DEFAULT_VALUE`, `AS_DEFAULT_VALUE_IS_EMPTY`, `AS_RECEIVE_VAR` (by-reference),
`AS_RECEIVE_VAR_ARRAY`, `AS_RECEIVE_VAR_OR_ARRAY`, `AS_RECEIVE_ARRAY_DATA`,
`AS_RECEIVE_ALL_TYPE_DATA`, `AS_RECEIVE_VAR_OR_OTHER`.

### Implementation function — fixed signature

```pascal
procedure Command1(pRetData: pMDATA_INF; nArgCount: Integer; pArgInf: pMDATA_INF); cdecl;
begin
  pRetData.m_Value.m_int := ArgArray(pArgInf)[0].m_Value.m_int
                          + ArgArray(pArgInf)[1].m_Value.m_int;
end;
```
- **Always `cdecl`.**
- `pRetData` → the return slot: assign into its `m_Value.m_<type>` member to return a value.
- `nArgCount` → the actual number of args received (matters for `CT_ALLOW_APPEND_NEW_ARG` commands and
  for commands that gained params in a later version).
- `pArgInf` → array of `MDATA_INF` arg slots; cast to `ArgArray` for indexing.
- Each `MDATA_INF` has `m_dtDataType` (the type tag) and a `m_Value` union; if the type is fixed read/
  write the known member, else branch on `m_dtDataType`.

## 7. Enum types — `DefineEnumDatatype()` + `DefineEnumElement()`

```pascal
idx := DefineEnumDatatype('Enum1', 'Enum1', 'about Enum1', 0);   // returns type index
DefineEnumElement(idx, 'A', 'constA', '', 1001, 0);
DefineEnumElement(idx, 'B', 'constB', '', 1002, 0);
```
Enum members are integer constants. Used in EPL as `#EnumName.MemberName`. `LDT_ENUM` and
`_DT_OS(__OS_WIN)` are preset in state.

## 8. Ordinary types (methods + data members) — `DefineDatatype()` etc.

A non-window type with methods and data members (no window handle, no properties/events, not visually
designable).

```pascal
idx := DefineDatatype('Type1', 'Datatype1', 'about Datatype1', 0);
DefineMethod(idx, Type1_Method1, [], 'Method1', 'Method1', '', SDT_INT, 0, LVL_SIMPLE);
DefineElement(idx, 'intMember', 'element1', '', SDT_INT, nil, 0, 0);
DefineElement(idx, 'textMember','element2', '', SDT_TEXT, nil, 0, 0);
```
- `DefineMethod()` is like `DefineCommand()` but with a leading type-index param. **Key difference:** a
  method's `pArgInf[0]` holds the object's own data (the `self`/`this`), so the real args start at index
  1 — meaning `nArgCount` for a method is always *actual arg count + 1*.
- `DefineElement()` data members are freely readable/writable in EPL code; neither definer nor user
  worries about storage layout.

## 9. Window/UI components — `DefineUIDatatype()` + interface functions

Window components have a handle, properties, methods, events, a toolbox icon, and visual design support.
Much more involved. Sketch of the model (Delphi-specific but reveals the EPL↔component contract):

- Subclass the VCL control you're wrapping (e.g. `TMyPanel = class(TPanel)`), storing EPL-provided IDs:
  `FWinFormID` (containing window ID), `FUnitID` (the control's own ID), `FInDesignMode`.
- Route window messages through the EDK helper: `WndProc` → `ELib_WinControlWndProc(self, msg,
  CallBaseWndProc)`; `CallBaseWndProc` → `inherited WndProc(msg)`. (Both bodies are boilerplate.)
- Implement and register a set of **interface functions** the EPL system calls (only the first is
  mandatory; the first four are usually needed):
  - **OnCreate** `(pAllData, nAllDataSize, dwStyle, hParentWnd, uID, hMenu, x,y,cx,cy, dwWinFormID,
    dwUnitID, hDesignWnd, blInDesignMode) : HUNIT stdcall` — called whenever the control is
    instantiated (dropped from toolbox, preview, program run, opening a `.e`). Typically calls
    `ELib_CreateControl(...)`, records the IDs, and `LoadProperties(pAllData, nAllDataSize)`. `pAllData`
    is `nil` on first drop-from-toolbox, otherwise carries serialized props.
  - **OnGetProperty** `(hUnit, nIndex, pValue) : EBool stdcall` — return the value of property `nIndex`
    (0-based, excluding built-in props like name/tag/visible). Uses `ELib_GetControl(hUnit)` then a big
    `case` writing into `pValue`.
  - **OnSetProperty** `(hUnit, nIndex, pValue, ppszTipText) : EBool stdcall` — apply a property value to
    the control/UI. Return `ETrue` **only if the control must be recreated**, else `EFalse`.
  - **SaveProperties → HGLOBAL** — pack all property values into a custom-format memory block returned
    as an `HGLOBAL`; fed back as `pAllData` on the next create. (`LoadProperties` is the inverse.)
- Helper functions used throughout: `ELib_CreateControl`, `ELib_GetControl`, `ELib_ToEBackColor` /
  `ELib_FromEBackColor`, `ELib_WinControlWndProc`, etc. `EBool` is EPL's bool (`ETrue`/`EFalse`).

## 10. Takeaways for OpenEPL

- The **whole design-time + runtime surface is reachable from one struct** returned by one export. Our
  ABI keeps that shape: one `openepl_get_lib_info()` → `OpenEPL_LibInfo`.
- The **command impl signature** (`void cmd(ret, argc, argv)`, cdecl, typed slots) and the **method
  `self`-as-arg-0** convention are cheap to reproduce and worth keeping for familiarity.
- The **storage ABI** (§4) is the part codegen must get bit-exact: 4-byte member alignment, byte-set
  `{1,len,bytes}`, array `{dims, sizes, data}`, pointer-arrays for text/bin/composite, the access-length
  rule.
- The **UI interface-function model** (OnCreate/OnGet/OnSetProperty/SaveProperties + a `self`-carrying
  handle) is the template for OpenEPL's later UI track — but we'd define it against a portable widget
  layer, not VCL.
- The **system-notification callback** (`pfnNotifyLib`) is the runtime↔library back-channel; see the
  BlackMoon design digest for the concrete message set (`NRS_MALLOC`, etc.).
