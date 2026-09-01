/* The "process" support library — running other programs.
 *
 * Two ways to run something, and the difference is the whole design:
 *
 *   process_run / process_run_capture   run it to completion, hand back what
 *                                       it said or how it ended;
 *   process_start                        keep it alive behind a handle and
 *                                        talk to it line by line.
 *
 * The distinction this file is most careful about is "could not start" versus
 * "ran and failed".  A program that mistakes one for the other retries a
 * missing binary forever, or reports a compiler's honest exit 1 as a broken
 * installation.  So a child is spawned through an errno pipe: the child writes
 * its exec failure into a close-on-exec pipe, and the parent reading a value
 * there knows the program never ran.  "Could not start" is -1 (or handle 0)
 * with the error slot set; "ran and failed" is the real exit status with the
 * slot clear.
 *
 * Only the SDK header — nothing runtime-internal.
 */
#include <errno.h>
#include <fcntl.h>
#include <signal.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#include <unistd.h>
#include <sys/types.h>
#include <sys/wait.h>

#include "openepl_abi.h"

/* --- small helpers ---------------------------------------------------- */

static const char *process_nz(const char *s) { return s ? s : ""; }

/* Text results are runtime-owned like every other text result, so even the ""
 * failure sentinel is a fresh runtime allocation. */
static char *process_empty_text(void) {
    char *s = (char *)oe_malloc(1);
    if (s) s[0] = '\0';
    return s;
}

static char *process_dup_text(const char *s, long n) {
    char *o = (char *)oe_malloc(n + 1);
    if (!o) return process_empty_text();
    memcpy(o, s, (size_t)n);
    o[n] = '\0';
    return o;
}

/* A wait status turned into the number a shell would report. */
static int32_t process_status_of(int wstatus) {
    if (WIFEXITED(wstatus)) return (int32_t)WEXITSTATUS(wstatus);
    if (WIFSIGNALED(wstatus)) return (int32_t)(128 + WTERMSIG(wstatus));
    return -1;
}

/* Writing to a child that has closed its input raises SIGPIPE, which would
 * kill the *parent* — a program should get `false` and an error code instead.
 * Disposition is process-wide, so it is set once, lazily, when this library
 * first spawns anything. */
static void process_ignore_sigpipe(void) {
    static int done = 0;
    if (!done) { signal(SIGPIPE, SIG_IGN); done = 1; }
}

/* --- the child --------------------------------------------------------- */

typedef struct {
    pid_t pid;
    int   in_fd;     /* write end of the child's stdin, -1 once closed  */
    FILE *out;       /* buffered read end of the child's stdout          */
    int   reaped;    /* the child has been waited for; `status` is real  */
    int32_t status;
    int   at_end;    /* stdout reached end of input                      */
} Proc;

/* Plain malloc, not oe_malloc: this is library bookkeeping, not program data,
 * and it must stay valid while the handle table's close functions run at exit,
 * after program data has been freed. */

/* Spawn `command` under /bin/sh.
 *
 * `in_fd`/`out_fd` are optional: pass NULL for a stream the child should
 * inherit from us instead of having piped.  Returns the pid, or -1 with
 * *err_out set to the errno that explains why it never ran. */
static pid_t process_spawn(const char *command, int *in_fd, int *out_fd, int *err_out) {
    int inp[2] = { -1, -1 }, outp[2] = { -1, -1 }, errp[2] = { -1, -1 };
    *err_out = 0;

    process_ignore_sigpipe();

    if (pipe(errp) != 0) { *err_out = errno; return -1; }
    /* Close-on-exec: a successful exec closes it, and the parent's read
     * returning 0 bytes is precisely "the program started". */
    if (fcntl(errp[1], F_SETFD, FD_CLOEXEC) != 0) {
        int e = errno;
        close(errp[0]); close(errp[1]);
        *err_out = e;
        return -1;
    }
    if (in_fd && pipe(inp) != 0) {
        int e = errno;
        close(errp[0]); close(errp[1]);
        *err_out = e;
        return -1;
    }
    if (out_fd && pipe(outp) != 0) {
        int e = errno;
        close(errp[0]); close(errp[1]);
        if (in_fd) { close(inp[0]); close(inp[1]); }
        *err_out = e;
        return -1;
    }

    pid_t pid = fork();
    if (pid < 0) {
        int e = errno;
        close(errp[0]); close(errp[1]);
        if (in_fd)  { close(inp[0]);  close(inp[1]); }
        if (out_fd) { close(outp[0]); close(outp[1]); }
        *err_out = e;
        return -1;
    }

    if (pid == 0) {                              /* --- child --- */
        close(errp[0]);
        if (in_fd)  { close(inp[1]);  dup2(inp[0], STDIN_FILENO);   close(inp[0]); }
        if (out_fd) { close(outp[0]); dup2(outp[1], STDOUT_FILENO); close(outp[1]); }
        execl("/bin/sh", "sh", "-c", command, (char *)NULL);
        {
            int e = errno;
            ssize_t ignored = write(errp[1], &e, sizeof e);
            (void)ignored;
        }
        _exit(127);
    }

    /* --- parent --- */
    close(errp[1]);
    if (in_fd)  close(inp[0]);
    if (out_fd) close(outp[1]);

    int child_err = 0;
    ssize_t got;
    do { got = read(errp[0], &child_err, sizeof child_err); }
    while (got < 0 && errno == EINTR);
    close(errp[0]);
    if (got == (ssize_t)sizeof child_err && child_err != 0) {
        /* It never ran.  Reap the shell corpse so it is not a zombie. */
        int st;
        while (waitpid(pid, &st, 0) < 0 && errno == EINTR) { }
        if (in_fd)  close(inp[1]);
        if (out_fd) close(outp[0]);
        *err_out = child_err;
        return -1;
    }

    if (in_fd)  *in_fd  = inp[1];
    if (out_fd) *out_fd = outp[0];
    return pid;
}

