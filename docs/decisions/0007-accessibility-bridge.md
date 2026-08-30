# ADR 0007 — The AccessKit accessibility bridge

**Status:** ✅ Accepted · **Date:** 2026-08-30 · **Implements:** ADR 0005/D16

Custom-drawn UI (D14) yields **zero** free accessibility, and FireMonkey is the
cautionary case — still shipping a11y as a separate add-on 14 years in. D16 said
build it in from day one. This is that.

## The model is a thread boundary, not just a layer

AccessKit's own header states: *"All of the handlers will always be called from
another thread."* RmlUi is **not** thread-safe, so an adapter callback must never
touch a widget.

`libs/ui/a11y_model.{h,cpp}` is the barrier: a mutex-guarded snapshot
(`id → parent, role, label, bounds, clickable`) that the main thread publishes
and the adapter thread only reads. It also holds the D10 line — no substrate
types and no AccessKit types — so `ui_rmlui.cpp` and `a11y_accesskit.cpp` never
include each other's headers.

Actions arrive on the adapter thread and are **queued**, then drained and
dispatched by the UI loop on the main thread. Verified 1:1: one AT-SPI
`do_action` produces exactly one action callback and exactly one dispatch.

**Widget handle == AccessKit node id.** Both are `u64` and deliberately the same
value, so no mapping table exists to drift.

## Vendored prebuilt, not built from source

`tools/fetch-accesskit.sh` pulls the pinned **0.22.3** release (prebuilt Linux
x86_64 static lib + headers) into gitignored `vendor/`, mirroring the RmlUi
pattern. This avoids requiring a cargo + cbindgen toolchain to build a GUI app.

Reading the shipped header mattered: the setter is `accesskit_node_set_label`,
not `set_name` — the sort of thing that is renamed across versions and must not
be recalled from memory.

## Degrades to nothing

No accessibility bus, no D-Bus, adapter creation fails, or `OPENEPL_NO_A11Y=1`
→ the app runs identically, logging one line to stderr. Guarded by
`app_runs_with_accessibility_disabled`. An app that breaks when accessibility
infrastructure is absent would fail the very requirement the bridge exists for.

`accesskit_unix_adapter_update_if_active` does nothing until an AT connects, so
publishing every frame is cheap.

## Verified against a real screen-reader stack

Not just unit-tested. With the a11y bus enabled
(`busctl --user set-property org.a11y.Bus … org.a11y.Status IsEnabled b true`),
`at-spi2-registryd` starts, our app appears on the bus as an application, and an
AT-SPI client sees:

```
application: 'form'
  frame: 'OpenEPL — Phase 2'
    label: ''
    button: 'Click me'        ← n_actions=1
```

Invoking the button's default action through AT-SPI reaches the OpenEPL handler
subroutine. **A screen reader can operate an OpenEPL app.**

## Known limitation: window-relative bounds on Wayland

Node bounds are window-relative; `accesskit_unix_adapter_set_root_window_bounds`
translates them to screen coordinates, and we call it. But AccessKit documents —
and Wayland enforces — that *a client cannot learn its own window position*, so
on Wayland the offset is `0,0` and accessible coordinates remain window-relative.
Correct on X11. This is a platform limitation, not something the bridge can fix;
recorded so it is not rediscovered as a bug.

## An unexplained observation, recorded honestly

While driving the app through AT-SPI, more `click` handler invocations were seen
than actions dispatched (e.g. 1 action → 1 dispatch → 8 handler fires), while a
control run with no AT client produced **zero**. The a11y path itself is
demonstrably 1:1; the extra events enter through the substrate's ordinary input
path, which is consistent with the AT stack synthesizing pointer clicks at the
node's reported (window-relative, hence wrong-on-Wayland) coordinates.

**Not proven.** It is plausible and consistent with the Wayland limitation above,
but it was not root-caused. Flagged here rather than left as folklore, and worth
re-testing under X11 where coordinates are correct.

## Testing, two tiers

- **Always-on:** `OPENEPL_UI_DUMP_A11Y=1` prints the model tree;
  `accessibility_tree_is_published` asserts roles, names, parent links and
  non-zero bounds. No bus needed, so it runs anywhere.
- **Skip-if-unavailable:** `accessibility_adapter_activates_on_a_real_bus`
  activates the session a11y bus and checks the adapter goes live. Skips cleanly
  when the session has no accessibility bus — never a hard CI dependency.

## Deferred

Focus tracking beyond reporting a focused node; text/value/live-region
semantics; keyboard navigation; Windows (UIA) and macOS (NSAccessibility)
adapters — AccessKit supports both, and the model is already platform-neutral;
roles beyond window/button/label.
