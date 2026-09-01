/* The "net" support library — TCP and plain HTTP over raw BSD sockets.
 *
 * No dependencies: getaddrinfo(3), socket(2), poll(2) and the C library, which
 * is all a program needs to speak TCP and HTTP/1.1.
 *
 * THERE IS NO TLS, AND THAT IS A DECISION, NOT AN OVERSIGHT.
 * --------------------------------------------------------------------------
 * OpenEPL links a program statically, so a TLS stack here would be vendored
 * into every shipped binary — megabytes of code, a certificate store to keep
 * current, and a security-critical dependency the project would have to patch
 * on someone else's schedule.  Hand-rolling one is worse still.  So this
 * library speaks http:// only.
 *
 * The consequence is stated loudly rather than papered over: an https:// URL
 * FAILS with OE_ERR_UNSUPPORTED and a message saying https is not supported
 * yet.  It is never silently downgraded to http — a downgrade would put a
 * user's password or token on the wire in the clear, and a program that
 * "worked" would be the vulnerability.  A redirect to an https:// location
 * fails the same way, for the same reason.  When a program needs TLS today,
 * the honest answer is to shell out to curl through the `process` library.
 *
 * Timeouts are not optional either.  A program that hangs forever on a dead
 * host is the worst failure mode in a networking library, because it looks
 * like a working program that is merely slow.  Every connect, send and receive
 * is bounded by net_timeout_set (10 seconds by default) and fails with
 * ETIMEDOUT when it expires.
 *
 * Text is NUL-terminated, so net_http_get and net_tcp_receive are for textual
 * protocols; binary content is what net_http_download is for — it writes bytes
 * to a file and never through a text slot.
 */
#include "net_internal.h"

/* --- small helpers, shared with net_httpd.c (net_internal.h) ---------- */

#ifdef _WIN32
/* Winsock refuses every call until WSAStartup has run.  The runtime is
 * single-threaded — see the sort-tag comment in runtime/oe_array.c — so a
 * plain flag is enough, and a library has no initialiser hook to do it in. */
static int g_wsa_ready = 0;
int net_start(void) {
    if (g_wsa_ready) return 1;
    WSADATA wsa;
    if (WSAStartup(MAKEWORD(2, 2), &wsa) != 0) return 0;
    g_wsa_ready = 1;
    return 1;
}
int net_set_nonblocking(int fd) {
    u_long on = 1;
    return ioctlsocket(fd, FIONBIO, &on) == 0;
}
int64_t net_now_ms(void) { return (int64_t)GetTickCount64(); }
#else
int net_start(void) { return 1; }
int net_set_nonblocking(int fd) {
    int flags = fcntl(fd, F_GETFL, 0);
    if (flags < 0) return 0;
    return fcntl(fd, F_SETFL, flags | O_NONBLOCK) == 0;
}
int64_t net_now_ms(void) {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return (int64_t)ts.tv_sec * 1000 + ts.tv_nsec / 1000000;
}
#endif

void net_fail(int code, const char *what) {
#ifdef _WIN32
    char msg[128];
    snprintf(msg, sizeof msg, "%s: Winsock error %d", what, code);
    oe_error_set((int32_t)code, msg);
#else
    oe_error_set_errno(code, what);
#endif
}

const char *net_nz(const char *s) { return s ? s : ""; }

char *net_text(const char *p, size_t n) {
    char *o = (char *)oe_malloc((long)n + 1);
    if (!o) return NULL;                 /* oe_malloc aborts, but be explicit */
    if (n) memcpy(o, p, n);
    o[n] = '\0';
    return o;
}
char *net_empty(void) { return net_text("", 0); }

int net_buf_add(NetBuf *b, const char *s, size_t n) {
    if (b->n + n + 1 > b->cap) {
        size_t want = b->cap ? b->cap * 2 : 1024;
        while (want < b->n + n + 1) want *= 2;
        char *q = (char *)realloc(b->p, want);
        if (!q) return 0;
        b->p = q;
        b->cap = want;
    }
    memcpy(b->p + b->n, s, n);
    b->n += n;
    b->p[b->n] = '\0';
    return 1;
}
void net_buf_free(NetBuf *b) { free(b->p); b->p = NULL; b->n = b->cap = 0; }

/* A response larger than this is refused rather than silently truncated. */
#define NET_MAX_BODY (64L * 1024L * 1024L)
#define NET_MAX_LINE (1L * 1024L * 1024L)

/* --- global state ----------------------------------------------------- */

static int   g_timeout_ms = 10000;
static int   g_status = 0;        /* status code of the last HTTP response   */
static char *g_headers = NULL;    /* its header block, for net_http_header   */

/* --- connecting ------------------------------------------------------- */

/* Connect with a deadline.  A blocking connect() ignores SO_SNDTIMEO on Linux,
 * so the socket goes non-blocking for the connect and back to blocking (with
 * receive/send timeouts) afterwards. */
