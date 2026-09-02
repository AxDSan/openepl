/* "net" library metadata (design-time only; compiled into the introspection
 * .so, never a shipped program — the same split as core_libinfo.c).
 *
 * Every command is named net_*: one flat command namespace is shared with core
 * and every other library, and this library owns that prefix.
 *
 * The command list does not move with the build.  https is an optional
 * capability (mbedTLS, see lib.json), and a capability may not add or remove a
 * command: a program that compiles here must compile on a machine that vendored
 * something different.  So net_http_get is one command that speaks both
 * schemes, and refuses https at run time when this build has no TLS — see the
 * header comment in net_cmds.c.
 *
 * The server side is one non-visual component plus the commands that read a
 * request and answer it.  They are named net_req_* rather than http_* because
 * one flat command namespace is shared with core and every other library, this
 * library owns the net_ prefix, and net_http_header already means something
 * else here — the header of the last response a CLIENT received.
 */
#include "openepl_abi.h"

void net_tcp_connect(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv);
void net_tcp_send(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv);
void net_tcp_receive(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv);
void net_tcp_receive_line(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv);
void net_tcp_at_end(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv);
void net_tcp_close(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv);
void net_tcp_close_all(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv);
void net_timeout_set(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv);
void net_timeout_get(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv);
void net_url_encode(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv);
void net_url_decode(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv);
void net_host_ip(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv);
void net_http_get(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv);
void net_http_post(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv);
void net_http_status(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv);
void net_http_header(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv);
void net_http_download(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv);
void net_request(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv);
void net_req_method(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv);
void net_req_path(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv);
void net_req_body(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv);
void net_req_header(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv);
void net_req_query(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv);
void net_req_reply(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv);
void net_req_reply_as(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv);
void tcpserver_send(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv);
void tcpserver_send_all(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv);
void tcpserver_disconnect(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv);
void tcpserver_client_count(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv);
void tcpserver_client_address(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv);
void tcpserver_client(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv);
void tcpclient_send(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv);
void tcpclient_connect(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv);
void tcpclient_disconnect(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv);
void tcpclient_connected(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv);

static const int32_t P_I[]   = { OE_SDT_INT };
static const int32_t P_T[]   = { OE_SDT_TEXT };
static const int32_t P_IIT[] = { OE_SDT_INT, OE_SDT_INT, OE_SDT_TEXT };
static const int32_t P_IITT[]= { OE_SDT_INT, OE_SDT_INT, OE_SDT_TEXT, OE_SDT_TEXT };
static const int32_t P_TI[]  = { OE_SDT_TEXT, OE_SDT_INT };
static const int32_t P_IT[]  = { OE_SDT_INT,  OE_SDT_TEXT };
static const int32_t P_II[]  = { OE_SDT_INT,  OE_SDT_INT };
static const int32_t P_TT[]  = { OE_SDT_TEXT, OE_SDT_TEXT };
static const int32_t P_TTT[] = { OE_SDT_TEXT, OE_SDT_TEXT, OE_SDT_TEXT };
static const int32_t P_TIT[] = { OE_SDT_TEXT, OE_SDT_INT,  OE_SDT_TEXT };

