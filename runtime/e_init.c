/* Runtime lifecycle (PRD §1.4: E_Init / E_DestroyRes).
 *
 * Phase 2: E_Init prepares the runtime; E_DestroyRes releases all runtime-owned
 * allocations.  Per-library init dispatch (the BlackMoonInitAllElib analog that
 * hands each linked library the notify callback) is deferred — spike libraries
 * are stateless and reach the runtime through the global `oe_notify` symbol
 * (ADR 0003). */
#include "openepl_core.h"

void oe_free_all(void); /* from oe_mem.c */

void E_Init(void) {
    /* Nothing to acquire yet; the allocator is lazily initialized. */
}

void E_DestroyRes(void) {
    oe_free_all();
}
