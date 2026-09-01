/* The JSON document tree, a strict RFC 8259 parser, and the serializer.
 *
 * Written from scratch, with no dependencies: the parser is a recursive-descent
 * reader over a NUL-terminated buffer that refuses everything the standard
 * refuses — trailing commas, single quotes, comments, unquoted keys, control
 * characters inside a string, a leading `+` or `.` on a number, more than one
 * top-level value.  Every refusal names the byte offset, because "invalid JSON"
 * without a position is not a diagnosis.
 */
#include <math.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "json_internal.h"

/* --- the tree ---------------------------------------------------------- */

JsonNode *json_node_new(int type) {
    JsonNode *v = (JsonNode *)calloc(1, sizeof(JsonNode));
    if (v) v->type = type;
    return v;
}

void json_node_free(JsonNode *v) {
    if (!v) return;
    for (long i = 0; i < v->count; i++) {
        json_node_free(v->kids[i]);
        if (v->keys) free(v->keys[i]);
    }
    free(v->kids);
    free(v->keys);
    free(v->str);
    free(v);
}

void json_node_free_payload(void *p) { json_node_free((JsonNode *)p); }

int json_node_set_str(JsonNode *v, const char *s) {
    if (!s) s = "";
    size_t n = strlen(s);
    char *copy = (char *)malloc(n + 1);
    if (!copy) return 0;
    memcpy(copy, s, n + 1);
    free(v->str);
    v->str = copy;
    return 1;
}

/* Grow both parallel arrays together; `keys` only exists for objects. */
static int json_reserve(JsonNode *c, int with_keys) {
    if (c->count < c->cap) return 1;
    long want = c->cap ? c->cap * 2 : 8;
    JsonNode **k = (JsonNode **)realloc(c->kids, (size_t)want * sizeof(JsonNode *));
    if (!k) return 0;
    c->kids = k;
    if (with_keys) {
        char **ks = (char **)realloc(c->keys, (size_t)want * sizeof(char *));
        if (!ks) return 0;
        c->keys = ks;
    }
    c->cap = want;
    return 1;
}

int json_arr_push(JsonNode *a, JsonNode *child) {
    if (!json_reserve(a, 0)) return 0;
    a->kids[a->count++] = child;
    return 1;
}

long json_obj_index(const JsonNode *o, const char *key) {
    if (o->type != JSON_T_OBJ || !o->keys) return -1;
    for (long i = 0; i < o->count; i++) {
        if (o->keys[i] && strcmp(o->keys[i], key) == 0) return i;
    }
    return -1;
}

/* Setting an existing member replaces its value and keeps its position, so
 * rewriting a field does not shuffle the document. */
int json_obj_put(JsonNode *o, const char *key, JsonNode *child) {
    long at = json_obj_index(o, key);
    if (at >= 0) {
        json_node_free(o->kids[at]);
        o->kids[at] = child;
        return 1;
    }
    if (!json_reserve(o, 1)) return 0;
    size_t n = strlen(key);
    char *k = (char *)malloc(n + 1);
    if (!k) return 0;
    memcpy(k, key, n + 1);
    o->keys[o->count] = k;
    o->kids[o->count] = child;
    o->count++;
    return 1;
}

void json_container_remove(JsonNode *c, long i) {
    if (i < 0 || i >= c->count) return;
    json_node_free(c->kids[i]);
    if (c->keys) free(c->keys[i]);
    for (long j = i + 1; j < c->count; j++) {
        c->kids[j - 1] = c->kids[j];
        if (c->keys) c->keys[j - 1] = c->keys[j];
    }
    c->count--;
}

/* --- the parser -------------------------------------------------------- */

#define JSON_MAX_DEPTH 200

typedef struct {
    const char *s;
    size_t      i;
    char       *err;
    size_t      errcap;
    int         depth;
} JsonP;

