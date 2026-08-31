/* Runtime-owned memory + the NRS_* notification channel.
 *
 * All EPL-data allocation flows through here, so ownership is consistent across
 * the program and every library.  Allocations are tracked in a singly-linked
 * list and released in E_DestroyRes — which retires the Phase-1 leak: text
 * results now live until program exit, then are freed. */
#include <stdio.h>
#include <stdlib.h>
#include "openepl_core.h"

typedef struct Block { struct Block *next; } Block; /* header precedes payload */

static Block *g_blocks = NULL;

void *E_MAlloc(long size) {
    if (size < 0) size = 0;
    Block *b = (Block *)malloc(sizeof(Block) + (size_t)size);
    if (!b) { oe_runtime_error("out of memory"); return NULL; }
    b->next = g_blocks;
    g_blocks = b;
    return (void *)(b + 1);
}

static Block *unlink_block(void *p) {
    if (!p) return NULL;
    Block *want = (Block *)p - 1;
    Block **pp = &g_blocks;
    while (*pp) {
        if (*pp == want) { *pp = want->next; return want; }
        pp = &(*pp)->next;
    }
    return NULL; /* not runtime-owned; ignore */
}

void E_MFree(void *p) {
    Block *b = unlink_block(p);
    if (b) free(b);
}

void *E_MRealloc(void *p, long size) {
    if (!p) return E_MAlloc(size);
    Block *b = unlink_block(p);
    if (!b) return NULL;
    Block *nb = (Block *)realloc(b, sizeof(Block) + (size_t)(size < 0 ? 0 : size));
    if (!nb) { oe_runtime_error("out of memory"); return NULL; }
    nb->next = g_blocks;
    g_blocks = nb;
    return (void *)(nb + 1);
}

/* The single notification entry point libraries call (abi/openepl_abi.h). */
void *oe_notify(int32_t msg, void *p1, void *p2) {
    switch (msg) {
        case OE_NRS_MALLOC:   return E_MAlloc((long)(size_t)p1);
        case OE_NRS_MFREE:    E_MFree(p1); return NULL;
        case OE_NRS_MREALLOC: return E_MRealloc(p1, (long)(size_t)p2);
        case OE_NRS_FREE_ARY: /* byte-set/array free — Phase 3 */ return NULL;
        case OE_NRS_RUNTIME_ERR:
            fprintf(stderr, "openepl runtime error: %s\n", p1 ? (const char *)p1 : "(unknown)");
            exit(1);
        default:
            return NULL;
    }
}

/* Free everything the runtime still owns (called from E_DestroyRes). */
void oe_free_all(void) {
    Block *b = g_blocks;
    while (b) { Block *n = b->next; free(b); b = n; }
    g_blocks = NULL;
}