static const OpenEPL_CommandDesc NET_COMMANDS[] = {
    /* --- TCP ---------------------------------------------------------- */
    { "net_tcp_connect",      "net_tcp_connect",      OE_SDT_INT,  2, P_TI  },
    { "net_tcp_send",         "net_tcp_send",         OE_SDT_BOOL, 2, P_IT  },
    { "net_tcp_receive",      "net_tcp_receive",      OE_SDT_TEXT, 2, P_II  },
    { "net_tcp_receive_line", "net_tcp_receive_line", OE_SDT_TEXT, 1, P_I   },
    { "net_tcp_at_end",       "net_tcp_at_end",       OE_SDT_BOOL, 1, P_I   },
    { "net_tcp_close",        "net_tcp_close",        OE_SDT_BOOL, 1, P_I   },
    { "net_tcp_close_all",    "net_tcp_close_all",    OE_SDT_INT,  0, 0     },
    /* --- timeouts ----------------------------------------------------- */
    { "net_timeout_set",      "net_timeout_set",      OE_SDT_BOOL, 1, P_I   },
    { "net_timeout_get",      "net_timeout_get",      OE_SDT_INT,  0, 0     },
    /* --- URLs and names ----------------------------------------------- */
    { "net_url_encode",       "net_url_encode",       OE_SDT_TEXT, 1, P_T   },
    { "net_url_decode",       "net_url_decode",       OE_SDT_TEXT, 1, P_T   },
    { "net_host_ip",          "net_host_ip",          OE_SDT_TEXT, 1, P_T   },
    /* --- HTTP (http, and https when TLS was vendored) ------------------ */
    { "net_http_get",         "net_http_get",         OE_SDT_TEXT, 1, P_T   },
    { "net_http_post",        "net_http_post",        OE_SDT_TEXT, 3, P_TTT },
    { "net_http_status",      "net_http_status",      OE_SDT_INT,  0, 0     },
    { "net_http_header",      "net_http_header",      OE_SDT_TEXT, 1, P_T   },
    { "net_http_download",    "net_http_download",    OE_SDT_BOOL, 2, P_TT  },
    /* --- serving: the request being handled right now ------------------ */
    { "net_request",          "net_request",          OE_SDT_INT,  0, 0     },
    { "net_req_method",       "net_req_method",       OE_SDT_TEXT, 1, P_I   },
    { "net_req_path",         "net_req_path",         OE_SDT_TEXT, 1, P_I   },
    { "net_req_body",         "net_req_body",         OE_SDT_TEXT, 1, P_I   },
    { "net_req_header",       "net_req_header",       OE_SDT_TEXT, 2, P_IT  },
    { "net_req_query",        "net_req_query",        OE_SDT_TEXT, 2, P_IT  },
    { "net_req_reply",        "net_req_reply",        OE_SDT_BOOL, 3, P_IIT },
    { "net_req_reply_as",     "net_req_reply_as",     OE_SDT_BOOL, 4, P_IITT},
    /* --- the tcpserver and tcpclient components ------------------------
     * Named after the component rather than net_, the way `grid_` and
     * `datasource_` are: the first argument is the component's `name`, and a
     * command that starts with the type it addresses reads as a method on
     * it.  The prefixes are this library's; see libs/README.md. */
    { "tcpserver_send",           "tcpserver_send",           OE_SDT_BOOL, 3, P_TIT },
    { "tcpserver_send_all",       "tcpserver_send_all",       OE_SDT_INT,  2, P_TT  },
    { "tcpserver_disconnect",     "tcpserver_disconnect",     OE_SDT_BOOL, 2, P_TI  },
    { "tcpserver_client_count",   "tcpserver_client_count",   OE_SDT_INT,  1, P_T   },
    { "tcpserver_client_address", "tcpserver_client_address", OE_SDT_TEXT, 2, P_TI  },
    { "tcpserver_client",         "tcpserver_client",         OE_SDT_INT,  2, P_TI  },
    { "tcpclient_send",           "tcpclient_send",           OE_SDT_BOOL, 2, P_TT  },
    { "tcpclient_connect",        "tcpclient_connect",        OE_SDT_BOOL, 1, P_T   },
    { "tcpclient_disconnect",     "tcpclient_disconnect",     OE_SDT_BOOL, 1, P_T   },
    { "tcpclient_connected",      "tcpclient_connected",      OE_SDT_BOOL, 1, P_T   },
};

/* --- httpserver: net's non-visual component ----------------------------
 * Two properties and one event is the whole surface: someone drops a server on
 * a form, sets a port, wires `request`, and has a web service.  `bind` is
 * loopback by default and has to be changed on purpose — the default is the
 * documentation for everyone who does not read any. */
static const OpenEPL_PropertyDesc HTTPD_PROPS[] = {
    { "port", OE_SDT_INT,  "8080",      NULL },
    { "bind", OE_SDT_TEXT, "127.0.0.1", NULL },
};
/* Zero-filled explicitly: `request` hands its handler nothing, and saying so
 * costs a reader less than a -Wextra warning about the v3 fields does. */
static const OpenEPL_EventDesc HTTPD_EVENTS[] = { { "request", 0, NULL } };

