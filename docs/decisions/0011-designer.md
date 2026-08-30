# ADR 0011 — The visual designer (Phase 3 RAD vertical slice)

**Status:** ✅ Accepted · **Date:** 2026-08-30 · **Delivers:** PRD §7 Phase 3, metric **M0**

The product's first real proof: draw an app, wire an event, get a running native
binary.

## The designer is a C++ RmlUi app

D18 says the IDE dogfoods RmlUi, and dogfooding means **full substrate access** —
documents, DOM queries, event capture — which the D10 shim deliberately does not
expose. So the designer links RmlUi directly.

**D19 still holds where it matters:** every compiler-side seam stays in Rust;
only the UI shell is C++, because that is what dogfooding requires.

**Rejected: writing the designer in OpenEPL itself.** That is the right long-term
answer — a RAD tool built in its own language is the strongest possible proof —
but the language cannot host it yet: no arrays, no dynamic component creation, no
file I/O. Recorded as the aspiration, not pretended to be a plan.

## The designer never parses `.oir`

`openepl inspect` is its only reader. A second grammar in C++ would drift from
the Rust one, and the drift would surface as the designer silently misreading
files it did not write. Output is line-based (`component:`, `prop:`, `handler:`,
`sub:`, `form: … span=A..B`) so neither side needs a serialisation library.

## Saving splices; it never regenerates the file

The parser records each form's source line span. Saving writes: *original lines
before the form* + *regenerated form block* + *original lines after it*, with
stubs for newly wired handlers appended at the end.

**This is the single most important decision in the designer.** Re-emitting the
whole module from the designer's model — the obvious implementation — would
delete every hand-written subroutine body the moment a user nudged a button.
`designer/test_model.cpp` asserts a hand-written body with comments survives a
save **byte-identical**, and it was written and passing before any window
existed.

## One mapping, shared with the runtime

`libs/ui/ui_mapping.h` holds the component→RmlUi mapping (tags, underscore→
hyphen, px suffixing, the D21 seed stylesheet, font candidates). The runtime
backend and the designer canvas both include it. Two copies would mean a
component rendering one way in the designer and another in the built app —
exactly the WYSIWYG drift D9 exists to prevent. Designer-only affordances (the
selection outline) layer on top; they never replace the shared mapping.

## The descriptors table finally meets its consumer

The designer links `ui_libinfo.c` directly, so the toolbox, the inspector's field
list, and the wirable events all come from the *same* metadata the compiler
introspects. This is the `.fne` design-time / `.fnr` runtime split (D12) working
as intended: one table, two consumers, no duplication.

## Scripted sessions for testing

`OPENEPL_DESIGNER_SCRIPT="add:button;set:text=Press me;wire:click=on_press;save"`
runs a session headlessly and exits — the same env-hook pattern already proven
for the UI runtime. The M0 test drives it, then **compiles and runs the result**,
asserting the designed button reaches its handler. Testing that the designer
produced *plausible text* would prove nothing; testing that the real compiler
accepts it proves everything.

## Closing the window saves

There is no dialog layer to ask "save changes?" with, and discarding a user's
work silently is far worse than an unexpected write, so the designer tracks a
dirty flag and **saves on exit**, announcing it on stdout. Found by watching a
real session add two components and lose both on close.

The scripted path applies the identical rule, so a scripted session can never
behave differently from the interactive one — a divergence there would make
tests prove something other than what users experience.

A proper "save / discard / cancel" prompt supersedes this once the designer has
a dialog layer.

## Editing model

- **Undo/redo** keeps whole-model snapshots rather than reversible commands. The
  model is a few dozen strings, so copying it is cheaper than describing every
  edit as an invertible operation — and a snapshot cannot drift from what it
  claims to undo. One snapshot per *gesture*, not per mouse-move, so a drag
  undoes in one step.
- **Run tracks its child process**, so Stop can actually stop it. `system()`
  gives no handle on the child, which is why Stop could only ever say "nothing
  running". The loop reaps the child if it exits on its own, so the status stays
  honest.
- **Alignment guides** snap a dragged component flush to a neighbour's edge or
  centre line when within 6px, and draw the line that claimed it. Grid snapping
  applies only when no neighbour did, so guides always win over the grid — the
  designer feels precise rather than approximate.
- **Multi-select** (ctrl-click) shows a plain outline on secondary selections
  and handles on the primary. Copy/paste re-identifies pasted components so ids
  stay unique, and offsets them so the copies are visible.
- Every outline measures the component's **rendered border box** through one
  shared helper, so a padded component (groupbox) cannot get an outline that
  disagrees with what is painted.

## Known debt, stated rather than hidden

- **The designer's own chrome is not accessible.** It bypasses the D10 layer that
  carries the a11y model, so AccessKit sees nothing. This is real debt against
  D16 and is called out here rather than left to be discovered: *apps built with
  OpenEPL are accessible; the tool that builds them is not yet.*
- Property values are re-quoted heuristically (bare digits stay numeric, anything
  else is quoted). The descriptor's declared type is the authority and should
  drive this instead.
- No undo/redo, resize handles, multi-select, copy/paste, snapping, nested
  containers, or in-designer code editing (users edit subroutine bodies in their
  own editor — the splice is what makes that safe).
- Drag-to-move commits on every mousemove; a real implementation would coalesce.

## Scope deliberately excluded

New component types, native file dialogs (the project path is argv), live
preview beyond the canvas itself, and project/session management.
