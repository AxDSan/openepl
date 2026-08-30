/* Core commands: text. All results are runtime-owned (oe_alloc). Inputs treat
 * NULL as the empty string (ABI text-slot rule, PRD §1.2). */
#include <ctype.h>
#include <stdlib.h>
#include <string.h>
#include "openepl_core.h"

static const char *nz(const char *s) { return s ? s : ""; }

static char *alloc_str(long len) { return (char *)oe_alloc(len + 1); }

int oe_length(const char *s) { return (int)strlen(nz(s)); }

char *oe_uppercase(const char *s) {
    s = nz(s);
    long n = (long)strlen(s);
    char *out = alloc_str(n);
    for (long i = 0; i < n; i++) out[i] = (char)toupper((unsigned char)s[i]);
    out[n] = '\0';
    return out;
}

char *oe_lowercase(const char *s) {
    s = nz(s);
    long n = (long)strlen(s);
    char *out = alloc_str(n);
    for (long i = 0; i < n; i++) out[i] = (char)tolower((unsigned char)s[i]);
    out[n] = '\0';
    return out;
}

char *oe_trim(const char *s) {
    s = nz(s);
    const char *start = s;
    while (*start && isspace((unsigned char)*start)) start++;
    const char *end = s + strlen(s);
    while (end > start && isspace((unsigned char)end[-1])) end--;
    long n = end - start;
    char *out = alloc_str(n);
    memcpy(out, start, n);
    out[n] = '\0';
    return out;
}

/* substr(s, start, count): byte offsets, clamped to bounds. */
char *oe_substr(const char *s, int start, int count) {
    s = nz(s);
    long len = (long)strlen(s);
    if (start < 0) start = 0;
    if (start > len) start = (int)len;
    if (count < 0) count = 0;
    long avail = len - start;
    long n = count < avail ? count : avail;
    char *out = alloc_str(n);
    memcpy(out, s + start, n);
    out[n] = '\0';
    return out;
}

/* find(haystack, needle): byte index of first match, or -1. Empty needle -> 0. */
int oe_find(const char *haystack, const char *needle) {
    haystack = nz(haystack);
    needle = nz(needle);
    const char *hit = strstr(haystack, needle);
    return hit ? (int)(hit - haystack) : -1;
}

char *oe_concat(const char *a, const char *b) {
    a = nz(a); b = nz(b);
    long la = (long)strlen(a), lb = (long)strlen(b);
    char *out = alloc_str(la + lb);
    memcpy(out, a, la);
    memcpy(out + la, b, lb);
    out[la + lb] = '\0';
    return out;
}

char *oe_repeat(const char *s, int times) {
    s = nz(s);
    if (times < 0) times = 0;
    long n = (long)strlen(s);
    long total = n * (long)times;
    char *out = alloc_str(total);
    char *p = out;
    for (int i = 0; i < times; i++) { memcpy(p, s, n); p += n; }
    *p = '\0';
    return out;
}

char *oe_reverse(const char *s) {
    s = nz(s);
    long n = (long)strlen(s);
    char *out = alloc_str(n);
    for (long i = 0; i < n; i++) out[i] = s[n - 1 - i];
    out[n] = '\0';
    return out;
}

/* replace(s, from, to): all non-overlapping occurrences. Empty `from` -> copy. */
char *oe_replace(const char *s, const char *from, const char *to) {
    s = nz(s); from = nz(from); to = nz(to);
    long flen = (long)strlen(from);
    if (flen == 0) {
        long n = (long)strlen(s);
        char *copy = alloc_str(n);
        memcpy(copy, s, n + 1);
        return copy;
    }
    long tlen = (long)strlen(to);

    /* Count occurrences to size the output exactly. */
    long count = 0;
    for (const char *p = s; (p = strstr(p, from)); p += flen) count++;

    long slen = (long)strlen(s);
    long outlen = slen + count * (tlen - flen);
    char *out = alloc_str(outlen);

    char *w = out;
    const char *p = s;
    while (1) {
        const char *hit = strstr(p, from);
        if (!hit) {
            long rest = (long)strlen(p);
            memcpy(w, p, rest);
            w += rest;
            break;
        }
        long chunk = hit - p;
        memcpy(w, p, chunk); w += chunk;
        memcpy(w, to, tlen);  w += tlen;
        p = hit + flen;
    }
    *w = '\0';
    return out;
}
