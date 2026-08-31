// A Language Server Protocol client for Studio's code editor.
//
// Studio speaks to the same `openepl lsp` that VS Code and Neovim use, rather
// than growing a second, private analysis path. Whatever the server learns to
// do, every editor gets — including this one.
//
// The transport is the fork + non-blocking pipe pattern used for the build and
// for running programs: the frame loop drains it, so a slow or silent server
// can never stall the UI.
#ifndef OPENEPL_DESIGNER_LSPCLIENT_H
#define OPENEPL_DESIGNER_LSPCLIENT_H

#include <fcntl.h>
#include <signal.h>
#include <string>
#include <sys/wait.h>
#include <unistd.h>
#include <vector>

#include "json.h"

namespace openepl::lsp {

/// One diagnostic, flattened to what the editor draws.
struct Diagnostic {
    int line = 0;        ///< 0-based, as the protocol reports it
    int severity = 1;    ///< 1 = error, 2 = warning
    std::string message;
};

class Client {
public:
    ~Client() { stop(); }

    bool running() const { return pid_ > 0; }

    /// Whether diagnostics have arrived since the last `take_diagnostics`.
    bool has_update() const { return updated_; }

    const std::vector<Diagnostic>& diagnostics() const { return diagnostics_; }

    void clear_update() { updated_ = false; }

    /// Launch `<openepl_bin> lsp` and complete the handshake.
    bool start(const std::string& openepl_bin, const std::string& root_dir) {
        int to_child[2], from_child[2];
        if (::pipe(to_child) != 0) return false;
        if (::pipe(from_child) != 0) {
            ::close(to_child[0]);
            ::close(to_child[1]);
            return false;
        }
        const pid_t pid = ::fork();
        if (pid == 0) {
            ::dup2(to_child[0], STDIN_FILENO);
            ::dup2(from_child[1], STDOUT_FILENO);
            ::close(to_child[0]);
            ::close(to_child[1]);
            ::close(from_child[0]);
            ::close(from_child[1]);
            // The server's own logging goes to stderr; let it reach the
            // terminal rather than mixing into the protocol stream.
            ::execlp(openepl_bin.c_str(), openepl_bin.c_str(), "lsp", (char*)nullptr);
            _exit(127);
        }
        ::close(to_child[0]);
        ::close(from_child[1]);
        if (pid < 0) {
            ::close(to_child[1]);
            ::close(from_child[0]);
            return false;
        }
        in_ = to_child[1];
        out_ = from_child[0];
        ::fcntl(out_, F_SETFL, O_NONBLOCK);
        pid_ = pid;

        send("{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{"
             "\"processId\":null,\"capabilities\":{},\"rootUri\":\"file://" +
             json::escape(root_dir) + "\"}}");
        send("{\"jsonrpc\":\"2.0\",\"method\":\"initialized\",\"params\":{}}");
        return true;
    }

    void did_open(const std::string& path, const std::string& text) {
        if (!running()) return;
        uri_ = "file://" + path;
        send("{\"jsonrpc\":\"2.0\",\"method\":\"textDocument/didOpen\",\"params\":{"
             "\"textDocument\":{\"uri\":\"" + json::escape(uri_) +
             "\",\"languageId\":\"openepl\",\"version\":1,\"text\":\"" + json::escape(text) +
             "\"}}}");
    }

    /// Full-text sync, matching what the server advertises.
    void did_change(const std::string& text) {
        if (!running() || uri_.empty()) return;
        send("{\"jsonrpc\":\"2.0\",\"method\":\"textDocument/didChange\",\"params\":{"
             "\"textDocument\":{\"uri\":\"" + json::escape(uri_) + "\",\"version\":" +
             std::to_string(++version_) + "},\"contentChanges\":[{\"text\":\"" +
             json::escape(text) + "\"}]}}");
    }

    /// Read whatever the server has sent. Call once per frame; never blocks.
    void poll() {
        if (out_ < 0) return;
        char buf[4096];
        ssize_t n;
        while ((n = ::read(out_, buf, sizeof buf)) > 0) inbuf_.append(buf, (size_t)n);
        if (n == 0) {                    // the server closed its end
            ::close(out_);
            out_ = -1;
        }

        // Frames are `Content-Length: N\r\n\r\n<N bytes>`; anything short is
        // left in the buffer for the next poll rather than mis-parsed.
        for (;;) {
            const size_t head = inbuf_.find("\r\n\r\n");
            if (head == std::string::npos) return;
            const size_t cl = inbuf_.find("Content-Length:");
            if (cl == std::string::npos || cl > head) {
                inbuf_.erase(0, head + 4);
                continue;
            }
            const size_t len = (size_t)std::atoi(inbuf_.c_str() + cl + 15);
            const size_t body = head + 4;
            if (inbuf_.size() < body + len) return;   // wait for the rest
            handle(inbuf_.substr(body, len));
            inbuf_.erase(0, body + len);
        }
    }

    /// Shut the server down the way the protocol says, so it exits cleanly.
    void stop() {
        if (pid_ <= 0) return;
        send("{\"jsonrpc\":\"2.0\",\"id\":99,\"method\":\"shutdown\",\"params\":null}");
        send("{\"jsonrpc\":\"2.0\",\"method\":\"exit\"}");
        if (in_ >= 0) { ::close(in_); in_ = -1; }
        // Closing stdin is what lets its reader thread finish; without that the
        // server would sit waiting and we would wait for it.
        int status = 0;
        for (int i = 0; i < 100 && ::waitpid(pid_, &status, WNOHANG) == 0; i++) usleep(10000);
        if (::waitpid(pid_, &status, WNOHANG) == 0) {
            ::kill(pid_, SIGTERM);
            ::waitpid(pid_, &status, 0);
        }
        if (out_ >= 0) { ::close(out_); out_ = -1; }
        pid_ = 0;
    }

private:
    void send(const std::string& body) {
        if (in_ < 0) return;
        const std::string frame =
            "Content-Length: " + std::to_string(body.size()) + "\r\n\r\n" + body;
        // A short write would corrupt the frame, so keep going until it is all
        // out. EPIPE means the server died; drop the pipe rather than loop.
        size_t sent = 0;
        while (sent < frame.size()) {
            const ssize_t w = ::write(in_, frame.data() + sent, frame.size() - sent);
            if (w <= 0) {
                ::close(in_);
                in_ = -1;
                return;
            }
            sent += (size_t)w;
        }
    }

    void handle(const std::string& body) {
        const json::Value msg = json::parse(body);
        if (msg["method"].str() != "textDocument/publishDiagnostics") return;
        const json::Value& params = msg["params"];
        // Only the document we opened. The server may publish for others.
        if (!uri_.empty() && params["uri"].str() != uri_) return;

        diagnostics_.clear();
        const json::Value& list = params["diagnostics"];
        for (size_t i = 0; i < list.size(); i++) {
            const json::Value& d = list.at(i);
            Diagnostic out;
            out.line = d["range"]["start"]["line"].num(0);
            out.severity = d["severity"].num(1);
            out.message = d["message"].str();
            diagnostics_.push_back(std::move(out));
        }
        updated_ = true;
    }

    pid_t pid_ = 0;
    int in_ = -1;
    int out_ = -1;
    int version_ = 1;
    bool updated_ = false;
    std::string inbuf_;
    std::string uri_;
    std::vector<Diagnostic> diagnostics_;
};

} // namespace openepl::lsp

#endif