static int net_dial(const char *host, int port) {
    if (!net_start()) {
        oe_error_set(OE_ERR_UNSUPPORTED, "Winsock could not be started");
        return -1;
    }
    if (!host || !*host) {
        oe_error_set(OE_ERR_INVALID_ARG, "host is empty");
        return -1;
    }
    if (port <= 0 || port > 65535) {
        oe_error_set(OE_ERR_INVALID_ARG, "port must be 1..65535");
        return -1;
    }

    char portstr[16];
    snprintf(portstr, sizeof portstr, "%d", port);

    struct addrinfo hints, *list = NULL;
    memset(&hints, 0, sizeof hints);
    hints.ai_family = AF_UNSPEC;
    hints.ai_socktype = SOCK_STREAM;

    int rc = getaddrinfo(host, portstr, &hints, &list);
    if (rc != 0 || !list) {
        /* A resolver failure is not an errno failure: gai_strerror carries the
         * detail, so it goes in the message and the code says "bad input". */
        char msg[256];
        snprintf(msg, sizeof msg, "resolve %s: %s", host, gai_strerror(rc));
        oe_error_set(OE_ERR_INVALID_ARG, msg);
        return -1;
    }

    int saved = 0;
    const char *what = "connect";
    int fd = -1;

    for (struct addrinfo *ai = list; ai; ai = ai->ai_next) {
        fd = socket(ai->ai_family, ai->ai_socktype, ai->ai_protocol);
        if (fd < 0) { saved = net_errno(); what = "socket"; continue; }

#ifdef _WIN32
        u_long nonblocking = 1;
        int flags = ioctlsocket(fd, FIONBIO, &nonblocking) == 0 ? 0 : -1;
#else
        int flags = fcntl(fd, F_GETFL, 0);
        if (flags >= 0) fcntl(fd, F_SETFL, flags | O_NONBLOCK);
#endif

        int r = connect(fd, ai->ai_addr, (socklen_t)ai->ai_addrlen);
        int e = net_errno();
        if (r < 0 && e == NET_EINPROGRESS) {
            struct pollfd pfd;
            pfd.fd = fd;
            pfd.events = POLLOUT;
            pfd.revents = 0;
            /* On Windows this is WSAPoll, which before Windows 10 2004 did not
             * report a REFUSED connect through POLLOUT or POLLERR.  There the
             * refusal surfaces as this call's timeout rather than as
             * WSAECONNREFUSED — a slower answer, never a wrong one. */
            int pr = poll(&pfd, 1, g_timeout_ms);
            int pe = net_errno();
            if (pr == 0) { saved = NET_ETIMEDOUT; r = -1; }
            else if (pr < 0) { saved = pe; r = -1; }
            else {
                int so_err = 0;
                socklen_t len = sizeof so_err;
                if (getsockopt(fd, SOL_SOCKET, SO_ERROR, (char *)&so_err, &len) < 0) {
                    saved = net_errno();
                    r = -1;
                } else if (so_err != 0) {
                    saved = so_err;
                    r = -1;
                } else {
                    r = 0;
                }
            }
        } else if (r < 0) {
            saved = e;
        }

        if (r == 0) {
#ifdef _WIN32
            u_long blocking = 0;
            if (flags >= 0) ioctlsocket(fd, FIONBIO, &blocking);
            /* Windows wants the timeout as milliseconds in a DWORD, not as a
             * struct timeval — passing the struct sets a nonsense timeout. */
            DWORD tv = (DWORD)g_timeout_ms;
#else
            if (flags >= 0) fcntl(fd, F_SETFL, flags);   /* blocking again */
            struct timeval tv;
            tv.tv_sec = g_timeout_ms / 1000;
            tv.tv_usec = (g_timeout_ms % 1000) * 1000;
#endif
            setsockopt(fd, SOL_SOCKET, SO_RCVTIMEO, (const char *)&tv, sizeof tv);
            setsockopt(fd, SOL_SOCKET, SO_SNDTIMEO, (const char *)&tv, sizeof tv);
            freeaddrinfo(list);
            return fd;
        }

        close(fd);
        fd = -1;
        what = "connect";
    }

    freeaddrinfo(list);
    if (saved == 0) saved = NET_ECONNREFUSED;
    net_fail(saved, what);
    return -1;
}

/* --- socket handles --------------------------------------------------- */

void net_sock_close(void *payload) {
    NetSock *s = (NetSock *)payload;
    if (!s) return;
    if (s->fd >= 0) close(s->fd);
    free(s);
}

/* Refill the pushback buffer.  1 = data available, 0 = clean EOF,
 * -1 = failure (errno in *saved). */