static void json_fail(JsonP *p, const char *what) {
    if (p->err && p->errcap && p->err[0] == '\0')
        snprintf(p->err, p->errcap, "%s at byte %lu", what, (unsigned long)p->i);
}

static int json_failed(JsonP *p) { return p->err && p->err[0] != '\0'; }

static void json_ws(JsonP *p) {
    for (;;) {
        char c = p->s[p->i];
        if (c == ' ' || c == '\t' || c == '\n' || c == '\r') p->i++;
        else return;
    }
}

static JsonNode *json_parse_value(JsonP *p);

static void json_utf8_put(char *out, size_t *n, unsigned long cp) {
    if (cp < 0x80) {
        out[(*n)++] = (char)cp;
    } else if (cp < 0x800) {
        out[(*n)++] = (char)(0xC0 | (cp >> 6));
        out[(*n)++] = (char)(0x80 | (cp & 0x3F));
    } else if (cp < 0x10000) {
        out[(*n)++] = (char)(0xE0 | (cp >> 12));
        out[(*n)++] = (char)(0x80 | ((cp >> 6) & 0x3F));
        out[(*n)++] = (char)(0x80 | (cp & 0x3F));
    } else {
        out[(*n)++] = (char)(0xF0 | (cp >> 18));
        out[(*n)++] = (char)(0x80 | ((cp >> 12) & 0x3F));
        out[(*n)++] = (char)(0x80 | ((cp >> 6) & 0x3F));
        out[(*n)++] = (char)(0x80 | (cp & 0x3F));
    }
}

static int json_hex4(JsonP *p, unsigned long *out) {
    unsigned long v = 0;
    for (int k = 0; k < 4; k++) {
        char c = p->s[p->i];
        int d;
        if (c >= '0' && c <= '9') d = c - '0';
        else if (c >= 'a' && c <= 'f') d = c - 'a' + 10;
        else if (c >= 'A' && c <= 'F') d = c - 'A' + 10;
        else { json_fail(p, "\\u needs four hex digits"); return 0; }
        v = v * 16 + (unsigned long)d;
        p->i++;
    }
    *out = v;
    return 1;
}

/* A decoded string is never longer than its source, so one allocation of the
 * remaining input length is always enough and never has to grow. */
static char *json_parse_string_raw(JsonP *p) {
    if (p->s[p->i] != '"') { json_fail(p, "expected a string"); return NULL; }
    p->i++;
    size_t cap = strlen(p->s + p->i) + 1;
    char *out = (char *)malloc(cap);
    if (!out) { json_fail(p, "out of memory"); return NULL; }
    size_t n = 0;
    for (;;) {
        unsigned char c = (unsigned char)p->s[p->i];
        if (c == '"') { p->i++; out[n] = '\0'; return out; }
        if (c == '\0') { json_fail(p, "unterminated string"); free(out); return NULL; }
        if (c < 0x20) { json_fail(p, "raw control character in a string"); free(out); return NULL; }
        if (c != '\\') { out[n++] = (char)c; p->i++; continue; }
        p->i++;
        char e = p->s[p->i];
        p->i++;
        switch (e) {
        case '"':  out[n++] = '"';  break;
        case '\\': out[n++] = '\\'; break;
        case '/':  out[n++] = '/';  break;
        case 'b':  out[n++] = '\b'; break;
        case 'f':  out[n++] = '\f'; break;
        case 'n':  out[n++] = '\n'; break;
        case 'r':  out[n++] = '\r'; break;
        case 't':  out[n++] = '\t'; break;
        case 'u': {
            unsigned long cp;
            if (!json_hex4(p, &cp)) { free(out); return NULL; }
            if (cp >= 0xD800 && cp <= 0xDBFF) {           /* leading surrogate */
                if (p->s[p->i] == '\\' && p->s[p->i + 1] == 'u') {
                    p->i += 2;
                    unsigned long lo;
                    if (!json_hex4(p, &lo)) { free(out); return NULL; }
                    if (lo < 0xDC00 || lo > 0xDFFF) {
                        json_fail(p, "bad surrogate pair"); free(out); return NULL;
                    }
                    cp = 0x10000 + ((cp - 0xD800) << 10) + (lo - 0xDC00);
                } else {
                    json_fail(p, "lone surrogate"); free(out); return NULL;
                }
            } else if (cp >= 0xDC00 && cp <= 0xDFFF) {
                json_fail(p, "lone surrogate"); free(out); return NULL;
            }
            json_utf8_put(out, &n, cp);
            break;
        }
        default:
            p->i--;                                   /* point at the escape  */
            json_fail(p, "unknown escape");
            free(out);
            return NULL;
        }
    }
}

