/* Runtime lifecycle (PRD §1.4: E_Init / E_DestroyRes).
 * Spike stub: no heap/library table yet — that arrives with the ABI (Phase 2).
 */
#include "openepl_core.h"

void E_Init(void) {
    /* Future: grab process heap, run BlackMoonInitAllElib() analog. */
}

void E_DestroyRes(void) {
    /* Future: run each library's destroy hook, free runtime resources. */
}
