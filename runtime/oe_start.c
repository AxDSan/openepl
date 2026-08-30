/* Process entry. Provides `main`, which runs the lean EPL entry:
 *   E_Init(); ECodeStart();   (PRD §1.4)
 * Kept in its own object so alternate entries (WinMain/DllMain) can replace it
 * per build target (PRD G12) without touching the command objects. */
#include "openepl_core.h"

int main(void) {
    E_Init();
    int rc = ECodeStart();
    E_DestroyRes();
    return rc;
}
