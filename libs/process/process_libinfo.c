/* "process" library metadata (design-time only; compiled into the
 * introspection .so, never a shipped program — the same split as
 * core_libinfo.c). */
#include "openepl_abi.h"

void process_run(OpenEPL_Slot *, int32_t, OpenEPL_Slot *);
void process_run_capture(OpenEPL_Slot *, int32_t, OpenEPL_Slot *);
void process_start(OpenEPL_Slot *, int32_t, OpenEPL_Slot *);
void process_read_line(OpenEPL_Slot *, int32_t, OpenEPL_Slot *);
void process_at_end(OpenEPL_Slot *, int32_t, OpenEPL_Slot *);
void process_write_line(OpenEPL_Slot *, int32_t, OpenEPL_Slot *);
void process_is_running(OpenEPL_Slot *, int32_t, OpenEPL_Slot *);
void process_wait(OpenEPL_Slot *, int32_t, OpenEPL_Slot *);
void process_kill(OpenEPL_Slot *, int32_t, OpenEPL_Slot *);
void process_close(OpenEPL_Slot *, int32_t, OpenEPL_Slot *);
void process_close_all(OpenEPL_Slot *, int32_t, OpenEPL_Slot *);

static const int32_t P_T[]  = { OE_SDT_TEXT };
static const int32_t P_I[]  = { OE_SDT_INT };
static const int32_t P_IT[] = { OE_SDT_INT, OE_SDT_TEXT };

static const OpenEPL_CommandDesc PROCESS_COMMANDS[] = {
    { "process_run",         "process_run",         OE_SDT_INT,  1, P_T  },
    { "process_run_capture", "process_run_capture", OE_SDT_TEXT, 1, P_T  },
    { "process_start",       "process_start",       OE_SDT_INT,  1, P_T  },
    { "process_read_line",   "process_read_line",   OE_SDT_TEXT, 1, P_I  },
    { "process_at_end",      "process_at_end",      OE_SDT_BOOL, 1, P_I  },
    { "process_write_line",  "process_write_line",  OE_SDT_BOOL, 2, P_IT },
    { "process_is_running",  "process_is_running",  OE_SDT_BOOL, 1, P_I  },
    { "process_wait",        "process_wait",        OE_SDT_INT,  1, P_I  },
    { "process_kill",        "process_kill",        OE_SDT_BOOL, 1, P_I  },
    { "process_close",       "process_close",       OE_SDT_BOOL, 1, P_I  },
    { "process_close_all",   "process_close_all",   OE_SDT_INT,  0, 0    },
};

static const OpenEPL_LibInfo PROCESS_INFO = {
    OPENEPL_ABI_VERSION,
    "process",
    "openepl-process-0000-0000-0000-000000000008",
    0, 1, 0,
    (int32_t)(sizeof(PROCESS_COMMANDS) / sizeof(PROCESS_COMMANDS[0])),
    PROCESS_COMMANDS,
    0, 0,
};

const OpenEPL_LibInfo *openepl_get_lib_info(void) {
    return &PROCESS_INFO;
}