static int net_sock_fill(NetSock *s, int *saved) {
    if (s->bpos < s->blen) return 1;
    s->bpos = s->blen = 0;
    ssize_t r = recv(s->fd, s->buf, (int)sizeof s->buf, 0);
    int e = net_errno();
    if (r > 0) { s->blen = (int)r; return 1; }
    if (r == 0) { s->eof = 1; return 0; }
    if (e == EAGAIN || e == NET_EWOULDBLOCK) e = NET_ETIMEDOUT;
    *saved = e;
    return -1;
}

/* Send everything, or fail.  A short write is not an error to report; a short
 * write silently accepted would be. */
static int net_sock_send_all(int fd, const char *p, size_t n, int *saved) {
    while (n > 0) {
        ssize_t w = send(fd, p, (int)n, MSG_NOSIGNAL);
        int e = net_errno();
        if (w > 0) { p += w; n -= (size_t)w; continue; }
        if (w < 0 && (e == EAGAIN || e == NET_EWOULDBLOCK)) e = NET_ETIMEDOUT;
        if (w < 0 && e == NET_EINTR) continue;
        *saved = (w == 0) ? NET_EPIPE : e;
        return 0;
    }
    return 1;
}

/* net_tcp_connect(text host, int port) -> int handle (0 on failure) */
void net_tcp_connect(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv) {
    (void)argc;
    const char *host = net_nz(oe_arg_text(argv, 0));
    int port = oe_arg_int(argv, 1);

    int fd = net_dial(host, port);            /* sets the slot on failure */
    if (fd < 0) { oe_ret_int(ret, 0); return; }

    NetSock *s = (NetSock *)calloc(1, sizeof *s);
    int ce = errno;                       /* nothing may intervene */
    if (!s) {
        int e = ce;
        close(fd);
        oe_error_set_errno(e, "allocate connection");
        oe_ret_int(ret, 0);
        return;
    }
    s->fd = fd;

    int32_t h = oe_handle_new(OE_HK_SOCKET, s, net_sock_close);
    if (h == 0) {                              /* oe_handle_new set the slot */
        net_sock_close(s);
        oe_ret_int(ret, 0);
        return;
    }
    oe_error_clear();
    oe_ret_int(ret, h);
}

/* net_tcp_send(int handle, text data) -> bool */
void net_tcp_send(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv) {
    (void)argc;
    NetSock *s = (NetSock *)oe_handle_resolve(oe_arg_int(argv, 0), OE_HK_SOCKET);
    if (!s) { oe_ret_bool(ret, 0); return; }   /* handle table set the slot */

    const char *data = net_nz(oe_arg_text(argv, 1));
    int saved = 0;
    if (!net_sock_send_all(s->fd, data, strlen(data), &saved)) {
        net_fail(saved, "send");
        oe_ret_bool(ret, 0);
        return;
    }
    oe_error_clear();
    oe_ret_bool(ret, 1);
}

/* net_tcp_receive(int handle, int max_bytes) -> text ("" on failure OR at end;
 * net_tcp_at_end and last_error_code tell the two apart) */
void net_tcp_receive(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv) {
    (void)argc;
    NetSock *s = (NetSock *)oe_handle_resolve(oe_arg_int(argv, 0), OE_HK_SOCKET);
    if (!s) { oe_ret_text(ret, net_empty()); return; }

    int max = oe_arg_int(argv, 1);
    if (max <= 0) {
        oe_error_set(OE_ERR_INVALID_ARG, "max_bytes must be positive");
        oe_ret_text(ret, net_empty());
        return;
    }

    int saved = 0;
    int r = net_sock_fill(s, &saved);
    if (r < 0) {
        net_fail(saved, "receive");
        oe_ret_text(ret, net_empty());
        return;
    }
    if (r == 0) { oe_error_clear(); oe_ret_text(ret, net_empty()); return; }

    int have = s->blen - s->bpos;
    if (have > max) have = max;
    char *out = net_text(s->buf + s->bpos, (size_t)have);
    s->bpos += have;
    oe_error_clear();
    oe_ret_text(ret, out);
}

/* net_tcp_receive_line(int handle) -> text, without its line ending */
void net_tcp_receive_line(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv) {
    (void)argc;
    NetSock *s = (NetSock *)oe_handle_resolve(oe_arg_int(argv, 0), OE_HK_SOCKET);
    if (!s) { oe_ret_text(ret, net_empty()); return; }

    NetBuf line = {0};
    for (;;) {
        int saved = 0;
        int r = net_sock_fill(s, &saved);
        if (r < 0) {
            net_buf_free(&line);
            net_fail(saved, "receive");
            oe_ret_text(ret, net_empty());
            return;
        }
        if (r == 0) break;                        /* end of input */

        int i = s->bpos;
        while (i < s->blen && s->buf[i] != '\n') i++;
        int take = i - s->bpos;
        if (!net_buf_add(&line, s->buf + s->bpos, (size_t)take)) {
            net_buf_free(&line);
            oe_error_set_errno(ENOMEM, "receive");
            oe_ret_text(ret, net_empty());
            return;
        }
        s->bpos = (i < s->blen) ? i + 1 : i;      /* consume the '\n' too */
        if (i < s->blen) break;                   /* found the line ending */

        if ((long)line.n > NET_MAX_LINE) {
            net_buf_free(&line);
            oe_error_set(OE_ERR_INVALID_ARG, "line longer than 1 MiB");
            oe_ret_text(ret, net_empty());
            return;
        }
    }

    if (line.n && line.p[line.n - 1] == '\r') line.n--;
    char *out = net_text(line.p ? line.p : "", line.n);
    net_buf_free(&line);
    oe_error_clear();
    oe_ret_text(ret, out);
}

