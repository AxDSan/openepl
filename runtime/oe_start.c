/* Process entry. Provides `main`, which runs the lean EPL entry:
 *   E_Init(); ECodeStart();
 * Kept in its own object so alternate entries (WinMain/DllMain) can replace it
 * per build target without touching the command objects. */
#include "openepl_core.h"

int main(void) {
    E_Init();
    int rc = ECodeStart();
    E_DestroyRes();
    return rc;
}