/* Wait for a child, restarting across signals. */
static int process_reap(pid_t pid, int *wstatus) {
    int r;
    do { r = (int)waitpid(pid, wstatus, 0); } while (r < 0 && errno == EINTR);
    return r;
}

/* The close function handed to oe_handle_new: it runs on process_close AND on
 * exit cleanup, which is what keeps a forgetful program from accumulating
 * zombies.  Closing the child's stdin first gives a well-behaved child its
 * cue to finish; only then do we insist. */
static void process_close_fn(void *payload) {
    Proc *p = (Proc *)payload;
    if (!p) return;

    if (p->in_fd >= 0) { close(p->in_fd); p->in_fd = -1; }
    if (p->out) { fclose(p->out); p->out = NULL; }

    if (!p->reaped) {
        int st;
        /* Give it a moment to notice the closed input, then escalate. */
        for (int i = 0; i < 100; i++) {           /* up to ~100ms */
            pid_t r = waitpid(p->pid, &st, WNOHANG);
            if (r == p->pid) { p->reaped = 1; p->status = process_status_of(st); break; }
            if (r < 0) { p->reaped = 1; break; }  /* already gone */
            struct timespec ts = { 0, 1000000 };  /* 1ms */
            nanosleep(&ts, NULL);
        }
        if (!p->reaped) {
            kill(p->pid, SIGTERM);
            for (int i = 0; i < 100; i++) {
                pid_t r = waitpid(p->pid, &st, WNOHANG);
                if (r == p->pid) { p->reaped = 1; p->status = process_status_of(st); break; }
                if (r < 0) { p->reaped = 1; break; }
                struct timespec ts = { 0, 1000000 };
                nanosleep(&ts, NULL);
            }
        }
        if (!p->reaped) {
            kill(p->pid, SIGKILL);
            if (process_reap(p->pid, &st) == p->pid) p->status = process_status_of(st);
            p->reaped = 1;
        }
    }
    free(p);
}

/* Resolve a handle; the handle table sets the error slot on every failure. */
static Proc *process_get(int32_t h) {
    return (Proc *)oe_handle_resolve(h, OE_HK_PROC);
}

/* --- run to completion ------------------------------------------------- */

/* process_run(text command) -> int
 * The command's exit status (128+signal if it was killed), or -1 if it could
 * not be started at all.  stdin/stdout are this program's own.
 *
 * Every command here runs under /bin/sh, so a pipeline, a redirection and a
 * loop all work.  That also fixes what "could not start" means: the shell is
 * the program being started, so a *missing* program inside the command is the
 * shell's honest exit 127 — the same answer a terminal gives — while -1 with
 * the error slot set means the spawn itself failed and nothing ran at all. */
void process_run(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv) {
    (void)argc;
    const char *cmd = process_nz(oe_arg_text(argv, 0));
    int err = 0;
    pid_t pid = process_spawn(cmd, NULL, NULL, &err);
    if (pid < 0) {
        oe_error_set_errno(err ? err : ENOENT, "run");
        oe_ret_int(ret, -1);
        return;
    }
    int st = 0;
    if (process_reap(pid, &st) != pid) {
        int e = errno;
        oe_error_set_errno(e, "wait");
        oe_ret_int(ret, -1);
        return;
    }
    oe_error_clear();
    oe_ret_int(ret, process_status_of(st));
}

/* process_run_capture(text command) -> text
 * Everything the command wrote to stdout, "" if it could not be started.  An
 * empty result with error code 0 means it ran and said nothing. */
