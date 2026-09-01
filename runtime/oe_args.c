/* The program's command-line arguments.
 *
 * Captured by main() in oe_start.c, which is the only place they exist, and
 * read by the `system` library's `arg_count` / `arg` commands.  A library
 * target excludes oe_start.c, so nothing sets these there and the count is 0 —
 * which is honest, rather than a pointer nobody initialised.
 */
#include "openepl_core.h"

static int    g_argc = 0;
static char **g_argv = NULL;

void oe_set_args(int argc, char **argv) {
    g_argc = argc;
    g_argv = argv;
}

int oe_arg_total(void) { return g_argc; }

const char *oe_arg_at(int i) {
    if (!g_argv || i < 0 || i >= g_argc) return NULL;
    return g_argv[i];
}