static JsonNode *json_parse_number(JsonP *p) {
    size_t start = p->i;
    if (p->s[p->i] == '-') p->i++;
    if (p->s[p->i] == '0') {
        p->i++;
    } else if (p->s[p->i] >= '1' && p->s[p->i] <= '9') {
        while (p->s[p->i] >= '0' && p->s[p->i] <= '9') p->i++;
    } else {
        json_fail(p, "expected a digit");
        return NULL;
    }
    if (p->s[p->i] == '.') {
        p->i++;
        if (p->s[p->i] < '0' || p->s[p->i] > '9') { json_fail(p, "expected a digit after '.'"); return NULL; }
        while (p->s[p->i] >= '0' && p->s[p->i] <= '9') p->i++;
    }
    if (p->s[p->i] == 'e' || p->s[p->i] == 'E') {
        p->i++;
        if (p->s[p->i] == '+' || p->s[p->i] == '-') p->i++;
        if (p->s[p->i] < '0' || p->s[p->i] > '9') { json_fail(p, "expected a digit in the exponent"); return NULL; }
        while (p->s[p->i] >= '0' && p->s[p->i] <= '9') p->i++;
    }
    JsonNode *v = json_node_new(JSON_T_NUM);
    if (!v) { json_fail(p, "out of memory"); return NULL; }
    v->num = strtod(p->s + start, NULL);
    return v;
}

static int json_word(JsonP *p, const char *w) {
    size_t n = strlen(w);
    if (strncmp(p->s + p->i, w, n) != 0) return 0;
    p->i += n;
    return 1;
}

