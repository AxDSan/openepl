# Networking

`use net` gives a program TCP, an HTTP client, and an HTTP server. The server
is a **component**: you drop it on a form or declare it at module level, set a
port, wire one event, and you have a web service. There is no socket in the
program text anywhere.

```
module hello_web
use net

httpserver site
  port = 8080
  on request: on_request
end

sub on_request
  call route(net_request())
end

sub route(req: int)
  call net_req_reply(req, 200, concat("you asked for ", net_req_path(req)))
end

sub main
  call print_text("serving on http://127.0.0.1:8080")
end
```

`main` prints one line and returns, and the program keeps running. A server is
a live event source, exactly like a timer, and the runtime's loop stays in it
until something calls `quit`. `examples/webserver.oir` starts you here.

## The request is a parameter, not a place

`request` is one of the events that hands its handler nothing — deliberately,
so the request is fetched by the code that is about to use it — and the
handler asks for the one being dispatched and hands it on:

```
sub on_request
  call route(net_request())
end

sub route(req: int)
  ...
end
```

That indirection is the point. A request kept in a module variable or a
component property would answer the first caller correctly and then quietly
serve the second caller the first one's path, headers and body — a bug that
passes every test written with one browser tab open.

`req` is a handle, and it is retired the moment the handler returns. A program
that saves one and uses it on the next request is told the handle is stale
(`last_error_code()` is `10002`) instead of being handed someone else's
connection.

`net_request()` outside a handler is a failure, not an empty answer: it returns
0 and sets error `10005`.

## Reading a request

| Command | Answers |
|---|---|
| `net_req_method(req)` | `GET`, `POST`, … |
| `net_req_path(req)` | the path, with the query string removed |
| `net_req_query(req, name)` | one query parameter, percent-decoded |
| `net_req_header(req, name)` | one header, matched case-insensitively |
| `net_req_body(req)` | the request body |

`net_req_query` and `net_req_header` answer `""` for something the client did
not send. That is a genuine "no", not a failure: `last_error_code()` stays 0,
which is the whole reason an empty answer is readable here.

## Answering

```
call net_req_reply(req, 200, "hello")
call net_req_reply_as(req, 200, "application/json", "{\"ok\":true}")
```

`net_req_reply` sends `text/plain; charset=utf-8`; `net_req_reply_as` takes the
content type. Both answer once — a second reply on the same request fails
rather than sending two.

A handler that replies to nothing still answers: **200 with an empty body**. A
route you forgot is then a blank page, not a browser spinning until it gives
up.

## The defaults, and why they are what they are

- **`bind` is `127.0.0.1`.** The server is reachable from this machine only
  until someone writes `bind = "0.0.0.0"` on purpose. A RAD tool whose default
  put a half-written service on every interface would be shipping the mistake.
- **A request body is capped at 1 MiB**, and a request head at 64 KiB. Past
  either, the client gets `413` or `431` and the handler never runs — handing a
  program half a request would be the wrong kind of honesty.
- **A chunked request is refused with `501`.** A body sent with
  `Transfer-Encoding: chunked` has no length to check, and dispatching it
  would hand the handler an empty body with no error — the one failure shape
  nothing here is allowed to have.
- **A connection that goes quiet for 15 seconds is dropped**, so a client that
  connects and says nothing cannot hold a slot.
- **16 connections at a time, per server.** The 17th is refused rather than
  queued: an unbounded backlog of half-read requests is how a small server
  becomes a memory leak with a port number.
- **Every response says `Connection: close`.** There is no keep-alive, because
  a server that promises to reuse a connection and then does not is worse than
  one that never promised.

## Nothing blocks

The server registers one pump with the runtime's event loop and does a slice of
work per turn: accept what is waiting, read what has arrived, dispatch what is
complete, write what it can. No thread is started and no call waits. A form
with a server on it keeps repainting while it serves — that is what the loop is
for.

The consequence worth knowing: **a handler runs on the same turn as the rest of
the program.** A handler that sleeps or does a slow HTTP call of its own stops
everything else, including the window. Keep handlers short.

Calling `quit()` from inside a handler works — the reply that handler already
set is flushed before the loop stops — but any *other* connection mid-response
is dropped with it.

## The HTTP client

```
module fetch
use net

sub main
  let body: text = net_http_get("http://example.com/")
  call print_int(net_http_status())
  call print_text(net_http_header("content-type"))
end
```

`net_http_post(url, content_type, body)` is the same shape. `net_timeout_set`
bounds every connect, send and receive (10 seconds by default), because a
program that hangs forever on a dead host looks like a working program that is
merely slow. `net_http_download(url, path)` writes bytes to a file, which is
what binary content needs — text is NUL-terminated and would stop at the first
zero byte.

## https is optional, and never assumed

TLS is a dependency you opt into. OpenEPL links a program's libraries in, so a
TLS stack is vendored into every binary that uses one — megabytes of code and a
security-critical dependency to patch on someone else's schedule. Nobody who
only speaks `http://` should pay that, and nobody who wants `https://` should
have to talk the build system into it. So:

```sh
tools/fetch-mbedtls.sh     # once; everything else builds without it
```

With mbedTLS vendored, `net_http_get("https://...")` works and the default port
becomes 443. Without it, the same call **fails** with `OE_ERR_UNSUPPORTED`
(`10006`) and a message naming the script. Every other command, and every other
library, builds and behaves identically either way.

### What never happens

**No downgrade.** An `https://` URL is never rewritten to `http://` — not when
TLS is missing, and not when a server answers a redirect with an `http://`
`Location:`. That redirect is refused with `OE_ERR_UNSUPPORTED`. A silent
downgrade would put a password on the wire in the clear and the program that
"worked" would be the vulnerability.

**No unverified certificate.** The certificate chain is verified against the
machine's trust store and the hostname is checked against the certificate, with
no option to turn either off. An https client that skips verification is
encrypted to whoever is on the path: it offers the appearance of security and
none of it, which is worse than the honest refusal it replaced, because the
refusal is visible and this is not. A certificate that does not verify fails the
request and says why — expired, wrong name, unknown issuer.

The store is found at one of the usual locations
(`/etc/ssl/certs/ca-certificates.crt`, `/etc/pki/tls/certs/ca-bundle.crt`, and
the rest). `OPENEPL_CA_BUNDLE` overrides it with a file or a directory, which is
what a container, a corporate proxy or a test with its own certificate needs. If
no store can be found the request fails with `OE_ERR_UNSUPPORTED` rather than
falling back to trusting everything — that fallback is the same hole arriving
through a different door.

### The server side is still plaintext

`httpserver` does not terminate TLS. Put a reverse proxy in front of it if it
needs to be reachable over https.

## Memory, over a long run

Every text a command returns is owned by the runtime and released when the
program exits, not when the value goes out of scope. That is fine for a program
that runs and stops, and it is worth knowing for a server that runs for weeks:
its memory grows with the number of requests it has answered. Restart it, or
put it behind something that will.