/* net_tcp_at_end(int handle) -> bool.  The predicate that makes "" readable:
 * an empty receive is end-of-input here, and a failure otherwise. */
void net_tcp_at_end(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv) {
    (void)argc;
    NetSock *s = (NetSock *)oe_handle_resolve(oe_arg_int(argv, 0), OE_HK_SOCKET);
    if (!s) { oe_ret_bool(ret, 0); return; }
    oe_error_clear();
    oe_ret_bool(ret, s->eof && s->bpos >= s->blen);
}

/* net_tcp_close(int handle) -> bool */
void net_tcp_close(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv) {
    (void)argc;
    /* oe_handle_close reports stale/wrong-kind itself and clears on success. */
    oe_ret_bool(ret, oe_handle_close(oe_arg_int(argv, 0), OE_HK_SOCKET));
}

/* net_tcp_close_all() -> int, the number closed */
void net_tcp_close_all(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv) {
    (void)argc; (void)argv;
    oe_ret_int(ret, oe_handle_close_kind(OE_HK_SOCKET));   /* clears the slot */
}

/* --- timeouts --------------------------------------------------------- */

/* net_timeout_set(int milliseconds) -> bool */
void net_timeout_set(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv) {
    (void)argc;
    int ms = oe_arg_int(argv, 0);
    if (ms <= 0) {
        oe_error_set(OE_ERR_INVALID_ARG, "timeout must be positive");
        oe_ret_bool(ret, 0);
        return;
    }
    g_timeout_ms = ms;
    oe_error_clear();
    oe_ret_bool(ret, 1);
}

/* net_timeout_get() -> int.  Infallible: never touches the error slot. */
void net_timeout_get(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv) {
    (void)argc; (void)argv;
    oe_ret_int(ret, g_timeout_ms);
}

/* --- URLs ------------------------------------------------------------- */

static int net_is_unreserved(unsigned char c) {
    return (c >= 'A' && c <= 'Z') || (c >= 'a' && c <= 'z') ||
           (c >= '0' && c <= '9') || c == '-' || c == '_' || c == '.' || c == '~';
}
static int net_hexval(char c) {
    if (c >= '0' && c <= '9') return c - '0';
    if (c >= 'a' && c <= 'f') return c - 'a' + 10;
    if (c >= 'A' && c <= 'F') return c - 'A' + 10;
    return -1;
}

/* net_url_encode(text) -> text.  Percent-encoding for a form field or a query
 * value: space becomes '+', everything outside the unreserved set becomes %XX.
 * Infallible. */
void net_url_encode(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv) {
    (void)argc;
    const char *s = net_nz(oe_arg_text(argv, 0));
    size_t n = strlen(s);
    char *out = (char *)oe_malloc((long)n * 3 + 1);
    size_t o = 0;
    for (size_t i = 0; i < n; i++) {
        unsigned char c = (unsigned char)s[i];
        if (net_is_unreserved(c)) out[o++] = (char)c;
        else if (c == ' ') out[o++] = '+';
        else { static const char *H = "0123456789ABCDEF";
               out[o++] = '%'; out[o++] = H[c >> 4]; out[o++] = H[c & 15]; }
    }
    out[o] = '\0';
    oe_ret_text(ret, out);
}

/* net_url_decode(text) -> text.  Lenient by design: a stray '%' that is not
 * followed by two hex digits is kept as a literal '%', because refusing to
 * decode a URL a browser accepts would be the surprising behaviour.
 * Infallible. */
void net_url_decode(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv) {
    (void)argc;
    const char *s = net_nz(oe_arg_text(argv, 0));
    size_t n = strlen(s);
    char *out = (char *)oe_malloc((long)n + 1);
    size_t o = 0;
    for (size_t i = 0; i < n; i++) {
        if (s[i] == '+') { out[o++] = ' '; continue; }
        if (s[i] == '%' && i + 2 < n) {
            int hi = net_hexval(s[i + 1]), lo = net_hexval(s[i + 2]);
            if (hi >= 0 && lo >= 0) { out[o++] = (char)((hi << 4) | lo); i += 2; continue; }
        }
        out[o++] = s[i];
    }
    out[o] = '\0';
    oe_ret_text(ret, out);
}

