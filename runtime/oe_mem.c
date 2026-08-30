/* Runtime-owned memory (PRD D4). Phase-1 spike: thin malloc wrappers; the
 * NRS_MALLOC/NRS_MFREE notification channel and reclamation land in Phase 2. */
#include <stdlib.h>
#include "openepl_core.h"

void *oe_alloc(long size) {
    void *p = malloc(size > 0 ? (size_t)size : 1);
    return p; /* out-of-memory handling arrives with the runtime error channel */
}

void oe_free(void *p) {
    free(p);
}
