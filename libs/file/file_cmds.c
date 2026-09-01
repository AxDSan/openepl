/* The "file" support library — files, directories and paths.
 *
 * Three surfaces, in order of how often they are the right one:
 *
 *   file_*  path-level, one shot.  Read a file, write a file, ask how big it
 *           is.  No resource is held, so nothing can be leaked or left open.
 *   file_open/...  a handle, for the file that does not fit in memory.  The
 *           program is handed a small positive int, never an address, and the
 *           handle table closes anything it still holds at exit.
 *   dir_*   directories.  A listing is a count plus an indexed accessor,
 *           because the language has no arrays.
 *   path_*  pure text.  These never touch the filesystem and cannot fail, so
 *           they never touch the error slot either.
 *
 * TEXT IS A C STRING.  file_read_text stops at an embedded NUL, so a file with
 * one in it reads short.  That is honest for text and wrong for binary; binary
 * I/O waits for a bytes type, which the ABI does not yet have.
 *
 * Every fallible command below takes exactly one of oe_error_clear() or
 * oe_error_set*() on every exit path, and copies errno to a local on the line
 * immediately after the call that failed — fclose() and free() clobber it.
 */
#include <dirent.h>
#include <errno.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <unistd.h>

#include "openepl_abi.h"

/* --- small helpers ---------------------------------------------------- */

static const char *file_nz(const char *s) { return s ? s : ""; }

/* A result string, runtime-owned like every other text result. */
static char *file_text_n(const char *s, size_t n) {
    char *o = (char *)oe_malloc((long)n + 1);
    if (!o) return NULL;
    if (n) memcpy(o, s, n);
    o[n] = '\0';
    return o;
}
static char *file_text(const char *s) { return file_text_n(s, strlen(file_nz(s))); }
/* The "" failure sentinel: a fresh empty string, so ownership is uniform. */
static char *file_empty(void) { return file_text_n("", 0); }

/* --- one-shot file commands ------------------------------------------- */

/* file_read_text(path) -> text : the whole file, "" on failure.  Read in
 * chunks rather than by seeking to the end, so a pipe or a /proc file — which
 * report no size — read correctly too. */
void file_read_text(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv) {
    (void)argc;
    const char *path = file_nz(oe_arg_text(argv, 0));
    FILE *f = fopen(path, "rb");
    int e = errno;
    if (!f) { oe_error_set_errno(e, "open"); oe_ret_text(ret, file_empty()); return; }

    long cap = 4096, len = 0;
    char *buf = (char *)oe_malloc(cap);
    if (!buf) { fclose(f); oe_error_set(OE_ERR_UNSUPPORTED, "out of memory"); oe_ret_text(ret, NULL); return; }
    for (;;) {
        size_t got = fread(buf + len, 1, (size_t)(cap - len - 1), f);
        len += (long)got;
        if (got == 0) break;
        if (len + 1 >= cap) {
            char *nb = (char *)oe_mrealloc(buf, cap * 2);
            if (!nb) break;
            buf = nb;
            cap *= 2;
        }
    }
    int bad = ferror(f);
    e = errno;
    fclose(f);
    if (bad) { oe_error_set_errno(e, "read"); oe_ret_text(ret, file_empty()); return; }
    buf[len] = '\0';
    oe_error_clear();
    oe_ret_text(ret, buf);
}

/* Shared by file_write_text and file_append_text. */
static void file_put(OpenEPL_Slot *ret, OpenEPL_Slot *argv, const char *mode, const char *what) {
    const char *path = file_nz(oe_arg_text(argv, 0));
    const char *body = file_nz(oe_arg_text(argv, 1));
    FILE *f = fopen(path, mode);
    int e = errno;
    if (!f) { oe_error_set_errno(e, what); oe_ret_bool(ret, 0); return; }
    size_t n = strlen(body);
    int err = 0;
    if (n && fwrite(body, 1, n, f) != n) { err = errno ? errno : EIO; }
    /* fclose flushes, so a short write can surface here and nowhere earlier. */
    if (fclose(f) != 0 && !err) { err = errno ? errno : EIO; }
    if (err) { oe_error_set_errno(err, "write"); oe_ret_bool(ret, 0); return; }
    oe_error_clear();
    oe_ret_bool(ret, 1);
}