/* net_host_ip(text host) -> text, the first address the resolver returns
 * ("" on failure) */
void net_host_ip(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv) {
    (void)argc;
    const char *host = net_nz(oe_arg_text(argv, 0));
    if (!*host) {
        oe_error_set(OE_ERR_INVALID_ARG, "host is empty");
        oe_ret_text(ret, net_empty());
        return;
    }

    struct addrinfo hints, *list = NULL;
    memset(&hints, 0, sizeof hints);
    hints.ai_family = AF_UNSPEC;
    hints.ai_socktype = SOCK_STREAM;

    if (!net_start()) {
        oe_error_set(OE_ERR_UNSUPPORTED, "Winsock could not be started");
        oe_ret_text(ret, net_empty());
        return;
    }
    int rc = getaddrinfo(host, NULL, &hints, &list);
    if (rc != 0 || !list) {
        char msg[256];
        snprintf(msg, sizeof msg, "resolve %s: %s", host, gai_strerror(rc));
        if (list) freeaddrinfo(list);
        oe_error_set(OE_ERR_INVALID_ARG, msg);
        oe_ret_text(ret, net_empty());
        return;
    }

    char text[INET6_ADDRSTRLEN + 1] = "";
    const char *p = NULL;
    if (list->ai_family == AF_INET) {
        struct sockaddr_in *a = (struct sockaddr_in *)list->ai_addr;
        p = inet_ntop(AF_INET, &a->sin_addr, text, sizeof text);
    } else if (list->ai_family == AF_INET6) {
        struct sockaddr_in6 *a = (struct sockaddr_in6 *)list->ai_addr;
        p = inet_ntop(AF_INET6, &a->sin6_addr, text, sizeof text);
    }
    int e = net_errno();
    freeaddrinfo(list);

    if (!p) {
        net_fail(e, "format address");
        oe_ret_text(ret, net_empty());
        return;
    }
    oe_error_clear();
    oe_ret_text(ret, net_text(text, strlen(text)));
}

/* --- HTTP ------------------------------------------------------------- */

/* Split an absolute http:// URL.  Returns 1 on success; on failure the error
 * slot is already set — including the one failure that matters most, an
 * https:// URL, which is refused rather than downgraded. */
static int net_url_split(const char *url, char *host, size_t hostsz,
                         int *port, char *path, size_t pathsz) {
    const char *sep = strstr(url, "://");
    if (!sep) {
        oe_error_set(OE_ERR_INVALID_ARG, "url must begin with http://");
        return 0;
    }
    size_t schemelen = (size_t)(sep - url);
    if (schemelen == 5 && strncasecmp(url, "https", 5) == 0) {
        oe_error_set(OE_ERR_UNSUPPORTED,
                     "https is not supported yet: this build has no TLS, and an "
                     "https url is never downgraded to http");
        return 0;
    }
    if (schemelen != 4 || strncasecmp(url, "http", 4) != 0) {
        oe_error_set(OE_ERR_INVALID_ARG, "only the http scheme is supported");
        return 0;
    }

    const char *auth = sep + 3;
    const char *at = NULL;
    for (const char *q = auth; *q && *q != '/' && *q != '?' && *q != '#'; q++)
        if (*q == '@') at = q;
    if (at) auth = at + 1;                       /* drop any user:password@ */

    const char *end = auth;
    while (*end && *end != '/' && *end != '?' && *end != '#') end++;

    const char *colon = NULL;
    if (*auth == '[') {                          /* [::1]:8080 */
        const char *rb = strchr(auth, ']');
        if (!rb || rb > end) {
            oe_error_set(OE_ERR_INVALID_ARG, "malformed ipv6 host in url");
            return 0;
        }
        if (rb + 1 < end && rb[1] == ':') colon = rb + 1;
        auth++;                                   /* strip the brackets */
        size_t hl = (size_t)(rb - auth);
        if (hl == 0 || hl >= hostsz) {
            oe_error_set(OE_ERR_INVALID_ARG, "host is empty or too long");
            return 0;
        }
        memcpy(host, auth, hl);
        host[hl] = '\0';
    } else {
        for (const char *q = auth; q < end; q++) if (*q == ':') colon = q;
        size_t hl = (size_t)((colon ? colon : end) - auth);
        if (hl == 0 || hl >= hostsz) {
            oe_error_set(OE_ERR_INVALID_ARG, "host is empty or too long");
            return 0;
        }
        memcpy(host, auth, hl);
        host[hl] = '\0';
    }

    *port = 80;
    if (colon) {
        long v = strtol(colon + 1, NULL, 10);
        if (v <= 0 || v > 65535) {
            oe_error_set(OE_ERR_INVALID_ARG, "port must be 1..65535");
            return 0;
        }
        *port = (int)v;
    }

    if (*end == '\0' || *end == '#') snprintf(path, pathsz, "/");
    else {
        size_t pl = strlen(end);
        const char *hash = strchr(end, '#');
        if (hash) pl = (size_t)(hash - end);
        if (pl >= pathsz) {
            oe_error_set(OE_ERR_INVALID_ARG, "url path is too long");
            return 0;
        }
        memcpy(path, end, pl);
        path[pl] = '\0';
    }
    return 1;
}