static JsonNode *json_parse_value(JsonP *p) {
    if (p->depth >= JSON_MAX_DEPTH) { json_fail(p, "nested too deeply"); return NULL; }
    json_ws(p);
    char c = p->s[p->i];
    switch (c) {
    case '\0':
        json_fail(p, "unexpected end of input");
        return NULL;
    case '"': {
        char *s = json_parse_string_raw(p);
        if (!s) return NULL;
        JsonNode *v = json_node_new(JSON_T_STR);
        if (!v) { free(s); json_fail(p, "out of memory"); return NULL; }
        v->str = s;
        return v;
    }
    case 't': case 'f': {
        int truth = (c == 't');
        if (!json_word(p, truth ? "true" : "false")) { json_fail(p, "unexpected character"); return NULL; }
        JsonNode *v = json_node_new(JSON_T_BOOL);
        if (!v) { json_fail(p, "out of memory"); return NULL; }
        v->bval = truth;
        return v;
    }
    case 'n': {
        if (!json_word(p, "null")) { json_fail(p, "unexpected character"); return NULL; }
        JsonNode *v = json_node_new(JSON_T_NULL);
        if (!v) json_fail(p, "out of memory");
        return v;
    }
    case '[': {
        p->i++;
        p->depth++;
        JsonNode *a = json_node_new(JSON_T_ARR);
        if (!a) { json_fail(p, "out of memory"); return NULL; }
        json_ws(p);
        if (p->s[p->i] == ']') { p->i++; p->depth--; return a; }
        for (;;) {
            JsonNode *item = json_parse_value(p);
            if (!item) { json_node_free(a); return NULL; }
            if (!json_arr_push(a, item)) {
                json_node_free(item); json_node_free(a);
                json_fail(p, "out of memory");
                return NULL;
            }
            json_ws(p);
            if (p->s[p->i] == ',') { p->i++; json_ws(p); continue; }
            if (p->s[p->i] == ']') { p->i++; p->depth--; return a; }
            json_fail(p, "expected ',' or ']'");
            json_node_free(a);
            return NULL;
        }
    }
    case '{': {
        p->i++;
        p->depth++;
        JsonNode *o = json_node_new(JSON_T_OBJ);
        if (!o) { json_fail(p, "out of memory"); return NULL; }
        json_ws(p);
        if (p->s[p->i] == '}') { p->i++; p->depth--; return o; }
        for (;;) {
            json_ws(p);
            char *key = json_parse_string_raw(p);
            if (!key) { json_node_free(o); return NULL; }
            json_ws(p);
            if (p->s[p->i] != ':') {
                json_fail(p, "expected ':'");
                free(key); json_node_free(o);
                return NULL;
            }
            p->i++;
            JsonNode *val = json_parse_value(p);
            if (!val) { free(key); json_node_free(o); return NULL; }
            if (!json_obj_put(o, key, val)) {
                json_fail(p, "out of memory");
                free(key); json_node_free(val); json_node_free(o);
                return NULL;
            }
            free(key);
            json_ws(p);
            if (p->s[p->i] == ',') { p->i++; continue; }
            if (p->s[p->i] == '}') { p->i++; p->depth--; return o; }
            json_fail(p, "expected ',' or '}'");
            json_node_free(o);
            return NULL;
        }
    }
    default:
        if (c == '-' || (c >= '0' && c <= '9')) return json_parse_number(p);
        json_fail(p, "unexpected character");
        return NULL;
    }
}

JsonNode *json_parse_text(const char *src, char *errbuf, size_t errcap) {
    if (errbuf && errcap) errbuf[0] = '\0';
    JsonP p;
    p.s = src ? src : "";
    p.i = 0;
    p.err = errbuf;
    p.errcap = errcap;
    p.depth = 0;

    JsonNode *v = json_parse_value(&p);
    if (!v) {
        if (!json_failed(&p)) json_fail(&p, "invalid JSON");
        return NULL;
    }
    json_ws(&p);
    if (p.s[p.i] != '\0') {
        json_fail(&p, "trailing text after the document");
        json_node_free(v);
        return NULL;
    }
    return v;
}

/* --- the serializer ---------------------------------------------------- */

typedef struct {
    char  *p;
    size_t len, cap;
    int    bad;
} JsonBuf;

static void json_buf_need(JsonBuf *b, size_t extra) {
    if (b->bad) return;
    if (b->len + extra + 1 <= b->cap) return;
    size_t want = b->cap ? b->cap : 128;
    while (want < b->len + extra + 1) want *= 2;
    char *q = (char *)realloc(b->p, want);
    if (!q) { b->bad = 1; return; }
    b->p = q;
    b->cap = want;
}

static void json_buf_add(JsonBuf *b, const char *s, size_t n) {
    json_buf_need(b, n);
    if (b->bad) return;
    memcpy(b->p + b->len, s, n);
    b->len += n;
    b->p[b->len] = '\0';
}

static void json_buf_str(JsonBuf *b, const char *s) { json_buf_add(b, s, strlen(s)); }
static void json_buf_ch(JsonBuf *b, char c) { json_buf_add(b, &c, 1); }

