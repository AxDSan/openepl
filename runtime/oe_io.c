/* Core commands: console output (slot ABI). */
#include <stdio.h>
#include "openepl_core.h"

void oe_print_int(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv) {
    (void)argc; (void)ret;
    printf("%d\n", oe_arg_int(argv, 0));
}
void oe_print_int64(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv) {
    (void)argc; (void)ret;
    printf("%lld\n", (long long)oe_arg_int64(argv, 0));
}
void oe_print_text(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv) {
    (void)argc; (void)ret;
    const char *t = oe_arg_text(argv, 0);
    printf("%s\n", t ? t : "");
}
void oe_print_double(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv) {
    (void)argc; (void)ret;
    printf("%g\n", oe_arg_double(argv, 0));
}

/* --- Input ------------------------------------------------------------
 * The core could print four ways and read none, which made the first program
 * anyone writes — ask a question, use the answer — impossible without a `use`
 * declaration.  These are the other half of the print_* pair, so they belong
 * beside them rather than in a support library. */

/* Read one line from stdin, without its newline.  Grows as it goes, so a long
 * line is not silently truncated the way a fixed buffer would. */
static char *read_line_into_text(void) {
    long cap = 128, len = 0;
    char *buf = (char *)oe_malloc(cap);
    if (!buf) return NULL;
    for (;;) {
        int c = getchar();
        if (c == EOF || c == '\n') break;
        if (len + 1 >= cap) {
            char *nb = (char *)oe_mrealloc(buf, cap * 2);
            if (!nb) break;
            buf = nb;
            cap *= 2;
        }
        buf[len++] = (char)c;
    }
    /* A trailing CR is stripped so a file written on Windows reads the same
     * here as it does there — otherwise every comparison against it fails for
     * a reason nothing on screen can show. */
    if (len > 0 && buf[len - 1] == '\r') len--;
    buf[len] = '\0';
    return buf;
}

/* read_line() -> text : the next line, or "" at end of input. */
void oe_read_line(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv) {
    (void)argc; (void)argv;
    oe_ret_text(ret, read_line_into_text());
}

/* input_ended() -> bool : the predicate that tells an empty line apart from no
 * line at all.  Peeks one character and puts it back, so it can be called
 * before a read without consuming anything. */
void oe_input_ended(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv) {
    (void)argc; (void)argv;
    int c = getchar();
    if (c == EOF) { oe_ret_bool(ret, 1); return; }
    ungetc(c, stdin);
    oe_ret_bool(ret, 0);
}

/* ask(text prompt) -> text : print the prompt, then read a line. */
void oe_ask(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv) {
    (void)argc;
    const char *p = oe_arg_text(argv, 0);
    fputs(p ? p : "", stdout);
    /* stdout is line-buffered, and a prompt has no newline — without this the
     * question appears after the answer is typed. */
    fflush(stdout);
    oe_ret_text(ret, read_line_into_text());
}
