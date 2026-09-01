/* "system" library metadata (design-time only; compiled into the introspection
 * .so, never a shipped program — the same split as core_libinfo.c).
 *
 * Prefixes owned by this library: env_, os_, sys_. */
#include "openepl_abi.h"

#define SYS_CMD(n) void n(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv)
SYS_CMD(system_env_get);      SYS_CMD(system_env_has);      SYS_CMD(system_env_set);
SYS_CMD(system_env_unset);    SYS_CMD(system_env_count);    SYS_CMD(system_env_name_at);
SYS_CMD(system_os_name);      SYS_CMD(system_os_arch);      SYS_CMD(system_os_host_name);
SYS_CMD(system_os_user_name); SYS_CMD(system_os_home_dir);  SYS_CMD(system_os_temp_dir);
SYS_CMD(system_sys_arg_count);    SYS_CMD(system_sys_arg);
SYS_CMD(system_sys_program_path); SYS_CMD(system_sys_program_dir);
SYS_CMD(system_sys_process_id);   SYS_CMD(system_sys_tick_count);
SYS_CMD(system_sys_sleep_ms);     SYS_CMD(system_sys_quit);

static const int32_t P_T[]  = { OE_SDT_TEXT };
static const int32_t P_TT[] = { OE_SDT_TEXT, OE_SDT_TEXT };
static const int32_t P_I[]  = { OE_SDT_INT };

#define CMD(name, sym, ret, argc, args) { name, #sym, ret, argc, args }

static const OpenEPL_CommandDesc SYSTEM_COMMANDS[] = {
    /* the environment block */
    CMD("env_get",     system_env_get,     OE_SDT_TEXT, 1, P_T),
    CMD("env_has",     system_env_has,     OE_SDT_BOOL, 1, P_T),
    CMD("env_set",     system_env_set,     OE_SDT_BOOL, 2, P_TT),
    CMD("env_unset",   system_env_unset,   OE_SDT_BOOL, 1, P_T),
    CMD("env_count",   system_env_count,   OE_SDT_INT,  0, NULL),
    CMD("env_name_at", system_env_name_at, OE_SDT_TEXT, 1, P_I),
    /* the machine and the account */
    CMD("os_name",      system_os_name,      OE_SDT_TEXT, 0, NULL),
    CMD("os_arch",      system_os_arch,      OE_SDT_TEXT, 0, NULL),
    CMD("os_host_name", system_os_host_name, OE_SDT_TEXT, 0, NULL),
    CMD("os_user_name", system_os_user_name, OE_SDT_TEXT, 0, NULL),
    CMD("os_home_dir",  system_os_home_dir,  OE_SDT_TEXT, 0, NULL),
    CMD("os_temp_dir",  system_os_temp_dir,  OE_SDT_TEXT, 0, NULL),
    /* this process */
    CMD("sys_arg_count",    system_sys_arg_count,    OE_SDT_INT,   0, NULL),
    CMD("sys_arg",          system_sys_arg,          OE_SDT_TEXT,  1, P_I),
    CMD("sys_program_path", system_sys_program_path, OE_SDT_TEXT,  0, NULL),
    CMD("sys_program_dir",  system_sys_program_dir,  OE_SDT_TEXT,  0, NULL),
    CMD("sys_process_id",   system_sys_process_id,   OE_SDT_INT,   0, NULL),
    CMD("sys_tick_count",   system_sys_tick_count,   OE_SDT_INT64, 0, NULL),
    CMD("sys_sleep_ms",     system_sys_sleep_ms,     OE_SDT_NULL,  1, P_I),
    CMD("sys_quit",         system_sys_quit,         OE_SDT_NULL,  1, P_I),
};

static const OpenEPL_LibInfo SYSTEM_INFO = {
    OPENEPL_ABI_VERSION,
    "system",
    "openepl-system-0000-0000-0000-73797374656d",
    0, 1, 0,
    (int32_t)(sizeof(SYSTEM_COMMANDS) / sizeof(SYSTEM_COMMANDS[0])),
    SYSTEM_COMMANDS,
    0, NULL,   /* the system library contributes no visual components */
};

const OpenEPL_LibInfo *openepl_get_lib_info(void) {
    return &SYSTEM_INFO;
}