static void json_buf_quoted(JsonBuf *b, const char *s) {
    json_buf_ch(b, '"');
    for (const unsigned char *q = (const unsigned char *)(s ? s : ""); *q; q++) {
        switch (*q) {
        case '"':  json_buf_str(b, "\\\""); break;
        case '\\': json_buf_str(b, "\\\\"); break;
        case '\b': json_buf_str(b, "\\b");  break;
        case '\f': json_buf_str(b, "\\f");  break;
        case '\n': json_buf_str(b, "\\n");  break;
        case '\r': json_buf_str(b, "\\r");  break;
        case '\t': json_buf_str(b, "\\t");  break;
        default:
            if (*q < 0x20) {
                char esc[7];
                snprintf(esc, sizeof esc, "\\u%04x", (unsigned)*q);
                json_buf_str(b, esc);
            } else {
                json_buf_ch(b, (char)*q);   /* UTF-8 passes through unchanged */
            }
        }
    }
    json_buf_ch(b, '"');
}

/* The shortest decimal form that reads back as the same double, so a document
 * that is round-tripped does not grow a tail of 0000000001. */
static void json_buf_num(JsonBuf *b, double d) {
    char tmp[64];
    if (!(d == d) || d > 1.7976931348623157e308 || d < -1.7976931348623157e308) {
        json_buf_str(b, "null");             /* NaN and infinity are not JSON */
        return;
    }
    if (d == floor(d) && d >= -9.007199254740992e15 && d <= 9.007199254740992e15) {
        snprintf(tmp, sizeof tmp, "%lld", (long long)d);
        json_buf_str(b, tmp);
        return;
    }
    for (int prec = 15; prec <= 17; prec++) {
        snprintf(tmp, sizeof tmp, "%.*g", prec, d);
        if (strtod(tmp, NULL) == d) break;
    }
    json_buf_str(b, tmp);
}

static void json_indent(JsonBuf *b, int depth) {
    for (int i = 0; i < depth; i++) json_buf_str(b, "  ");
}

static void json_emit(JsonBuf *b, const JsonNode *v, int pretty, int depth) {
    switch (v->type) {
    case JSON_T_NULL: json_buf_str(b, "null"); return;
    case JSON_T_BOOL: json_buf_str(b, v->bval ? "true" : "false"); return;
    case JSON_T_NUM:  json_buf_num(b, v->num); return;
    case JSON_T_STR:  json_buf_quoted(b, v->str); return;
    case JSON_T_ARR:
        if (v->count == 0) { json_buf_str(b, "[]"); return; }
        json_buf_ch(b, '[');
        for (long i = 0; i < v->count; i++) {
            if (i) json_buf_ch(b, ',');
            if (pretty) { json_buf_ch(b, '\n'); json_indent(b, depth + 1); }
            json_emit(b, v->kids[i], pretty, depth + 1);
        }
        if (pretty) { json_buf_ch(b, '\n'); json_indent(b, depth); }
        json_buf_ch(b, ']');
        return;
    case JSON_T_OBJ:
        if (v->count == 0) { json_buf_str(b, "{}"); return; }
        json_buf_ch(b, '{');
        for (long i = 0; i < v->count; i++) {
            if (i) json_buf_ch(b, ',');
            if (pretty) { json_buf_ch(b, '\n'); json_indent(b, depth + 1); }
            json_buf_quoted(b, v->keys[i]);
            json_buf_ch(b, ':');
            if (pretty) json_buf_ch(b, ' ');
            json_emit(b, v->kids[i], pretty, depth + 1);
        }
        if (pretty) { json_buf_ch(b, '\n'); json_indent(b, depth); }
        json_buf_ch(b, '}');
        return;
    default:
        json_buf_str(b, "null");
        return;
    }
}

char *json_write(const JsonNode *v, int pretty) {
    JsonBuf b;
    b.p = NULL; b.len = 0; b.cap = 0; b.bad = 0;
    json_buf_need(&b, 0);
    if (b.bad) return NULL;
    b.p[0] = '\0';
    json_emit(&b, v, pretty, 0);
    if (b.bad) { free(b.p); return NULL; }
    return b.p;
}
