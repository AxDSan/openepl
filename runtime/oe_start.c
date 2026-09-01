/* Process entry. Provides `main`, which runs the lean EPL entry:
 *   E_Init(); ECodeStart();
 * Kept in its own object so alternate entries (WinMain/DllMain) can replace it
 * per build target without touching the command objects. */
#include "openepl_core.h"

int main(int argc, char **argv) {
    /* Captured here because this is the only place the program's arguments
     * exist.  A library target excludes this file, so `arg_count()` there
     * reports 0 rather than reading a pointer nobody set. */
    oe_set_args(argc, argv);
    E_Init();
    int rc = ECodeStart();
    E_DestroyRes();
    return rc;
}