/* strcasestr is a GNU extension and this translation unit is compiled with no
 * feature macros, so the two characters of it that are needed live here. */
static int net_ci_contains(const char *hay, const char *needle) {
    size_t nl = strlen(needle);
    if (nl == 0) return 1;
    for (const char *p = hay; *p; p++)
        if (strncasecmp(p, needle, nl) == 0) return 1;
    return 0;
}

/* Case-insensitive header lookup in a CRLF-separated header block.
 * 1 = found (value copied), 0 = absent. */
static int net_header(const char *block, const char *name, char *out, size_t outsz) {
    if (!block) return 0;
    size_t nl = strlen(name);
    const char *line = block;
    while (line && *line) {
        const char *eol = strstr(line, "\r\n");
        size_t len = eol ? (size_t)(eol - line) : strlen(line);
        if (len > nl && strncasecmp(line, name, nl) == 0 && line[nl] == ':') {
            const char *v = line + nl + 1;
            while (*v == ' ' || *v == '\t') v++;
            size_t vl = len - (size_t)(v - line);
            if (vl >= outsz) vl = outsz - 1;
            memcpy(out, v, vl);
            out[vl] = '\0';
            return 1;
        }
        line = eol ? eol + 2 : NULL;
    }
    return 0;
}

/* Decode a chunked body in place.  0 = malformed, and the caller reports it
 * rather than handing back a body with chunk headers embedded in it. */
static int net_dechunk(const char *in, size_t n, NetBuf *out) {
    size_t i = 0;
    for (;;) {
        size_t j = i;
        while (j + 1 < n && !(in[j] == '\r' && in[j + 1] == '\n')) j++;
        if (j + 1 >= n) return 0;                       /* no size line */
        char sz[64];
        size_t sl = j - i;
        if (sl == 0 || sl >= sizeof sz) return 0;
        memcpy(sz, in + i, sl);
        sz[sl] = '\0';
        char *semi = strchr(sz, ';');
        if (semi) *semi = '\0';                         /* chunk extensions */
        char *endp = NULL;
        long len = strtol(sz, &endp, 16);
        if (endp == sz || len < 0) return 0;
        i = j + 2;
        if (len == 0) return 1;                         /* trailers ignored */
        if (i + (size_t)len > n) return 0;              /* truncated */
        if (!net_buf_add(out, in + i, (size_t)len)) return 0;
        i += (size_t)len;
        if (i + 1 < n && in[i] == '\r' && in[i + 1] == '\n') i += 2;
        else if (i >= n) return 1;          /* body ended without a final CRLF */
        else return 0;                      /* chunk not followed by CRLF */
    }
}

/* One request/response exchange, redirects followed.  1 = success and `body`
 * holds the decoded body; 0 = failure with the error slot already set. */