void process_run_capture(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv) {
    (void)argc;
    const char *cmd = process_nz(oe_arg_text(argv, 0));
    int err = 0, out_fd = -1;
    pid_t pid = process_spawn(cmd, NULL, &out_fd, &err);
    if (pid < 0) {
        oe_error_set_errno(err ? err : ENOENT, "run");
        oe_ret_text(ret, process_empty_text());
        return;
    }

    char *buf = NULL;
    size_t len = 0, cap = 0;
    char chunk[4096];
    ssize_t n;
    int read_err = 0;
    while ((n = read(out_fd, chunk, sizeof chunk)) != 0) {
        if (n < 0) {
            if (errno == EINTR) continue;
            read_err = errno;
            break;
        }
        if (len + (size_t)n + 1 > cap) {
            size_t want = cap ? cap * 2 : 8192;
            while (want < len + (size_t)n + 1) want *= 2;
            char *nb = (char *)realloc(buf, want);
            if (!nb) { read_err = ENOMEM; break; }
            buf = nb; cap = want;
        }
        memcpy(buf + len, chunk, (size_t)n);
        len += (size_t)n;
    }
    close(out_fd);

    int st = 0;
    process_reap(pid, &st);              /* reap regardless: no zombies */

    if (read_err) {
        free(buf);
        oe_error_set_errno(read_err, "read");
        oe_ret_text(ret, process_empty_text());
        return;
    }
    char *out = process_dup_text(buf ? buf : "", (long)len);
    free(buf);
    oe_error_clear();                    /* a non-zero exit is not a failure */
    oe_ret_text(ret, out);
}

/* --- a child kept alive behind a handle -------------------------------- */

/* process_start(text command) -> int handle, 0 if it could not be started. */
void process_start(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv) {
    (void)argc;
    const char *cmd = process_nz(oe_arg_text(argv, 0));
    int err = 0, in_fd = -1, out_fd = -1;
    pid_t pid = process_spawn(cmd, &in_fd, &out_fd, &err);
    if (pid < 0) {
        oe_error_set_errno(err ? err : ENOENT, "start");
        oe_ret_int(ret, 0);
        return;
    }
    FILE *out = fdopen(out_fd, "r");
    if (!out) {
        int e = errno;
        close(in_fd); close(out_fd);
        kill(pid, SIGKILL);
        { int st; process_reap(pid, &st); }
        oe_error_set_errno(e, "start");
        oe_ret_int(ret, 0);
        return;
    }
    Proc *p = (Proc *)malloc(sizeof *p);
    if (!p) {
        fclose(out); close(in_fd);
        kill(pid, SIGKILL);
        { int st; process_reap(pid, &st); }
        oe_error_set(OE_ERR_TABLE_FULL, "out of memory");
        oe_ret_int(ret, 0);
        return;
    }
    p->pid = pid; p->in_fd = in_fd; p->out = out;
    p->reaped = 0; p->status = -1; p->at_end = 0;

    int32_t h = oe_handle_new(OE_HK_PROC, p, process_close_fn);
    if (h == 0) {                        /* the table set the slot itself */
        process_close_fn(p);
        oe_ret_int(ret, 0);
        return;
    }
    oe_error_clear();
    oe_ret_int(ret, h);
}

/* process_read_line(int h) -> text
 * One line of the child's output, without its newline.  "" at end of input —
 * which is why process_at_end sits beside it: a blank line the child printed
 * and no line at all are otherwise the same text. */
void process_read_line(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv) {
    (void)argc;
    Proc *p = process_get(oe_arg_int(argv, 0));
    if (!p) { oe_ret_text(ret, process_empty_text()); return; }
    if (!p->out) {
        oe_error_set(OE_ERR_INVALID_ARG, "output is closed");
        oe_ret_text(ret, process_empty_text());
        return;
    }

    char *line = NULL;
    size_t cap = 0;
    errno = 0;
    ssize_t n = getline(&line, &cap, p->out);
    int e = errno;
    if (n < 0) {
        free(line);
        if (ferror(p->out)) {
            oe_error_set_errno(e ? e : EIO, "read");
            oe_ret_text(ret, process_empty_text());
            return;
        }
        p->at_end = 1;
        oe_error_clear();                /* end of input is not a failure */
        oe_ret_text(ret, process_empty_text());
        return;
    }
    if (n > 0 && line[n - 1] == '\n') n--;
    if (n > 0 && line[n - 1] == '\r') n--;
    char *out = process_dup_text(line, (long)n);
    free(line);
    oe_error_clear();
    oe_ret_text(ret, out);
}

/* process_at_end(int h) -> bool : the child's output has ended. */
void process_at_end(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv) {
    (void)argc;
    Proc *p = process_get(oe_arg_int(argv, 0));
    if (!p) { oe_ret_bool(ret, 0); return; }
    oe_error_clear();
    oe_ret_bool(ret, p->at_end || !p->out);
}

