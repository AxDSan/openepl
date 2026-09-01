/* The "system" support library — the environment the program runs in.
 *
 * Three families, one per prefix:
 *   env_*  the environment block          (get / has / set / unset / enumerate)
 *   os_*   the machine and the account    (name, arch, host, user, home, temp)
 *   sys_*  this process                   (arguments, path, pid, clock, exit)
 *
 * Running another program deliberately does NOT live here; that belongs to the
 * `process` library.  This library only answers questions about the process it
 * is already inside.
 *
 * "openepl_core.h" is included rather than the ABI header alone because the
 * program's arguments (oe_arg_total / oe_arg_at) and the "" text sentinel
 * (oe_empty_text) are runtime internals, not part of the third-party ABI.
 */
#define _POSIX_C_SOURCE 200809L
#include <errno.h>
#include <pwd.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#include <unistd.h>
#include "openepl_core.h"

#if defined(__APPLE__)
#include <mach-o/dyld.h>
#endif

/* POSIX guarantees this; declaring it here rather than relying on a
 * feature-test macro keeps the translation unit portable across libcs. */
extern char **environ;

/* --- small helpers ---------------------------------------------------- */

/* Every text result is runtime-owned, like every other text value in the
 * program, so a caller can hold a failed result without a special case. */
static char *sys_dup_text(const char *s) {
    if (!s) return oe_empty_text();
    size_t n = strlen(s);
    char *out = (char *)oe_malloc((long)n + 1);
    if (!out) return NULL;
    memcpy(out, s, n + 1);
    return out;
}

/* Fail with `code`/`msg` and return the "" text sentinel. */
static void sys_fail_text(OpenEPL_Slot *ret, int32_t code, const char *msg) {
    oe_error_set(code, msg);
    oe_ret_text(ret, oe_empty_text());
}

static void sys_fail_text_errno(OpenEPL_Slot *ret, int e, const char *what) {
    oe_error_set_errno(e, what);
    oe_ret_text(ret, oe_empty_text());
}

/* A name that is empty or contains '=' cannot address a variable at all — that
 * is a mistake in the program, not a missing variable, so it is a failure
 * rather than a "no". */
static int sys_bad_env_name(const char *name) {
    return !name || name[0] == '\0' || strchr(name, '=') != NULL;
}

/* --- env_* ------------------------------------------------------------ */

/* env_get(text name) -> text.
 *
 * An unset variable is a "no", not a failure: the slot is CLEARED and "" comes
 * back.  That makes "" ambiguous between unset and set-to-empty, which is
 * exactly why env_has ships beside it — the same pairing as file_at_end next to
 * file_read_line. */
void system_env_get(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv) {
    (void)argc;
    const char *name = oe_arg_text(argv, 0);
    if (sys_bad_env_name(name)) {
        sys_fail_text(ret, OE_ERR_INVALID_ARG, "env_get: bad variable name");
        return;
    }
    const char *v = getenv(name);
    oe_error_clear();
    oe_ret_text(ret, sys_dup_text(v));
}

/* env_has(text name) -> bool.  false with code 0 means genuinely absent. */
void system_env_has(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv) {
    (void)argc;
    const char *name = oe_arg_text(argv, 0);
    if (sys_bad_env_name(name)) {
        oe_error_set(OE_ERR_INVALID_ARG, "env_has: bad variable name");
        oe_ret_bool(ret, 0);
        return;
    }
    const char *v = getenv(name);
    oe_error_clear();
    oe_ret_bool(ret, v != NULL);
}

/* env_set(text name, text value) -> bool.  Overwrites. */
void system_env_set(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv) {
    (void)argc;
    const char *name = oe_arg_text(argv, 0);
    const char *value = oe_arg_text(argv, 1);
    if (sys_bad_env_name(name)) {
        oe_error_set(OE_ERR_INVALID_ARG, "env_set: bad variable name");
        oe_ret_bool(ret, 0);
        return;
    }
    if (!value) value = "";
    int rc = setenv(name, value, 1);
    int e = errno;
    if (rc != 0) {
        oe_error_set_errno(e, "env_set");
        oe_ret_bool(ret, 0);
        return;
    }
    oe_error_clear();
    oe_ret_bool(ret, 1);
}