static int net_http_do(const char *method, const char *url,
                       const char *ctype, const char *reqbody, NetBuf *body) {
    char cur[2048];
    if (strlen(url) >= sizeof cur) {
        oe_error_set(OE_ERR_INVALID_ARG, "url is too long");
        return 0;
    }
    snprintf(cur, sizeof cur, "%s", url);

    g_status = 0;
    free(g_headers);
    g_headers = NULL;

    for (int hop = 0; hop < 5; hop++) {
        char host[256], path[1600];
        int port = 80;
        if (!net_url_split(cur, host, sizeof host, &port, path, sizeof path))
            return 0;                                   /* slot already set */

        int fd = net_dial(host, port);
        if (fd < 0) return 0;

        NetBuf req = {0};
        char head[2600];
        int hn;
        if (port == 80)
            hn = snprintf(head, sizeof head,
                "%s %s HTTP/1.1\r\nHost: %s\r\n", method, path, host);
        else
            hn = snprintf(head, sizeof head,
                "%s %s HTTP/1.1\r\nHost: %s:%d\r\n", method, path, host, port);
        net_buf_add(&req, head, (size_t)hn);
        net_buf_add(&req, "User-Agent: OpenEPL/1.0\r\n", 25);
        net_buf_add(&req, "Accept: */*\r\n", 13);
        net_buf_add(&req, "Accept-Encoding: identity\r\n", 27);
        net_buf_add(&req, "Connection: close\r\n", 19);
        if (reqbody) {
            hn = snprintf(head, sizeof head,
                          "Content-Type: %s\r\nContent-Length: %zu\r\n",
                          (ctype && *ctype) ? ctype : "application/octet-stream",
                          strlen(reqbody));
            net_buf_add(&req, head, (size_t)hn);
        }
        net_buf_add(&req, "\r\n", 2);
        if (reqbody) net_buf_add(&req, reqbody, strlen(reqbody));

        int saved = 0;
        if (!req.p || !net_sock_send_all(fd, req.p, req.n, &saved)) {
            if (!req.p) saved = ENOMEM;
            net_buf_free(&req);
            close(fd);
            net_fail(saved, "send request");
            return 0;
        }
        net_buf_free(&req);

        /* Connection: close, so the response ends at end of stream. */
        NetBuf raw = {0};
        char chunk[8192];
        for (;;) {
            ssize_t r = recv(fd, chunk, (int)sizeof chunk, 0);
            int e = net_errno();
            if (r == 0) break;
            if (r < 0) {
                if (e == NET_EINTR) continue;
                if (e == EAGAIN || e == NET_EWOULDBLOCK) e = NET_ETIMEDOUT;
                net_buf_free(&raw);
                close(fd);
                net_fail(e, "receive response");
                return 0;
            }
            if (!net_buf_add(&raw, chunk, (size_t)r)) {
                net_buf_free(&raw);
                close(fd);
                oe_error_set_errno(ENOMEM, "receive response");
                return 0;
            }
            if ((long)raw.n > NET_MAX_BODY) {
                net_buf_free(&raw);
                close(fd);
                oe_error_set(OE_ERR_UNSUPPORTED, "response larger than 64 MiB");
                return 0;
            }
        }
        close(fd);

        const char *sep = NULL;
        for (size_t i = 0; i + 3 < raw.n; i++)
            if (raw.p[i] == '\r' && raw.p[i+1] == '\n' &&
                raw.p[i+2] == '\r' && raw.p[i+3] == '\n') { sep = raw.p + i; break; }
        if (!sep || raw.n < 12 || strncmp(raw.p, "HTTP/", 5) != 0) {
            net_buf_free(&raw);
            oe_error_set(OE_ERR_INVALID_ARG, "not an http response");
            return 0;
        }

        /* Status line, then the header block that follows it. */
        const char *eol = NULL;
        for (const char *q = raw.p; q + 1 <= sep; q++)
            if (q[0] == '\r' && q[1] == '\n') { eol = q; break; }
        if (!eol) {
            net_buf_free(&raw);
            oe_error_set(OE_ERR_INVALID_ARG, "not an http response");
            return 0;
        }
        const char *sp = (const char *)memchr(raw.p, ' ', (size_t)(eol - raw.p));
        g_status = sp ? (int)strtol(sp + 1, NULL, 10) : 0;

        size_t hstart = (size_t)(eol - raw.p) + 2;
        size_t hlen = (size_t)(sep - raw.p) + 2 - hstart;
        free(g_headers);
        g_headers = (char *)malloc(hlen + 1);
        if (g_headers) { memcpy(g_headers, raw.p + hstart, hlen); g_headers[hlen] = '\0'; }

        const char *bstart = sep + 4;
        size_t blen = raw.n - (size_t)(bstart - raw.p);

        /* A redirect to https is the case this library must never paper over:
         * following it would need TLS, and rewriting it to http would put the
         * request on the wire in the clear. */
        char loc[2048];
        if ((g_status == 301 || g_status == 302 || g_status == 303 ||
             g_status == 307 || g_status == 308) &&
            net_header(g_headers, "Location", loc, sizeof loc) && *loc) {
            char next[2048];
            if (strncasecmp(loc, "https://", 8) == 0) {
                net_buf_free(&raw);
                oe_error_set(OE_ERR_UNSUPPORTED,
                             "the server redirected to https, which is not "
                             "supported yet: this build has no TLS, and the "
                             "request is never downgraded to http");
                return 0;
            }
            if (strstr(loc, "://")) snprintf(next, sizeof next, "%s", loc);
            else if (loc[0] == '/') {
                if (port == 80) snprintf(next, sizeof next, "http://%s%s", host, loc);
                else snprintf(next, sizeof next, "http://%s:%d%s", host, port, loc);
            } else {
                if (port == 80) snprintf(next, sizeof next, "http://%s/%s", host, loc);
                else snprintf(next, sizeof next, "http://%s:%d/%s", host, port, loc);
            }
            net_buf_free(&raw);
            snprintf(cur, sizeof cur, "%s", next);
            /* 303, and by long-standing practice 301/302, turn a POST into a
             * GET.  307 and 308 exist precisely to say "same method, same
             * body", so they keep both: quietly turning one into a GET would
             * be the same class of silent change as downgrading https. */
            if (g_status != 307 && g_status != 308) {
                method = "GET";
                reqbody = NULL;
            }
            continue;
        }

        char v[256];
        if (net_header(g_headers, "Content-Encoding", v, sizeof v) &&
            *v && strcasecmp(v, "identity") != 0) {
            net_buf_free(&raw);
            oe_error_set(OE_ERR_UNSUPPORTED, "compressed response bodies are "
                         "not supported: the server ignored Accept-Encoding: identity");
            return 0;
        }

        if (net_header(g_headers, "Transfer-Encoding", v, sizeof v) &&
            net_ci_contains(v, "chunked")) {
            if (!net_dechunk(bstart, blen, body)) {
                net_buf_free(body);
                net_buf_free(&raw);
                oe_error_set(OE_ERR_INVALID_ARG,
                             "malformed chunked response body");
                return 0;
            }
        } else {
            if (net_header(g_headers, "Content-Length", v, sizeof v)) {
                long cl = strtol(v, NULL, 10);
                if (cl >= 0 && (size_t)cl < blen) blen = (size_t)cl;
            }
            if (blen && !net_buf_add(body, bstart, blen)) {
                net_buf_free(&raw);
                oe_error_set_errno(ENOMEM, "read body");
                return 0;
            }
        }

        net_buf_free(&raw);
        oe_error_clear();
        return 1;
    }

    oe_error_set(OE_ERR_UNSUPPORTED, "too many redirects");
    return 0;
}