/* file_write_text(path, content) -> bool : replaces the file. */
void file_write_text(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv) {
    (void)argc; file_put(ret, argv, "wb", "create");
}
/* file_append_text(path, content) -> bool : adds to the end, creating it. */
void file_append_text(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv) {
    (void)argc; file_put(ret, argv, "ab", "open for append");
}

/* "the path is not there" is an answer, not a failure: those errno values
 * clear the slot and return false, so `false, code 0` means a genuine no. */
static int file_absent(int e) { return e == ENOENT || e == ENOTDIR; }

static void file_stat_is(OpenEPL_Slot *ret, OpenEPL_Slot *argv, int want_dir) {
    const char *path = file_nz(oe_arg_text(argv, 0));
    struct stat st;
    int rc = stat(path, &st);
    int e = errno;
    if (rc != 0) {
        if (file_absent(e)) { oe_error_clear(); oe_ret_bool(ret, 0); return; }
        oe_error_set_errno(e, "stat");
        oe_ret_bool(ret, 0);
        return;
    }
    oe_error_clear();
    oe_ret_bool(ret, want_dir ? S_ISDIR(st.st_mode) != 0 : S_ISREG(st.st_mode) != 0);
}

/* file_exists(path) -> bool : true for a regular file.  A directory is not a
 * file, so this is false for one — ask dir_exists instead. */
void file_exists(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv) {
    (void)argc; file_stat_is(ret, argv, 0);
}

/* file_size(path) -> int64 : bytes, -1 on failure. */
void file_size(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv) {
    (void)argc;
    const char *path = file_nz(oe_arg_text(argv, 0));
    struct stat st;
    int rc = stat(path, &st);
    int e = errno;
    if (rc != 0) { oe_error_set_errno(e, "stat"); oe_ret_int64(ret, -1); return; }
    oe_error_clear();
    oe_ret_int64(ret, (int64_t)st.st_size);
}

/* file_modified(path) -> int64 : last-modified time in seconds since the
 * epoch, the same scale core's now() and format_time() use.  -1 on failure. */
void file_modified(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv) {
    (void)argc;
    const char *path = file_nz(oe_arg_text(argv, 0));
    struct stat st;
    int rc = stat(path, &st);
    int e = errno;
    if (rc != 0) { oe_error_set_errno(e, "stat"); oe_ret_int64(ret, -1); return; }
    oe_error_clear();
    oe_ret_int64(ret, (int64_t)st.st_mtime);
}

/* file_delete(path) -> bool.  unlink, not remove: a directory must be refused
 * rather than quietly removed by a command whose name says "file". */
void file_delete(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv) {
    (void)argc;
    const char *path = file_nz(oe_arg_text(argv, 0));
    int rc = unlink(path);
    int e = errno;
    if (rc != 0) { oe_error_set_errno(e, "delete"); oe_ret_bool(ret, 0); return; }
    oe_error_clear();
    oe_ret_bool(ret, 1);
}

/* Byte-for-byte copy.  Returns 0 on success, otherwise an errno value, and
 * writes the stage that failed to *what. */
static int file_copy_bytes(const char *from, const char *to, const char **what) {
    FILE *in = fopen(from, "rb");
    int e = errno;
    if (!in) { *what = "open source"; return e ? e : EIO; }
    FILE *out = fopen(to, "wb");
    e = errno;
    if (!out) { fclose(in); *what = "create destination"; return e ? e : EIO; }

    char buf[8192];
    int err = 0;
    for (;;) {
        size_t got = fread(buf, 1, sizeof buf, in);
        if (got == 0) break;
        size_t put = fwrite(buf, 1, got, out);
        e = errno;
        if (put != got) { err = e ? e : EIO; *what = "write"; break; }
    }
    if (!err && ferror(in)) { err = errno ? errno : EIO; *what = "read"; }
    fclose(in);
    if (fclose(out) != 0 && !err) { err = errno ? errno : EIO; *what = "write"; }
    return err;
}