/* env_unset(text name) -> bool.  Removing something already absent succeeds:
 * the post-condition asked for is "not set", and it holds. */
void system_env_unset(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv) {
    (void)argc;
    const char *name = oe_arg_text(argv, 0);
    if (sys_bad_env_name(name)) {
        oe_error_set(OE_ERR_INVALID_ARG, "env_unset: bad variable name");
        oe_ret_bool(ret, 0);
        return;
    }
    int rc = unsetenv(name);
    int e = errno;
    if (rc != 0) {
        oe_error_set_errno(e, "env_unset");
        oe_ret_bool(ret, 0);
        return;
    }
    oe_error_clear();
    oe_ret_bool(ret, 1);
}

static int sys_env_total(void) {
    int n = 0;
    if (!environ) return 0;
    while (environ[n]) n++;
    return n;
}

/* env_count() -> int.  A collection is a count plus an indexed accessor; this
 * one cannot fail, so it never touches the error slot. */
void system_env_count(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv) {
    (void)argc; (void)argv;
    oe_ret_int(ret, sys_env_total());
}

/* env_name_at(int index) -> text : the NAME of the i-th variable, 0-based.
 * The value is then env_get(name) — the accessor deliberately does not return
 * "NAME=value", which a program would only have to take apart again. */
void system_env_name_at(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv) {
    (void)argc;
    int i = oe_arg_int(argv, 0);
    if (i < 1 || i > sys_env_total()) {
        sys_fail_text(ret, OE_ERR_INVALID_ARG, "env_name_at: index out of range");
        return;
    }
    const char *entry = environ[i - 1];
    const char *eq = strchr(entry, '=');
    size_t n = eq ? (size_t)(eq - entry) : strlen(entry);
    char *out = (char *)oe_malloc((long)n + 1);
    if (!out) { sys_fail_text(ret, OE_ERR_TABLE_FULL, "env_name_at: out of memory"); return; }
    memcpy(out, entry, n);
    out[n] = '\0';
    oe_error_clear();
    oe_ret_text(ret, out);
}

/* --- os_* ------------------------------------------------------------- */

/* os_name() -> text : "linux", "macos", "windows", or "unknown".
 * Decided at compile time, so it cannot fail and never touches the slot. */
void system_os_name(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv) {
    (void)argc; (void)argv;
#if defined(_WIN32)
    const char *s = "windows";
#elif defined(__APPLE__)
    const char *s = "macos";
#elif defined(__linux__)
    const char *s = "linux";
#else
    const char *s = "unknown";
#endif
    oe_ret_text(ret, sys_dup_text(s));
}

/* os_arch() -> text : the CPU this binary was built for. */
void system_os_arch(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv) {
    (void)argc; (void)argv;
#if defined(__x86_64__) || defined(_M_X64)
    const char *s = "x86_64";
#elif defined(__aarch64__) || defined(_M_ARM64)
    const char *s = "aarch64";
#elif defined(__i386__) || defined(_M_IX86)
    const char *s = "x86";
#elif defined(__arm__)
    const char *s = "arm";
#elif defined(__riscv) && __riscv_xlen == 64
    const char *s = "riscv64";
#else
    const char *s = "unknown";
#endif
    oe_ret_text(ret, sys_dup_text(s));
}

/* os_host_name() -> text.  "" on failure. */
void system_os_host_name(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv) {
    (void)argc; (void)argv;
    char buf[256];
    buf[0] = '\0';
    int rc = gethostname(buf, sizeof buf);
    int e = errno;
    if (rc != 0) { sys_fail_text_errno(ret, e, "os_host_name"); return; }
    buf[sizeof buf - 1] = '\0';   /* truncation may leave it unterminated */
    oe_error_clear();
    oe_ret_text(ret, sys_dup_text(buf));
}

