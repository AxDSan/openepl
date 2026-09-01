/* Arrays and byte-sets — the types that hold more than one value.
 *
 * Both are one runtime-owned allocation whose layout is stated in
 * abi/openepl_abi.h, and both live in the slot as a pointer, exactly the way
 * text does.  That is the whole trick: the slot's value field is eight bytes
 * and a pointer is eight bytes, so nothing in the marshaling path had to widen
 * to make aggregates work.
 *
 * NOTHING HERE EVER MOVES AN ARRAY.  `append` allocates a new one and copies,
 * rather than growing the old one through E_MRealloc, because a program may
 * hold the same array under two names — and reallocating would leave the other
 * name addressing freed memory.  The cost is a copy per append; the alternative
 * is the class of bug the handle table exists to make impossible.  `cap` is
 * still recorded truthfully, since the header states it and a wrong number in a
 * header is worse than a redundant one.
 *
 * Indexing is reached through the plain helpers at the top rather than through
 * the slot ABI: it is syntax, and building an argv array to read one element
 * would cost more code than the read.  They move raw 64-bit values, which is
 * what a slot's value field already holds.
 */
#include <stdlib.h>
#include <string.h>
#include <stdio.h>
#include "openepl_core.h"

/* Elements are one slot-width each, so an array of text holds pointers the
 * same way an array of int holds ints. */
static int64_t *elems(OpenEPL_Array *a) { return (int64_t *)(a + 1); }
static unsigned char *bin_bytes(OpenEPL_Bin *b) { return (unsigned char *)(b + 1); }

/* An array that was never created reads as empty rather than as an error: a
 * module-level `var xs: int[]` is zero until its initializer runs, and a
 * program asking how long nothing is deserves 0, not a diagnostic. */
static int32_t ary_len(const OpenEPL_Array *a) { return a ? a->len : 0; }

void *oe_ary_new(int32_t tag, int32_t len) {
    if (len < 0) len = 0;
    OpenEPL_Array *a = (OpenEPL_Array *)oe_malloc(
        (long)sizeof(OpenEPL_Array) + (long)len * 8);
    if (!a) return NULL;
    a->elem_tag = tag;
    a->len = len;
    a->cap = len;
    a->_pad = 0;
    memset(elems(a), 0, (size_t)len * 8);
    return a;
}

/* Out of range fails LOUDLY and returns a sentinel: reading whatever follows
 * the array is the one outcome an index must never have.  Text gets "" rather
 * than a null pointer, so a failed read stays printable. */
int64_t oe_ary_get(void *p, int32_t i) {
    OpenEPL_Array *a = (OpenEPL_Array *)p;
    if (!a || i < 0 || i >= a->len) {
        char msg[96];
        snprintf(msg, sizeof msg, "index %d is outside a list of %d element(s)",
                 (int)i, (int)ary_len(a));
        oe_error_set(OE_ERR_OUT_OF_RANGE, msg);
        if (a && a->elem_tag == OE_SDT_TEXT) return (int64_t)(intptr_t)oe_empty_text();
        return 0;
    }
    oe_error_clear();
    return elems(a)[i];
}

void oe_ary_set(void *p, int32_t i, int64_t v) {
    OpenEPL_Array *a = (OpenEPL_Array *)p;
    if (!a || i < 0 || i >= a->len) {
        char msg[96];
        snprintf(msg, sizeof msg, "index %d is outside a list of %d element(s)",
                 (int)i, (int)ary_len(a));
        oe_error_set(OE_ERR_OUT_OF_RANGE, msg);
        return;
    }
    elems(a)[i] = v;
    oe_error_clear();
}

void *oe_bin_new(int32_t len) {
    if (len < 0) len = 0;
    OpenEPL_Bin *b = (OpenEPL_Bin *)oe_malloc((long)sizeof(OpenEPL_Bin) + len);
    if (!b) return NULL;
    b->dims = 1;
    b->len = len;
    memset(bin_bytes(b), 0, (size_t)len);
    return b;
}