/* file_copy(from, to) -> bool : contents only; permissions and timestamps are
 * whatever a newly created file gets. */
void file_copy(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv) {
    (void)argc;
    const char *from = file_nz(oe_arg_text(argv, 0));
    const char *to   = file_nz(oe_arg_text(argv, 1));
    const char *what = "copy";
    int err = file_copy_bytes(from, to, &what);
    if (err) { oe_error_set_errno(err, what); oe_ret_bool(ret, 0); return; }
    oe_error_clear();
    oe_ret_bool(ret, 1);
}

/* file_move(from, to) -> bool.  rename() first — it is atomic and cheap — and
 * fall back to copy-then-delete only for the one case it cannot do, a move
 * across filesystems. */
void file_move(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv) {
    (void)argc;
    const char *from = file_nz(oe_arg_text(argv, 0));
    const char *to   = file_nz(oe_arg_text(argv, 1));
    int rc = rename(from, to);
    int e = errno;
    if (rc == 0) { oe_error_clear(); oe_ret_bool(ret, 1); return; }
    if (e != EXDEV) { oe_error_set_errno(e, "move"); oe_ret_bool(ret, 0); return; }

    const char *what = "move";
    int err = file_copy_bytes(from, to, &what);
    if (err) { oe_error_set_errno(err, what); oe_ret_bool(ret, 0); return; }
    rc = unlink(from);
    e = errno;
    if (rc != 0) { oe_error_set_errno(e, "delete source"); oe_ret_bool(ret, 0); return; }
    oe_error_clear();
    oe_ret_bool(ret, 1);
}

/* file_line_count(path) -> int : -1 on failure.  A last line without a
 * trailing newline still counts, so the number matches what an editor shows. */
void file_line_count(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv) {
    (void)argc;
    const char *path = file_nz(oe_arg_text(argv, 0));
    FILE *f = fopen(path, "rb");
    int e = errno;
    if (!f) { oe_error_set_errno(e, "open"); oe_ret_int(ret, -1); return; }

    char buf[8192];
    long lines = 0;
    int last = '\n';                 /* an empty file has no lines at all */
    for (;;) {
        size_t got = fread(buf, 1, sizeof buf, f);
        if (got == 0) break;
        for (size_t i = 0; i < got; i++) if (buf[i] == '\n') lines++;
        last = (unsigned char)buf[got - 1];
    }
    int bad = ferror(f);
    e = errno;
    fclose(f);
    if (bad) { oe_error_set_errno(e, "read"); oe_ret_int(ret, -1); return; }
    if (last != '\n') lines++;
    oe_error_clear();
    oe_ret_int(ret, (int32_t)lines);
}

/* --- handles ----------------------------------------------------------- */

static void file_close_fn(void *payload) {
    if (payload) fclose((FILE *)payload);
}

/* file_open(path, mode) -> int : a handle, 0 on failure.  The mode is a word,
 * not a punctuation soup: "read", "write" or "append". */
void file_open(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv) {
    (void)argc;
    const char *path = file_nz(oe_arg_text(argv, 0));
    const char *mode = file_nz(oe_arg_text(argv, 1));
    const char *cmode = NULL;
    if      (strcmp(mode, "read")   == 0) cmode = "rb";
    else if (strcmp(mode, "write")  == 0) cmode = "wb";
    else if (strcmp(mode, "append") == 0) cmode = "ab";
    if (!cmode) {
        oe_error_set(OE_ERR_INVALID_ARG, "mode must be \"read\", \"write\" or \"append\"");
        oe_ret_int(ret, 0);
        return;
    }
    FILE *f = fopen(path, cmode);
    int e = errno;
    if (!f) { oe_error_set_errno(e, "open"); oe_ret_int(ret, 0); return; }

    int32_t h = oe_handle_new(OE_HK_FILE, f, file_close_fn);
    if (h == 0) { fclose(f); oe_ret_int(ret, 0); return; }  /* slot set by the table */
    oe_error_clear();
    oe_ret_int(ret, h);
}

