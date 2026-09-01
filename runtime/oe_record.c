/* Records — a name for a group of related values.
 *
 * One runtime-owned allocation whose layout is stated in openepl_core.h, held
 * in the slot as a pointer, allocated through oe_malloc: the same three
 * sentences that describe an array, and deliberately so.  A second ownership
 * model would need a second exit path, and E_DestroyRes already releases this
 * one.
 *
 * A record is therefore a REFERENCE.  Two names for one record are two names
 * for the same fields, and `p.x = 5` is seen through both — the bargain
 * `xs[1] = 2` already makes.  Copying instead would make the record the one
 * aggregate in the language that behaves unlike the rest, and would silently
 * turn passing one to a subroutine into a deep copy of everything it holds.
 *
 * Fields are reached by POSITION, counting from 1, because which record a
 * value is and where a field sits inside it are both compile-time facts.  No
 * field name reaches a shipped binary, and no lookup happens at run time.
 */
#include <string.h>
#include <stdio.h>
#include "openepl_core.h"

static int64_t *fields(OpenEPL_Record *r) { return (int64_t *)(r + 1); }

void *oe_rec_new(int32_t field_count) {
    if (field_count < 0) field_count = 0;
    OpenEPL_Record *r = (OpenEPL_Record *)oe_malloc(
        (long)sizeof(OpenEPL_Record) + (long)field_count * 8);
    if (!r) return NULL;
    r->count = field_count;
    r->_pad = 0;
    memset(fields(r), 0, (size_t)field_count * 8);
    return r;
}

/* The compiler works out every index, so a bad one means the compiler and this
 * file disagree about a layout — which is worth a loud, specific failure
 * rather than a read of whatever follows the record. */
static int in_range(OpenEPL_Record *r, int32_t i, const char *what) {
    if (r && i >= 1 && i <= r->count) return 1;
    char msg[96];
    snprintf(msg, sizeof msg, "field %d of a record with %d field(s) cannot be %s",
             (int)i, r ? (int)r->count : 0, what);
    oe_error_set(OE_ERR_OUT_OF_RANGE, msg);
    return 0;
}

int64_t oe_rec_get(void *p, int32_t i) {
    OpenEPL_Record *r = (OpenEPL_Record *)p;
    if (!in_range(r, i, "read")) return 0;
    oe_error_clear();
    return fields(r)[i - 1];
}

void oe_rec_set(void *p, int32_t i, int64_t v) {
    OpenEPL_Record *r = (OpenEPL_Record *)p;
    if (!in_range(r, i, "written")) return;
    fields(r)[i - 1] = v;
    oe_error_clear();
}
