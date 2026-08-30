# OpenEPL component model — specification (v0, Phase 2)

> Promised as a Phase 0 deliverable (PRD §7) and written here, once the model was
> real rather than speculative. Companion to [`ir.md`](ir.md) and [`abi.md`](abi.md).

## 1. The one primitive

The whole RAD loop rests on a single idea, borrowed from Delphi (where
`published` members get RTTI) and from EPL's UI interface functions:

> **A component's properties and events are enumerable and settable by string
> name at run time.**

From that one primitive fall out form streaming, the Object Inspector, the
designer, and event wiring — each of them generic code with no per-component
knowledge. OpenEPL carries this in the **same `LibInfo` mechanism** that already
describes commands (ADR 0005/D11), so a library contributes components exactly
the way it contributes commands.

## 2. Declaring a component (the ABI side)

```c
typedef struct { const char *name; int32_t tag; const char *default_value; } OpenEPL_PropertyDesc;
typedef struct { const char *name; } OpenEPL_EventDesc;

typedef struct {
    const char                 *name;        /* surface type name, e.g. "button" */
    int32_t                     a11y_role;   /* OE_ROLE_* — accessibility (D16)  */
    int32_t                     property_count;
    const OpenEPL_PropertyDesc *properties;
    int32_t                     event_count;
    const OpenEPL_EventDesc    *events;
} OpenEPL_ComponentDesc;
```

Components are listed in `OpenEPL_LibInfo.components`. As with commands, this
table lives in a **metadata-only translation unit** compiled into the
introspection `.so` and **never linked into a shipped program** (ADR 0003/D12).

**Accessibility is part of the descriptor, not an afterthought** (ADR 0005/D16):
every component states its role, and per-instance accessible names travel with
its properties. Custom-drawn UI gets no accessibility for free, so the data must
exist from the start — the AccessKit bridge that consumes it lands in Phase 3.

## 3. Declaring a form (the IR side)

```text
form     := "form" IDENT NEWLINE member* "end" NEWLINE
member   := property | binding | component
property := IDENT "=" expr NEWLINE
binding  := "on" IDENT ":" IDENT NEWLINE
component:= IDENT IDENT NEWLINE (property | binding)* "end" NEWLINE
```

```text
module hello_form
use ui

form main_window
  title  = "OpenEPL"
  width  = 480
  height = 300

  button ok_button
    text = "Click me"
    left = 40
    top  = 110
    on click: on_ok_click
  end
end

sub on_ok_click
  call print_text("button clicked!")
end
```

Property names use **underscores** (`background_color`), consistent with the rest
of the language; the UI backend translates to whatever the substrate spells them
(RCSS hyphens, today). Property values are literals in v0.

The validator checks every component type, property name, property value type,
and event name against the introspected descriptors, and requires each bound
handler to be a real subroutine — so a typo is a compile error, not a silently
missing widget.

## 4. What reaches the binary

| | Ships? |
|---|---|
| Component **type** names (`"button"`) | yes — generic vocabulary |
| Property names (`"text"`) and values | yes — the substrate needs them |
| Event names (`"click"`) | yes — generic vocabulary |
| **Component ids** (`ok_button`) | **no** — compile-time only |
| **Handler names** (`on_ok_click`) | **no** — bound by function pointer |

Handlers are wired by **function pointer**; there is no name-based dispatch
table at run time, so no user identifier is emitted as data (G8). Accessible
names come from user-facing *text*, never from an identifier — a component with
no text gets a role and no name rather than leaking its id.

**Known limitation:** when code can read and write `button1.text` (Phase 3), ids
will need to reach the runtime somehow. The intended answer is to intern them to
integers at compile time so identifiers still never ship. Not solved here
because v0 does not need it (ADR 0006).

## 5. Entry points

| Module | Entry |
|---|---|
| declares a `form` | `ECodeStart` builds the form, wires handlers, runs the UI loop. `main`, if present, runs first as start-up code. |
| no form | `ECodeStart` calls `main` — the console path, unchanged. |

Each subroutine lowers to its own `@oe_user_<name>` function so a handler can be
taken by address.

## 6. The widget-backend boundary (D10)

The runtime speaks [`abi/openepl_ui.h`](../../abi/openepl_ui.h): opaque
`uint64_t` handles, create-by-type-name, set/get-property-by-name, bind-event-to-
function-pointer, plus `oe_ui_set_a11y`. **That header contains no substrate
types at all** — no RmlUi, no SDL, no GL. Swapping substrate means replacing one
file (`libs/ui/ui_rmlui.cpp`).

This is load-bearing: if substrate types ever leak through this header, the
substrate choice silently stops being reversible.

## 7. Deferred

Property read/write from code; components created dynamically at run time (all
components come from the form today); layout containers; more than the three
built-in components (`form`, `button`, `label`); the designer; the AccessKit
bridge; data binding (ADR 0005/D17).