/* file_read_line(handle) -> text : the next line without its newline, "" at
 * the end of the file.  A blank line and the end of the file both read "",
 * which is why file_at_end ships next to this one. */
void file_read_line(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv) {
    (void)argc;
    FILE *f = (FILE *)oe_handle_resolve(oe_arg_int(argv, 0), OE_HK_FILE);
    if (!f) { oe_ret_text(ret, file_empty()); return; }   /* slot set by the table */

    long cap = 128, len = 0;
    char *buf = (char *)oe_malloc(cap);
    if (!buf) { oe_error_set(OE_ERR_UNSUPPORTED, "out of memory"); oe_ret_text(ret, NULL); return; }
    for (;;) {
        int c = getc(f);
        if (c == EOF || c == '\n') break;
        if (len + 1 >= cap) {
            char *nb = (char *)oe_mrealloc(buf, cap * 2);
            if (!nb) break;
            buf = nb;
            cap *= 2;
        }
        buf[len++] = (char)c;
    }
    int bad = ferror(f);
    int e = errno;
    if (bad) { oe_error_set_errno(e, "read"); oe_ret_text(ret, file_empty()); return; }
    /* A trailing CR is stripped so a file written on Windows reads the same
     * here as it does there. */
    if (len > 0 && buf[len - 1] == '\r') len--;
    buf[len] = '\0';
    oe_error_clear();
    oe_ret_text(ret, buf);
}

/* file_at_end(handle) -> bool : true when the next read would find nothing.
 * Peeks one character and puts it back, so it can be asked before a read
 * without consuming anything — feof() alone only turns true after a read has
 * already run off the end. */
void file_at_end(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv) {
    (void)argc;
    FILE *f = (FILE *)oe_handle_resolve(oe_arg_int(argv, 0), OE_HK_FILE);
    if (!f) { oe_ret_bool(ret, 0); return; }              /* slot set by the table */
    int c = getc(f);
    int e = errno;
    if (c == EOF) {
        if (ferror(f)) { oe_error_set_errno(e, "read"); oe_ret_bool(ret, 0); return; }
        oe_error_clear();
        oe_ret_bool(ret, 1);
        return;
    }
    ungetc(c, f);
    oe_error_clear();
    oe_ret_bool(ret, 0);
}

/* file_write_line(handle, line) -> bool : the line plus a newline. */
void file_write_line(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv) {
    (void)argc;
    FILE *f = (FILE *)oe_handle_resolve(oe_arg_int(argv, 0), OE_HK_FILE);
    if (!f) { oe_ret_bool(ret, 0); return; }              /* slot set by the table */
    const char *line = file_nz(oe_arg_text(argv, 1));
    int rc = fputs(line, f);
    int e = errno;
    if (rc != EOF) { rc = fputc('\n', f); e = errno; }
    if (rc == EOF) { oe_error_set_errno(e ? e : EIO, "write"); oe_ret_bool(ret, 0); return; }
    oe_error_clear();
    oe_ret_bool(ret, 1);
}

/* file_close(handle) -> bool.  Closing twice is a failure, not a crash: the
 * handle table bumps a generation and reports a stale handle. */
void file_close(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv) {
    (void)argc;
    /* The table clears or sets the slot itself, so every family reports a bad
     * handle in the same words. */
    oe_ret_bool(ret, oe_handle_close(oe_arg_int(argv, 0), OE_HK_FILE));
}

/* file_close_all() -> int : how many were still open.  The safety net for a
 * program that lost track; exit closes them anyway. */
void file_close_all(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv) {
    (void)argc; (void)argv;
    oe_ret_int(ret, oe_handle_close_kind(OE_HK_FILE));
}

/* --- directories ------------------------------------------------------- */

/* dir_exists(path) -> bool. */
void dir_exists(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv) {
    (void)argc; file_stat_is(ret, argv, 1);
}

/* dir_create(path) -> bool : creates missing parents too, and succeeds when
 * the directory is already there — asking for a directory to exist should not
 * fail because it does. */