/* net_http_get(text url) -> text body ("" on failure) */
void net_http_get(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv) {
    (void)argc;
    NetBuf body = {0};
    if (!net_http_do("GET", net_nz(oe_arg_text(argv, 0)), NULL, NULL, &body)) {
        net_buf_free(&body);
        oe_ret_text(ret, net_empty());
        return;
    }
    char *out = net_text(body.p ? body.p : "", body.n);
    net_buf_free(&body);
    oe_ret_text(ret, out);                    /* net_http_do cleared the slot */
}

/* net_http_post(text url, text content_type, text body) -> text ("" on failure) */
void net_http_post(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv) {
    (void)argc;
    NetBuf body = {0};
    if (!net_http_do("POST", net_nz(oe_arg_text(argv, 0)),
                     net_nz(oe_arg_text(argv, 1)),
                     net_nz(oe_arg_text(argv, 2)), &body)) {
        net_buf_free(&body);
        oe_ret_text(ret, net_empty());
        return;
    }
    char *out = net_text(body.p ? body.p : "", body.n);
    net_buf_free(&body);
    oe_ret_text(ret, out);
}

/* net_http_status() -> int, the status of the last request (0 if none, or if
 * the request failed before a response arrived).  Infallible: reading it must
 * not disturb the error that explains the failure. */
void net_http_status(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv) {
    (void)argc; (void)argv;
    oe_ret_int(ret, g_status);
}

/* net_http_header(text name) -> text, from the last response; "" when absent.
 * Infallible for the same reason: an absent header is a genuine "no". */
void net_http_header(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv) {
    (void)argc;
    char v[1024];
    if (!net_header(g_headers, net_nz(oe_arg_text(argv, 0)), v, sizeof v)) {
        oe_ret_text(ret, net_empty());
        return;
    }
    oe_ret_text(ret, net_text(v, strlen(v)));
}

/* net_http_download(text url, text path) -> bool */
void net_http_download(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv) {
    (void)argc;
    const char *path = net_nz(oe_arg_text(argv, 1));
    if (!*path) {
        oe_error_set(OE_ERR_INVALID_ARG, "path is empty");
        oe_ret_bool(ret, 0);
        return;
    }

    NetBuf body = {0};
    if (!net_http_do("GET", net_nz(oe_arg_text(argv, 0)), NULL, NULL, &body)) {
        net_buf_free(&body);
        oe_ret_bool(ret, 0);
        return;
    }
    /* An error page is not the file that was asked for; writing it would leave
     * a program believing it had downloaded something. */
    if (g_status < 200 || g_status > 299) {
        char msg[128];
        snprintf(msg, sizeof msg, "http status %d", g_status);
        net_buf_free(&body);
        oe_error_set(OE_ERR_INVALID_ARG, msg);
        oe_ret_bool(ret, 0);
        return;
    }

    FILE *f = fopen(path, "wb");
    int e = errno;
    if (!f) {
        net_buf_free(&body);
        oe_error_set_errno(e, "open");
        oe_ret_bool(ret, 0);
        return;
    }
    size_t w = body.n ? fwrite(body.p, 1, body.n, f) : 0;
    int we = errno;
    int bad = (w != body.n);
    if (fclose(f) != 0 && !bad) { bad = 1; we = errno; }
    net_buf_free(&body);
    if (bad) {
        oe_error_set_errno(we, "write");
        oe_ret_bool(ret, 0);
        return;
    }
    oe_error_clear();
    oe_ret_bool(ret, 1);
}