/* process_write_line(int h, text line) -> bool
 * Sends the line and a newline to the child's input.  false with a non-zero
 * error code is a failure; the child having closed its input is EPIPE. */
void process_write_line(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv) {
    (void)argc;
    Proc *p = process_get(oe_arg_int(argv, 0));
    if (!p) { oe_ret_bool(ret, 0); return; }
    if (p->in_fd < 0) {
        oe_error_set(OE_ERR_INVALID_ARG, "input is closed");
        oe_ret_bool(ret, 0);
        return;
    }
    const char *s = process_nz(oe_arg_text(argv, 1));
    size_t len = strlen(s);

    /* One buffer so the line and its newline cannot be split by a short
     * write, which a line-oriented child would read as two lines. */
    char *buf = (char *)malloc(len + 1);
    if (!buf) { oe_error_set(OE_ERR_TABLE_FULL, "out of memory"); oe_ret_bool(ret, 0); return; }
    memcpy(buf, s, len);
    buf[len] = '\n';

    size_t off = 0;
    while (off < len + 1) {
        ssize_t w = write(p->in_fd, buf + off, len + 1 - off);
        int e = errno;
        if (w < 0) {
            if (e == EINTR) continue;
            free(buf);
            oe_error_set_errno(e, "write");
            oe_ret_bool(ret, 0);
            return;
        }
        off += (size_t)w;
    }
    free(buf);
    oe_error_clear();
    oe_ret_bool(ret, 1);
}

/* process_is_running(int h) -> bool
 * Checking also reaps a child that has finished, so polling this in a loop
 * never leaves a zombie behind. */
void process_is_running(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv) {
    (void)argc;
    Proc *p = process_get(oe_arg_int(argv, 0));
    if (!p) { oe_ret_bool(ret, 0); return; }
    if (p->reaped) { oe_error_clear(); oe_ret_bool(ret, 0); return; }

    int st = 0;
    pid_t r;
    do { r = waitpid(p->pid, &st, WNOHANG); } while (r < 0 && errno == EINTR);
    int e = errno;
    if (r == 0) { oe_error_clear(); oe_ret_bool(ret, 1); return; }
    if (r < 0) { oe_error_set_errno(e, "wait"); oe_ret_bool(ret, 0); return; }
    p->reaped = 1;
    p->status = process_status_of(st);
    oe_error_clear();
    oe_ret_bool(ret, 0);
}

/* process_wait(int h) -> int
 * The child's exit status, -1 if it could not be waited for.  The child's
 * input is closed first: a child still reading from us would otherwise never
 * finish, and the program would hang in a command that promises to return. */
void process_wait(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv) {
    (void)argc;
    Proc *p = process_get(oe_arg_int(argv, 0));
    if (!p) { oe_ret_int(ret, -1); return; }
    if (p->reaped) { oe_error_clear(); oe_ret_int(ret, p->status); return; }

    if (p->in_fd >= 0) { close(p->in_fd); p->in_fd = -1; }
    int st = 0;
    if (process_reap(p->pid, &st) != p->pid) {
        int e = errno;
        oe_error_set_errno(e, "wait");
        oe_ret_int(ret, -1);
        return;
    }
    p->reaped = 1;
    p->status = process_status_of(st);
    oe_error_clear();
    oe_ret_int(ret, p->status);
}

/* process_kill(int h) -> bool : stop the child now (SIGKILL) and reap it.
 * Killing an already-finished child is a success, not a failure — the caller
 * asked for it to be gone, and it is. */
void process_kill(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv) {
    (void)argc;
    Proc *p = process_get(oe_arg_int(argv, 0));
    if (!p) { oe_ret_bool(ret, 0); return; }
    if (p->reaped) { oe_error_clear(); oe_ret_bool(ret, 1); return; }

    if (kill(p->pid, SIGKILL) != 0) {
        int e = errno;
        if (e != ESRCH) { oe_error_set_errno(e, "kill"); oe_ret_bool(ret, 0); return; }
    }
    int st = 0;
    if (process_reap(p->pid, &st) == p->pid) p->status = process_status_of(st);
    p->reaped = 1;
    oe_error_clear();
    oe_ret_bool(ret, 1);
}

/* process_close(int h) -> bool : release the handle, ending the child if it
 * is still running. */
void process_close(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv) {
    (void)argc;
    int32_t h = oe_arg_int(argv, 0);
    if (oe_handle_close(h, OE_HK_PROC)) { oe_error_clear(); oe_ret_bool(ret, 1); return; }
    oe_ret_bool(ret, 0);                 /* the handle table set the slot */
}

/* process_close_all() -> int : how many children were closed. */
void process_close_all(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv) {
    (void)argc; (void)argv;
    int32_t n = oe_handle_close_kind(OE_HK_PROC);
    oe_error_clear();
    oe_ret_int(ret, n);
}