int32_t oe_bin_at(void *p, int32_t i) {
    OpenEPL_Bin *b = (OpenEPL_Bin *)p;
    if (!b || i < 0 || i >= b->len) {
        char msg[96];
        snprintf(msg, sizeof msg, "index %d is outside %d byte(s)",
                 (int)i, b ? (int)b->len : 0);
        oe_error_set(OE_ERR_OUT_OF_RANGE, msg);
        return -1;
    }
    oe_error_clear();
    return bin_bytes(b)[i];
}

void oe_bin_set(void *p, int32_t i, int32_t v) {
    OpenEPL_Bin *b = (OpenEPL_Bin *)p;
    if (!b || i < 0 || i >= b->len) {
        char msg[96];
        snprintf(msg, sizeof msg, "index %d is outside %d byte(s)",
                 (int)i, b ? (int)b->len : 0);
        oe_error_set(OE_ERR_OUT_OF_RANGE, msg);
        return;
    }
    bin_bytes(b)[i] = (unsigned char)(v & 0xFF);
    oe_error_clear();
}

/* --- comparing and formatting one element ------------------------------
 * Every element is 64 raw bits; only the array's tag says what they mean, so
 * these two functions are the single place that knows. */
static const char *as_text(int64_t v) {
    const char *s = (const char *)(intptr_t)v;
    return s ? s : "";
}

static int elem_cmp(int32_t tag, int64_t x, int64_t y) {
    if (tag == OE_SDT_TEXT) return strcmp(as_text(x), as_text(y));
    if (tag == OE_SDT_DOUBLE) {
        double a, b;
        memcpy(&a, &x, 8); memcpy(&b, &y, 8);
        return (a < b) ? -1 : (a > b) ? 1 : 0;
    }
    return (x < y) ? -1 : (x > y) ? 1 : 0;
}

/* Formatting goes through the same spellings as int_to_text/double_to_text, so
 * a joined list and a printed element never disagree about what a number
 * looks like. */
static void elem_text(int32_t tag, int64_t v, char *out, size_t n) {
    switch (tag) {
        case OE_SDT_INT:    snprintf(out, n, "%d", (int)(int32_t)v); break;
        case OE_SDT_INT64:  snprintf(out, n, "%lld", (long long)v); break;
        case OE_SDT_DOUBLE: { double d; memcpy(&d, &v, 8); snprintf(out, n, "%g", d); break; }
        case OE_SDT_BOOL:   snprintf(out, n, "%s", v ? "true" : "false"); break;
        default:            snprintf(out, n, "%s", as_text(v)); break;
    }
}

/* qsort's comparator carries no context, and the runtime is single-threaded,
 * so the tag of the array being sorted is parked here for the duration. */
static int32_t g_sort_tag = OE_SDT_INT;
static int sort_cmp(const void *x, const void *y) {
    return elem_cmp(g_sort_tag, *(const int64_t *)x, *(const int64_t *)y);
}

/* --- commands ---------------------------------------------------------- */

static OpenEPL_Array *arg_ary(OpenEPL_Slot *argv, int i) {
    return (OpenEPL_Array *)argv[i].v.ptr;
}
static OpenEPL_Bin *arg_bin(OpenEPL_Slot *argv, int i) {
    return (OpenEPL_Bin *)argv[i].v.ptr;
}

void oe_ary_count(OpenEPL_Slot *r, int32_t c, OpenEPL_Slot *argv) {
    (void)c;
    oe_ret_int(r, ary_len(arg_ary(argv, 0)));
}

/* A NEW array, longer by one. See the file header for why this copies. */
void oe_ary_append(OpenEPL_Slot *r, int32_t c, OpenEPL_Slot *argv) {
    (void)c;
    OpenEPL_Array *a = arg_ary(argv, 0);
    int32_t n = ary_len(a);
    int32_t tag = a ? a->elem_tag : argv[1].tag;
    OpenEPL_Array *out = (OpenEPL_Array *)oe_ary_new(tag, n + 1);
    if (!out) { r->tag = OE_SDT_BIN; r->v.ptr = NULL; return; }
    if (n) memcpy(elems(out), elems(a), (size_t)n * 8);
    elems(out)[n] = argv[1].v.i64;
    r->tag = OE_SDT_ARRAY_OF(tag);
    r->v.ptr = out;
}