/* --- tcpserver / tcpclient: plain TCP, the Indy shape ----------------------
 * `name` is how a command finds the component (`tcpserver_send("chat", ...)`)
 * — the compiler hands a library no id, and this is the same answer `grid`
 * gives.  `active` is the switch: nothing listens or connects until it is
 * true, and false closes everything.  `address` is every interface, unlike
 * httpserver's `bind`: a chat server that only its own machine could reach
 * is the surprising default here, and it does nothing until asked anyway.
 *
 * The events are typed.  A server event names the client it is about, as a
 * small int the commands take back; `receive` adds the line, and `error` the
 * message.  A client's `receive` hands the line alone: there is one peer. */
static const OpenEPL_PropertyDesc TCPSERVER_PROPS[] = {
    { "name",        OE_SDT_TEXT, "",        NULL },
    { "port",        OE_SDT_INT,  "0",       NULL },
    { "address",     OE_SDT_TEXT, "0.0.0.0", NULL },
    { "active",      OE_SDT_BOOL, "false",   NULL },
    { "max_clients", OE_SDT_INT,  "64",      NULL },
    { "delimiter",   OE_SDT_TEXT, "\n",     NULL },
};
static const int32_t EV_I[]  = { OE_SDT_INT };
static const int32_t EV_IT[] = { OE_SDT_INT, OE_SDT_TEXT };
static const int32_t EV_T[]  = { OE_SDT_TEXT };
static const OpenEPL_EventDesc TCPSERVER_EVENTS[] = {
    { "connect",    1, EV_I  },
    { "disconnect", 1, EV_I  },
    { "receive",    2, EV_IT },
    { "error",      1, EV_T  },
};

static const OpenEPL_PropertyDesc TCPCLIENT_PROPS[] = {
    { "name",       OE_SDT_TEXT, "",      NULL },
    { "host",       OE_SDT_TEXT, "",      NULL },
    { "port",       OE_SDT_INT,  "0",     NULL },
    { "active",     OE_SDT_BOOL, "false", NULL },
    /* Read-only: setting it is refused at run time with the error slot set. */
    { "connected",  OE_SDT_BOOL, "false", NULL },
    { "delimiter",  OE_SDT_TEXT, "\n",   NULL },
    { "timeout_ms", OE_SDT_INT,  "5000",  NULL },
};
static const OpenEPL_EventDesc TCPCLIENT_EVENTS[] = {
    { "connect",    0, NULL },
    { "disconnect", 0, NULL },
    { "receive",    1, EV_T },
    { "error",      1, EV_T },
};

#define NET_COUNT(a) ((int32_t)(sizeof(a) / sizeof((a)[0])))

static const OpenEPL_ComponentDesc NET_COMPONENTS[] = {
    { "httpserver", OE_ROLE_UNKNOWN,
      NET_COUNT(HTTPD_PROPS), HTTPD_PROPS, NET_COUNT(HTTPD_EVENTS), HTTPD_EVENTS,
      OE_COMPONENT_NONVISUAL },
    { "tcpserver", OE_ROLE_UNKNOWN,
      NET_COUNT(TCPSERVER_PROPS), TCPSERVER_PROPS, NET_COUNT(TCPSERVER_EVENTS), TCPSERVER_EVENTS,
      OE_COMPONENT_NONVISUAL },
    { "tcpclient", OE_ROLE_UNKNOWN,
      NET_COUNT(TCPCLIENT_PROPS), TCPCLIENT_PROPS, NET_COUNT(TCPCLIENT_EVENTS), TCPCLIENT_EVENTS,
      OE_COMPONENT_NONVISUAL },
};

static const OpenEPL_LibInfo NET_INFO = {
    OPENEPL_ABI_VERSION,
    "net",
    "openepl-net-0000-0000-0000-6e6574000001",
    0, 1, 0,
    (int32_t)(sizeof(NET_COMMANDS) / sizeof(NET_COMMANDS[0])),
    NET_COMMANDS,
    (int32_t)(sizeof(NET_COMPONENTS) / sizeof(NET_COMPONENTS[0])),
    NET_COMPONENTS,
};

const OpenEPL_LibInfo *openepl_get_lib_info(void) {
    return &NET_INFO;
}