/* The account's name and home come from the environment first, because that is
 * what a login shell set and what the user would expect a program to honour,
 * and from the password database only when it is missing. */
void system_os_user_name(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv) {
    (void)argc; (void)argv;
    const char *v = getenv("USER");
    if (!v || !*v) v = getenv("LOGNAME");
    if (v && *v) { oe_error_clear(); oe_ret_text(ret, sys_dup_text(v)); return; }
    errno = 0;
    struct passwd *pw = getpwuid(getuid());
    int e = errno;
    if (!pw || !pw->pw_name) {
        if (e) sys_fail_text_errno(ret, e, "os_user_name");
        else   sys_fail_text(ret, OE_ERR_UNSUPPORTED, "os_user_name: no such user");
        return;
    }
    oe_error_clear();
    oe_ret_text(ret, sys_dup_text(pw->pw_name));
}

void system_os_home_dir(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv) {
    (void)argc; (void)argv;
    const char *v = getenv("HOME");
    if (v && *v) { oe_error_clear(); oe_ret_text(ret, sys_dup_text(v)); return; }
    errno = 0;
    struct passwd *pw = getpwuid(getuid());
    int e = errno;
    if (!pw || !pw->pw_dir || !*pw->pw_dir) {
        if (e) sys_fail_text_errno(ret, e, "os_home_dir");
        else   sys_fail_text(ret, OE_ERR_UNSUPPORTED, "os_home_dir: no home directory");
        return;
    }
    oe_error_clear();
    oe_ret_text(ret, sys_dup_text(pw->pw_dir));
}

/* os_temp_dir() -> text.  There is always an answer — the environment's, or the
 * platform default — so this cannot fail and never touches the slot. */
void system_os_temp_dir(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv) {
    (void)argc; (void)argv;
    const char *v = getenv("TMPDIR");
    if (!v || !*v) v = getenv("TMP");
    if (!v || !*v) v = getenv("TEMP");
    if (!v || !*v) v = "/tmp";
    oe_ret_text(ret, sys_dup_text(v));
}

/* --- sys_* ------------------------------------------------------------ */

/* sys_arg_count() -> int : how many command-line arguments, including the
 * program name at index 0.  A library target links no main(), so nothing
 * captured the arguments and this reports 0 — honest, rather than reading a
 * pointer nobody set.  It cannot fail, so it never touches the slot. */
void system_sys_arg_count(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv) {
    (void)argc; (void)argv;
    /* The count of REAL arguments, so `sys_arg(1)` .. `sys_arg(sys_arg_count())`
     * is the whole range and the program name is not quietly one of them. */
    int total = oe_arg_total();
    oe_ret_int(ret, total > 0 ? total - 1 : 0);
}

/* sys_arg(int index) -> text.  Index 0 is the program name as invoked.
 * Out of range is a failure rather than a silent "", because "" is itself a
 * perfectly legal argument value. */
void system_sys_arg(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv) {
    (void)argc;
    int i = oe_arg_int(argv, 0);
    /* sys_arg(1) is the first real argument.  argv[0] is the program name and
     * is NOT reachable here — sys_program_path() already reports it, and one
     * way to say a thing is enough. */
    const char *v = i < 1 ? 0 : oe_arg_at(i);
    if (!v) { sys_fail_text(ret, OE_ERR_INVALID_ARG, "sys_arg: index out of range"); return; }
    oe_error_clear();
    oe_ret_text(ret, sys_dup_text(v));
}

/* The running binary's own path, resolved from the OS rather than from argv[0],
 * which the caller controls and may not be a path at all. */
