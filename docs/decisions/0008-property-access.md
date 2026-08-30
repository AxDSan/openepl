# ADR 0008 — Property access from code

**Status:** ✅ Accepted · **Date:** 2026-08-30 · **Follows:** ADR 0006

Lets code read and write component properties (`count_label.text = "1"`), which
is what makes an app *do* something rather than just display a fixed layout.

## Interning turned out to be unnecessary

ADR 0006 flagged that when code can address `button1.text`, component ids would
have to reach the runtime somehow, and proposed interning them to integers so
identifiers still never ship (G8).

**Interning is not needed.** Component creation order is fully static, so the
compiler already knows every component's runtime handle: the form root is `1`
and children follow in declaration order. `count_label` compiles to the integer
`2`. No name, no table, no runtime lookup — verified: emitted IR contains
`oe_ui_get(i64 2`, and none of `count_label`, `add_button`, `main_window` or
`on_add` appear as data.

`map_components` assigns the map in a pre-pass, before any subroutine is
lowered, because a handler may address a component declared after it.

## Syntax is plain assignment

`count_label.text = int_to_text(n)` as a statement, `count_label.text` in
expression position. No `set` keyword: forms already use bare `name = value`,
and G9 asks for prose, not ceremony. Component ids are **module-scoped** — they
come from the form and every subroutine can see them.

Types come from the introspected `ComponentDesc`, so unknown ids, unknown
properties, and assignment type mismatches are all compile errors.

## The form is now built before `main`

Previously `ECodeStart` called `main` and *then* built the form. Any property
access in `main` would have addressed components that did not exist — a
segfault appearing only for modules having both a `main` and a form. The entry
now builds the form, runs `main` as start-up code, then enters the loop.
`main_may_touch_components_before_the_loop_starts` guards it.

## Getters return runtime-owned copies

`oe_ui_get` previously returned a pointer into a single shared scratch buffer,
so `concat(a.text, b.text)` would have aliased — the second read clobbering the
first. It now returns memory allocated through the notification channel (D4),
freed with everything else at shutdown: the same ownership story as every other
text-returning command.

`oe_ui_get_int` was added so integer properties are parsed once at the boundary
rather than at every call site.

## Accessible names must track live text

Setting `text` now also updates the accessibility model's label. Without this
the accessible name stayed at its construction-time value and a screen reader
would announce stale content — the counter read `1` on screen while the
accessibility tree still said `0`. Caught by inspecting the a11y dump after a
click, and now asserted in `property_access_updates_a_component`.

This is a general lesson for D16: **a11y data is not a construction-time
snapshot.** Every property that feeds an accessible name must keep it in step.

## Deferred

Mutable variables and globals — the counter example stores its state in the
label's own text because there is nowhere else to put it yet, which is
expedient, not good. Also: computed left-hand sides, `form.title` at run time
(window-title plumbing is separate), property-change events, and properties on
components created dynamically rather than declared in a form.