void dir_create(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv) {
    (void)argc;
    const char *path = file_nz(oe_arg_text(argv, 0));
    size_t n = strlen(path);
    if (n == 0) { oe_error_set(OE_ERR_INVALID_ARG, "empty path"); oe_ret_bool(ret, 0); return; }

    char *work = (char *)malloc(n + 1);      /* bookkeeping, not program data */
    if (!work) { oe_error_set(OE_ERR_UNSUPPORTED, "out of memory"); oe_ret_bool(ret, 0); return; }
    memcpy(work, path, n + 1);

    for (size_t i = 1; i <= n; i++) {
        if (work[i] != '/' && work[i] != '\0') continue;
        char saved = work[i];
        work[i] = '\0';
        int rc = mkdir(work, 0777);
        int e = errno;
        if (rc != 0 && e != EEXIST) {
            free(work);
            oe_error_set_errno(e, "create directory");
            oe_ret_bool(ret, 0);
            return;
        }
        work[i] = saved;
    }
    free(work);
    /* EEXIST above may have been a *file* in the way; confirm what is there. */
    struct stat st;
    int rc = stat(path, &st);
    int e = errno;
    if (rc != 0) { oe_error_set_errno(e, "create directory"); oe_ret_bool(ret, 0); return; }
    if (!S_ISDIR(st.st_mode)) {
        oe_error_set_errno(ENOTDIR, "create directory");
        oe_ret_bool(ret, 0);
        return;
    }
    oe_error_clear();
    oe_ret_bool(ret, 1);
}

/* dir_delete(path) -> bool : the directory must be empty.  There is no
 * recursive delete here on purpose — one mistyped path should not be able to
 * erase a tree. */
void dir_delete(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv) {
    (void)argc;
    const char *path = file_nz(oe_arg_text(argv, 0));
    int rc = rmdir(path);
    int e = errno;
    if (rc != 0) { oe_error_set_errno(e, "delete directory"); oe_ret_bool(ret, 0); return; }
    oe_error_clear();
    oe_ret_bool(ret, 1);
}

/* dir_current() -> text : the working directory, "" on failure. */
void dir_current(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv) {
    (void)argc; (void)argv;
    char buf[4096];
    char *p = getcwd(buf, sizeof buf);
    int e = errno;
    if (!p) { oe_error_set_errno(e, "current directory"); oe_ret_text(ret, file_empty()); return; }
    oe_error_clear();
    oe_ret_text(ret, file_text(buf));
}

/* dir_set_current(path) -> bool. */
void dir_set_current(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv) {
    (void)argc;
    const char *path = file_nz(oe_arg_text(argv, 0));
    int rc = chdir(path);
    int e = errno;
    if (rc != 0) { oe_error_set_errno(e, "change directory"); oe_ret_bool(ret, 0); return; }
    oe_error_clear();
    oe_ret_bool(ret, 1);
}

/* The listing snapshot.  A directory changes under a loop that reads it, so
 * dir_entry_count takes a sorted snapshot and dir_entry only reads it: a loop
 * sees one stable view, and refreshing is an explicit act — calling the count
 * again.  Plain malloc, like the handle table: this is bookkeeping, not
 * program data, and must not be freed by oe_free_all() at exit. */
static char  *g_snap_path = NULL;
static char **g_snap = NULL;
static long   g_snap_n = 0;

static void file_snap_free(void) {
    for (long i = 0; i < g_snap_n; i++) free(g_snap[i]);
    free(g_snap);
    free(g_snap_path);
    g_snap = NULL;
    g_snap_path = NULL;
    g_snap_n = 0;
}

static int file_name_cmp(const void *a, const void *b) {
    return strcmp(*(const char *const *)a, *(const char *const *)b);
}

/* dir_entry_count(path) -> int : -1 on failure.  Re-reads the directory and
 * re-takes the snapshot that dir_entry reads. */
