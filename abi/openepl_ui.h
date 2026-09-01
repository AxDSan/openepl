/* OpenEPL widget-backend interface — the D10 boundary.
 *
 * THE LEAK RULE: this header must contain **no substrate types whatsoever** —
 * no RmlUi, no SDL, no GL. Widgets are opaque integer handles. If a substrate
 * type ever appears here, the substrate choice (D14) silently stops being
 * reversible, which is the single failure mode calls load-bearing.
 *
 * Everything below is plain C so that code emitted by the OpenEPL backend can
 * call it directly. The current implementation lives in `libs/ui/` (C++ over
 * RmlUi); a different backend can be dropped in behind this same header.
 *
 * v0 scope (Phase 2 RAD half): build a form, set properties by name, bind
 * events to function pointers, run the loop. Property values are textual — this
 * is internal plumbing, not the command slot ABI; typed values arrive with the
 * property descriptor tags when code can read/write properties (Phase 3).
 */
#ifndef OPENEPL_UI_H
#define OPENEPL_UI_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* Opaque widget handle. 0 is the null handle. */
typedef uint64_t OpenEPL_Widget;

/* An event handler emitted by the compiler. Handlers are bound by FUNCTION
 * POINTER, never by name: no name-based dispatch table exists at runtime, so no
 * user identifier reaches the shipped binary (G8). */
typedef void (*OpenEPL_EventFn)(void);

/* Lifecycle. `oe_ui_init` creates the window/context; returns 0 on success. */
int  oe_ui_init(const char *title, int width, int height);
void oe_ui_shutdown(void);

/* Create a component by its registered type name (e.g. "button"), parented to
 * `parent` (0 = the form root). Returns 0 on failure. */
OpenEPL_Widget oe_ui_create(OpenEPL_Widget parent, const char *type_name);

/* The form root, valid after oe_ui_init. */
OpenEPL_Widget oe_ui_root(void);

/* Set/get a property by name. Values are textual at this boundary in v0.
 *
 * `oe_ui_get` returns runtime-owned memory (allocated through the notification
 * channel, D4) and is therefore safe to hold and to nest — `concat(a.text,
 * b.text)` does not alias. It is freed with everything else at shutdown.
 * Returns NULL if the property is unset. */
int         oe_ui_set(OpenEPL_Widget w, const char *property, const char *value);
const char *oe_ui_get(OpenEPL_Widget w, const char *property);

/* Integer-typed convenience getter, so callers do not each re-parse text. */
int32_t     oe_ui_get_int(OpenEPL_Widget w, const char *property);

/* Bind an event by name (generic vocabulary, e.g. "click") to a handler. */
int oe_ui_on(OpenEPL_Widget w, const char *event, OpenEPL_EventFn handler);

/* Accessibility (D16): every widget carries a role plus an accessible name.
 * The AccessKit bridge lands in Phase 3; these record the intent now so the
 * information exists when the bridge is built, rather than being retrofitted. */
int oe_ui_set_a11y(OpenEPL_Widget w, int32_t role, const char *name);

/* Register the window as an event source and enter the runtime's loop, which
 * returns once nothing is left alive. Returns the exit code.
 *
 * The loop belongs to the runtime, not to this library: a program may hold a
 * window and a timer at once, and two loops could not both be entered. Closing
 * the window still ends the program rather than merely retiring this source.
 *
 * Test hooks (honoured only when the corresponding environment variable is set,
 * so they cost nothing in a shipped app):
 *   OPENEPL_UI_EXIT_AFTER_FRAMES=<n>   render n frames, then return
 *   OPENEPL_UI_SYNTH_CLICK=<handle>    dispatch a synthetic click to that widget
 *                                      (a handle, not an id — ids never ship)
 *   OPENEPL_UI_DUMP=<path.ppm>         write the framebuffer before exiting
 *   OPENEPL_UI_MOUSE=<x,y>             place the mouse (drives hover states)
 *   OPENEPL_UI_DUMP_A11Y=1             print the accessibility tree
 * These make the GUI testable headlessly in CI; see libs/ui/README.md. */
int oe_ui_run(void);

#ifdef __cplusplus
}
#endif
#endif /* OPENEPL_UI_H */