/* In place: removing shortens, and shortening never needs to move anything.
 * That is why `remove` is a statement and `append` is a value. */
void oe_ary_remove(OpenEPL_Slot *r, int32_t c, OpenEPL_Slot *argv) {
    (void)c; (void)r;
    OpenEPL_Array *a = arg_ary(argv, 0);
    int32_t i = oe_arg_int(argv, 1);
    if (!a || i < 0 || i >= a->len) {
        char msg[96];
        snprintf(msg, sizeof msg, "index %d is outside a list of %d element(s)",
                 (int)i, (int)ary_len(a));
        oe_error_set(OE_ERR_OUT_OF_RANGE, msg);
        return;
    }
    memmove(elems(a) + i, elems(a) + i + 1, (size_t)(a->len - i - 1) * 8);
    a->len--;
    oe_error_clear();
}

void oe_ary_sort(OpenEPL_Slot *r, int32_t c, OpenEPL_Slot *argv) {
    (void)c; (void)r;
    OpenEPL_Array *a = arg_ary(argv, 0);
    if (!a || a->len < 2) return;
    g_sort_tag = a->elem_tag;
    qsort(elems(a), (size_t)a->len, 8, sort_cmp);
}

static int32_t find_elem(OpenEPL_Array *a, int64_t want) {
    for (int32_t i = 0; i < ary_len(a); i++) {
        if (elem_cmp(a->elem_tag, elems(a)[i], want) == 0) return i;
    }
    return -1;
}

void oe_ary_contains(OpenEPL_Slot *r, int32_t c, OpenEPL_Slot *argv) {
    (void)c;
    oe_ret_bool(r, find_elem(arg_ary(argv, 0), argv[1].v.i64) >= 0);
}

void oe_ary_index_of(OpenEPL_Slot *r, int32_t c, OpenEPL_Slot *argv) {
    (void)c;
    oe_ret_int(r, find_elem(arg_ary(argv, 0), argv[1].v.i64));
}

void oe_ary_join(OpenEPL_Slot *r, int32_t c, OpenEPL_Slot *argv) {
    (void)c;
    OpenEPL_Array *a = arg_ary(argv, 0);
    const char *sep = oe_arg_text(argv, 1);
    if (!sep) sep = "";
    size_t seplen = strlen(sep), total = 0;
    int32_t n = ary_len(a);
    char buf[64];
    /* Measure, then fill: one allocation of the right size, no growing. */
    for (int32_t i = 0; i < n; i++) {
        if (a->elem_tag == OE_SDT_TEXT) {
            total += strlen(as_text(elems(a)[i]));
        } else {
            elem_text(a->elem_tag, elems(a)[i], buf, sizeof buf);
            total += strlen(buf);
        }
        if (i + 1 < n) total += seplen;
    }
    char *out = (char *)oe_malloc((long)total + 1);
    if (!out) { oe_ret_text(r, NULL); return; }
    char *w = out;
    for (int32_t i = 0; i < n; i++) {
        const char *piece;
        if (a->elem_tag == OE_SDT_TEXT) {
            piece = as_text(elems(a)[i]);
        } else {
            elem_text(a->elem_tag, elems(a)[i], buf, sizeof buf);
            piece = buf;
        }
        size_t plen = strlen(piece);
        memcpy(w, piece, plen); w += plen;
        if (i + 1 < n) { memcpy(w, sep, seplen); w += seplen; }
    }
    *w = '\0';
    oe_ret_text(r, out);
}

/* The other direction: text in, list out.  This is what a program reads a file
 * into before it can do anything with the lines. */