void dir_entry_count(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv) {
    (void)argc;
    const char *path = file_nz(oe_arg_text(argv, 0));
    DIR *d = opendir(path);
    int e = errno;
    if (!d) {
        file_snap_free();                    /* a failed listing leaves none */
        oe_error_set_errno(e, "open directory");
        oe_ret_int(ret, -1);
        return;
    }
    file_snap_free();

    long cap = 0;
    struct dirent *de;
    errno = 0;
    while ((de = readdir(d)) != NULL) {
        if (strcmp(de->d_name, ".") == 0 || strcmp(de->d_name, "..") == 0) continue;
        if (g_snap_n == cap) {
            long want = cap ? cap * 2 : 32;
            char **nb = (char **)realloc(g_snap, (size_t)want * sizeof(char *));
            if (!nb) { closedir(d); file_snap_free(); oe_error_set(OE_ERR_UNSUPPORTED, "out of memory"); oe_ret_int(ret, -1); return; }
            g_snap = nb;
            cap = want;
        }
        size_t n = strlen(de->d_name);
        char *copy = (char *)malloc(n + 1);
        if (!copy) { closedir(d); file_snap_free(); oe_error_set(OE_ERR_UNSUPPORTED, "out of memory"); oe_ret_int(ret, -1); return; }
        memcpy(copy, de->d_name, n + 1);
        g_snap[g_snap_n++] = copy;
        errno = 0;
    }
    int read_err = errno;
    closedir(d);
    if (read_err != 0) {
        file_snap_free();
        oe_error_set_errno(read_err, "read directory");
        oe_ret_int(ret, -1);
        return;
    }
    /* Sorted, so two runs of the same program list the same order — readdir's
     * own order is whatever the filesystem happens to hand back. */
    if (g_snap_n > 1) qsort(g_snap, (size_t)g_snap_n, sizeof(char *), file_name_cmp);

    size_t pn = strlen(path);
    g_snap_path = (char *)malloc(pn + 1);
    if (!g_snap_path) { file_snap_free(); oe_error_set(OE_ERR_UNSUPPORTED, "out of memory"); oe_ret_int(ret, -1); return; }
    memcpy(g_snap_path, path, pn + 1);

    oe_error_clear();
    oe_ret_int(ret, (int32_t)g_snap_n);
}

/* dir_entry(path, index) -> text : one name from the snapshot, without its
 * directory.  Out of range is "" with no error — the count is the authority,
 * and no real entry is ever named "".  Asking for a path that was not the last
 * one counted IS an error, because the answer would otherwise be silently
 * about a different directory. */
void dir_entry(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv) {
    (void)argc;
    const char *path = file_nz(oe_arg_text(argv, 0));
    int32_t i = oe_arg_int(argv, 1);
    if (!g_snap_path || strcmp(g_snap_path, path) != 0) {
        oe_error_set(OE_ERR_INVALID_ARG, "no listing for this path; call dir_entry_count first");
        oe_ret_text(ret, file_empty());
        return;
    }
    if (i < 0 || (long)i >= g_snap_n) { oe_error_clear(); oe_ret_text(ret, file_empty()); return; }
    oe_error_clear();
    oe_ret_text(ret, file_text(g_snap[i]));
}

/* --- paths ------------------------------------------------------------
 * Pure text.  None of these look at the filesystem, so none of them can fail,
 * so none of them touches the error slot — an error set by a file command
 * survives the path arithmetic done while reporting it. */

/* path_join(a, b) -> text : one separator, never two.  An absolute b wins,
 * which is what every other path_join in the world does. */
void path_join(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv) {
    (void)argc;
    const char *a = file_nz(oe_arg_text(argv, 0));
    const char *b = file_nz(oe_arg_text(argv, 1));
    if (*b == '/' || *a == '\0') { oe_ret_text(ret, file_text(b)); return; }
    if (*b == '\0') { oe_ret_text(ret, file_text(a)); return; }
    size_t la = strlen(a), lb = strlen(b);
    int sep = a[la - 1] != '/';
    char *o = (char *)oe_malloc((long)(la + (size_t)sep + lb + 1));
    if (!o) { oe_ret_text(ret, NULL); return; }
    memcpy(o, a, la);
    if (sep) o[la] = '/';
    memcpy(o + la + sep, b, lb + 1);
    oe_ret_text(ret, o);
}