static int sys_exe_path(char *buf, size_t cap, int *saved_errno) {
    *saved_errno = 0;
#if defined(__linux__)
    ssize_t n = readlink("/proc/self/exe", buf, cap - 1);
    int e = errno;
    if (n < 0) { *saved_errno = e; return 0; }
    buf[n] = '\0';
    return 1;
#elif defined(__APPLE__)
    uint32_t size = (uint32_t)cap;
    if (_NSGetExecutablePath(buf, &size) != 0) { *saved_errno = ENAMETOOLONG; return 0; }
    buf[cap - 1] = '\0';
    return 1;
#else
    (void)buf; (void)cap;
    return -1;   /* no way to ask on this platform */
#endif
}

void system_sys_program_path(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv) {
    (void)argc; (void)argv;
    char buf[4096];
    int e = 0;
    int rc = sys_exe_path(buf, sizeof buf, &e);
    if (rc < 0) { sys_fail_text(ret, OE_ERR_UNSUPPORTED, "sys_program_path: unsupported platform"); return; }
    if (rc == 0) { sys_fail_text_errno(ret, e, "sys_program_path"); return; }
    oe_error_clear();
    oe_ret_text(ret, sys_dup_text(buf));
}

/* sys_program_dir() -> text : the directory holding the binary, with no
 * trailing separator (except at the root, where "/" is the directory). */
void system_sys_program_dir(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv) {
    (void)argc; (void)argv;
    char buf[4096];
    int e = 0;
    int rc = sys_exe_path(buf, sizeof buf, &e);
    if (rc < 0) { sys_fail_text(ret, OE_ERR_UNSUPPORTED, "sys_program_dir: unsupported platform"); return; }
    if (rc == 0) { sys_fail_text_errno(ret, e, "sys_program_dir"); return; }
    char *slash = strrchr(buf, '/');
    if (!slash) { oe_error_clear(); oe_ret_text(ret, sys_dup_text(".")); return; }
    if (slash == buf) buf[1] = '\0'; else *slash = '\0';
    oe_error_clear();
    oe_ret_text(ret, sys_dup_text(buf));
}

/* sys_process_id() -> int.  Always available; never touches the slot. */
void system_sys_process_id(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv) {
    (void)argc; (void)argv;
    oe_ret_int(ret, (int32_t)getpid());
}

/* sys_tick_count() -> int64 : milliseconds from a monotonic clock, for timing.
 * The origin is arbitrary and only differences are meaningful — that is the
 * point: unlike now(), it cannot go backwards when the wall clock is adjusted.
 * -1 on failure. */
void system_sys_tick_count(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv) {
    (void)argc; (void)argv;
    struct timespec ts;
    int rc = clock_gettime(CLOCK_MONOTONIC, &ts);
    int e = errno;
    if (rc != 0) {
        oe_error_set_errno(e, "sys_tick_count");
        oe_ret_int64(ret, -1);
        return;
    }
    oe_error_clear();
    oe_ret_int64(ret, (int64_t)ts.tv_sec * 1000 + (int64_t)(ts.tv_nsec / 1000000));
}

/* sys_sleep_ms(int ms) : void.  A negative or zero wait returns at once.
 * Signals are absorbed by resuming the remaining time, so this waits for the
 * duration asked for; it has no way to report failure and never touches the
 * error slot. */
void system_sys_sleep_ms(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv) {
    (void)argc; (void)ret;
    int ms = oe_arg_int(argv, 0);
    if (ms <= 0) return;
    struct timespec req;
    req.tv_sec = ms / 1000;
    req.tv_nsec = (long)(ms % 1000) * 1000000L;
    struct timespec rem;
    while (nanosleep(&req, &rem) != 0 && errno == EINTR) req = rem;
}

/* sys_quit(int code) : void, and does not return.  Runtime teardown runs first,
 * exactly as it does on the normal path out of main(), so a program that quits
 * early still closes its handles and frees its data. */
void system_sys_quit(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv) {
    (void)argc; (void)ret;
    int code = oe_arg_int(argv, 0);
    E_DestroyRes();
    exit(code);
}