void oe_ary_split(OpenEPL_Slot *r, int32_t c, OpenEPL_Slot *argv) {
    (void)c;
    const char *s = oe_arg_text(argv, 0), *sep = oe_arg_text(argv, 1);
    if (!s) s = "";
    /* An empty separator has no answer that is not arbitrary — every position
     * matches it — so it is refused rather than guessed at. */
    if (!sep || !*sep) {
        oe_error_set(OE_ERR_INVALID_ARG, "split needs a separator to split on");
        r->tag = OE_SDT_ARRAY_OF(OE_SDT_TEXT);
        r->v.ptr = oe_ary_new(OE_SDT_TEXT, 0);
        return;
    }
    size_t seplen = strlen(sep);
    int32_t n = 1;
    for (const char *p = s; (p = strstr(p, sep)); p += seplen) n++;
    OpenEPL_Array *out = (OpenEPL_Array *)oe_ary_new(OE_SDT_TEXT, n);
    if (!out) { r->tag = OE_SDT_ARRAY_OF(OE_SDT_TEXT); r->v.ptr = NULL; return; }
    const char *p = s;
    for (int32_t i = 0; i < n; i++) {
        const char *hit = strstr(p, sep);
        size_t len = hit ? (size_t)(hit - p) : strlen(p);
        char *piece = (char *)oe_malloc((long)len + 1);
        if (!piece) break;
        memcpy(piece, p, len);
        piece[len] = '\0';
        elems(out)[i] = (int64_t)(intptr_t)piece;
        if (!hit) break;
        p = hit + seplen;
    }
    r->tag = OE_SDT_ARRAY_OF(OE_SDT_TEXT);
    r->v.ptr = out;
    oe_error_clear();
}

/* --- byte-sets --------------------------------------------------------- */

void oe_bin_make(OpenEPL_Slot *r, int32_t c, OpenEPL_Slot *argv) {
    (void)c;
    r->tag = OE_SDT_BIN;
    r->v.ptr = oe_bin_new(oe_arg_int(argv, 0));
}

void oe_bin_size(OpenEPL_Slot *r, int32_t c, OpenEPL_Slot *argv) {
    (void)c;
    OpenEPL_Bin *b = arg_bin(argv, 0);
    oe_ret_int(r, b ? b->len : 0);
}

void oe_bin_byte(OpenEPL_Slot *r, int32_t c, OpenEPL_Slot *argv) {
    (void)c;
    oe_ret_int(r, oe_bin_at(argv[0].v.ptr, oe_arg_int(argv, 1)));
}

void oe_bin_put(OpenEPL_Slot *r, int32_t c, OpenEPL_Slot *argv) {
    (void)c; (void)r;
    oe_bin_set(argv[0].v.ptr, oe_arg_int(argv, 1), oe_arg_int(argv, 2));
}

/* Text is UTF-8, so its bytes ARE its encoding: the round trip is exact, and
 * the byte count of text with an accent in it is larger than its length. */
void oe_bin_from_text(OpenEPL_Slot *r, int32_t c, OpenEPL_Slot *argv) {
    (void)c;
    const char *s = oe_arg_text(argv, 0);
    if (!s) s = "";
    size_t n = strlen(s);
    OpenEPL_Bin *b = (OpenEPL_Bin *)oe_bin_new((int32_t)n);
    if (b) memcpy(bin_bytes(b), s, n);
    r->tag = OE_SDT_BIN;
    r->v.ptr = b;
}

/* A NUL byte anywhere would truncate the result, so it stops there rather than
 * handing back text whose length disagrees with the bytes behind it. */
void oe_bin_to_text(OpenEPL_Slot *r, int32_t c, OpenEPL_Slot *argv) {
    (void)c;
    OpenEPL_Bin *b = arg_bin(argv, 0);
    int32_t n = b ? b->len : 0;
    for (int32_t i = 0; i < n; i++) {
        if (bin_bytes(b)[i] == 0) { n = i; break; }
    }
    char *out = (char *)oe_malloc((long)n + 1);
    if (!out) { oe_ret_text(r, NULL); return; }
    if (n) memcpy(out, bin_bytes(b), (size_t)n);
    out[n] = '\0';
    oe_ret_text(r, out);
}