/* Where the last component starts, ignoring trailing slashes. */
static void file_split(const char *p, size_t *base, size_t *end) {
    size_t n = strlen(p);
    while (n > 1 && p[n - 1] == '/') n--;
    size_t b = n;
    while (b > 0 && p[b - 1] != '/') b--;
    *base = b;
    *end = n;
}

/* path_name(path) -> text : the last component. */
void path_name(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv) {
    (void)argc;
    const char *p = file_nz(oe_arg_text(argv, 0));
    size_t b, e;
    file_split(p, &b, &e);
    oe_ret_text(ret, file_text_n(p + b, e - b));
}

/* path_parent(path) -> text : everything before the last component, "" when
 * there is none.  "" joins as nothing, so path_join(path_parent(x), y) does
 * the right thing for a bare filename. */
void path_parent(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv) {
    (void)argc;
    const char *p = file_nz(oe_arg_text(argv, 0));
    size_t b, e;
    file_split(p, &b, &e);
    (void)e;
    if (b == 0) { oe_ret_text(ret, file_text_n("", 0)); return; }
    if (b == 1) { oe_ret_text(ret, file_text_n("/", 1)); return; }
    oe_ret_text(ret, file_text_n(p, b - 1));            /* drop the separator */
}

/* path_extension(path) -> text : after the last dot of the last component,
 * without the dot.  A name that only begins with a dot has no extension —
 * ".bashrc" is a hidden file, not a "bashrc" file. */
void path_extension(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv) {
    (void)argc;
    const char *p = file_nz(oe_arg_text(argv, 0));
    size_t b, e;
    file_split(p, &b, &e);
    size_t dot = e;
    for (size_t i = e; i > b; i--) if (p[i - 1] == '.') { dot = i - 1; break; }
    if (dot == e || dot == b) { oe_ret_text(ret, file_text_n("", 0)); return; }
    oe_ret_text(ret, file_text_n(p + dot + 1, e - dot - 1));
}

/* path_absolute(path) -> text : the path rooted at the working directory, with
 * "." and ".." resolved textually.  Deliberately lexical: realpath() fails for
 * a path that does not exist yet, and "where would I write this?" is a
 * question about text, not about what is on disk.  It follows that a symbolic
 * link is not resolved. */
void path_absolute(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv) {
    (void)argc;
    const char *p = file_nz(oe_arg_text(argv, 0));
    char cwd[4096];
    size_t need = strlen(p) + 2;
    if (*p != '/') {
        if (!getcwd(cwd, sizeof cwd)) {
            /* Infallible by contract: hand back what we were given rather than
             * set an error slot a path command is not allowed to touch. */
            oe_ret_text(ret, file_text(p));
            return;
        }
        need += strlen(cwd) + 1;
    }
    char *o = (char *)oe_malloc((long)need);
    if (!o) { oe_ret_text(ret, NULL); return; }

    /* Build, then walk component by component, writing each one after the last
     * '/' already emitted.  ".." rewinds that write position. */
    size_t w = 0;
    const char *parts[2];
    int np = 0;
    if (*p != '/') parts[np++] = cwd;
    parts[np++] = p;
    o[w++] = '/';
    for (int k = 0; k < np; k++) {
        const char *s = parts[k];
        size_t i = 0, n = strlen(s);
        while (i < n) {
            while (i < n && s[i] == '/') i++;
            size_t start = i;
            while (i < n && s[i] != '/') i++;
            size_t len = i - start;
            if (len == 0) continue;
            if (len == 1 && s[start] == '.') continue;
            if (len == 2 && s[start] == '.' && s[start + 1] == '.') {
                while (w > 1 && o[w - 1] != '/') w--;
                if (w > 1) w--;                     /* drop the separator too */
                continue;
            }
            if (w > 1) o[w++] = '/';
            memcpy(o + w, s + start, len);
            w += len;
        }
    }
    o[w] = '\0';                    /* w >= 1: the leading '/' is always there */
    oe_ret_text(ret, o);
}
