//! Wire-level tests for `openepl lsp`.
//!
//! These drive the real binary over real stdio with real JSON-RPC framing,
//! because that is where language servers actually break: a `Content-Length`
//! off by one, a handshake that never completes, a notification sent before
//! `initialized`. A unit test on `diagnose()` would pass while the server was
//! silent in every editor on earth — the same "mechanism works, surface is
//! broken" trap that has caught this project repeatedly.

use std::io::{BufRead, BufReader, Read, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

struct Client {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

impl Client {
    /// Start the server and complete the initialize handshake.
    fn start() -> Client {
        let mut child = Command::new(env!("CARGO_BIN_EXE_openepl"))
            .arg("lsp")
            .current_dir(repo())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn openepl lsp");
        let stdin = child.stdin.take().unwrap();
        let stdout = BufReader::new(child.stdout.take().unwrap());
        let mut c = Client {
            child,
            stdin,
            stdout,
        };

        let root = format!("file://{}", repo().display());
        c.send(serde_json::json!({
            "jsonrpc": "2.0", "id": 1, "method": "initialize",
            "params": { "capabilities": {}, "rootUri": root }
        }));
        let reply = c.recv();
        assert_eq!(reply["id"], 1, "initialize must be answered: {reply}");
        assert!(
            reply["result"]["capabilities"]["textDocumentSync"] == 1,
            "server should advertise full-text sync: {reply}"
        );
        c.send(serde_json::json!({
            "jsonrpc": "2.0", "method": "initialized", "params": {}
        }));
        c
    }

    fn send(&mut self, v: serde_json::Value) {
        let body = serde_json::to_string(&v).unwrap();
        write!(self.stdin, "Content-Length: {}\r\n\r\n{}", body.len(), body).unwrap();
        self.stdin.flush().unwrap();
    }

    /// Read one framed message. Panics on malformed framing, which is the point.
    fn recv(&mut self) -> serde_json::Value {
        let mut len = 0usize;
        loop {
            let mut line = String::new();
            let n = self.stdout.read_line(&mut line).expect("read header");
            assert!(n > 0, "server closed the stream while we were reading");
            let line = line.trim_end();
            if line.is_empty() {
                break;
            }
            if let Some(v) = line.strip_prefix("Content-Length: ") {
                len = v.parse().expect("numeric Content-Length");
            }
        }
        assert!(len > 0, "message had no Content-Length");
        let mut buf = vec![0u8; len];
        self.stdout.read_exact(&mut buf).expect("read body");
        serde_json::from_slice(&buf).expect("body is JSON")
    }

    fn open(&mut self, uri: &str, text: &str) {
        self.send(serde_json::json!({
            "jsonrpc": "2.0", "method": "textDocument/didOpen",
            "params": { "textDocument": {
                "uri": uri, "languageId": "openepl", "version": 1, "text": text
            }}
        }));
    }

    /// Read messages until a `publishDiagnostics` for `uri` arrives.
    fn diagnostics(&mut self, uri: &str) -> Vec<serde_json::Value> {
        for _ in 0..10 {
            let m = self.recv();
            if m["method"] == "textDocument/publishDiagnostics" && m["params"]["uri"] == uri {
                return m["params"]["diagnostics"].as_array().unwrap().clone();
            }
        }
        panic!("no publishDiagnostics for {uri}");
    }

    fn shutdown(mut self) {
        self.send(serde_json::json!({
            "jsonrpc": "2.0", "id": 99, "method": "shutdown", "params": null
        }));
        let _ = self.recv();
        self.send(serde_json::json!({"jsonrpc": "2.0", "method": "exit"}));
        // Close stdin BEFORE waiting. `lsp-server`'s reader thread runs until
        // stdin hits EOF, and the server joins its io threads on the way out —
        // so holding the pipe open here deadlocks the exit we are waiting for.
        drop(self.stdin);
        let status = self.child.wait().expect("server exits");
        assert!(status.success(), "server should exit cleanly: {status}");
    }
}

/// A clean file must produce an *empty* diagnostics array, not silence — that
/// is how a client clears previous squiggles.
#[test]
fn clean_file_publishes_empty_diagnostics() {
    let mut c = Client::start();
    let uri = "file:///tmp/openepl_lsp_clean.oir";
    c.open(
        uri,
        "module m\nsub main\n  call print_text(\"hi\")\nend\n",
    );
    let d = c.diagnostics(uri);
    assert!(d.is_empty(), "expected no diagnostics, got {d:?}");
    c.shutdown();
}

/// The whole point of the position work: the squiggle lands on the line with
/// the mistake. LSP lines are 0-based, so source line 5 is `line: 4`.
#[test]
fn semantic_errors_land_on_the_right_line() {
    let mut c = Client::start();
    let uri = "file:///tmp/openepl_lsp_errors.oir";
    //                1          2         3                 4          5
    c.open(
        uri,
        "module m\nsub main\n  let x: int = 1\n  x = 2\n  call nosuch()\nend\n",
    );
    let d = c.diagnostics(uri);
    assert_eq!(d.len(), 2, "expected two diagnostics, got {d:?}");

    let immutable = d
        .iter()
        .find(|x| x["message"].as_str().unwrap().contains("immutable"))
        .expect("immutability error");
    assert_eq!(immutable["range"]["start"]["line"], 3, "source line 4");
    assert_eq!(immutable["severity"], 1, "errors are severity 1");
    assert_eq!(immutable["source"], "openepl");
    assert!(
        !immutable["message"].as_str().unwrap().starts_with("line "),
        "the message must not repeat the position — the range carries it: {immutable}"
    );

    let unknown = d
        .iter()
        .find(|x| x["message"].as_str().unwrap().contains("unknown command"))
        .expect("unknown-command error");
    assert_eq!(unknown["range"]["start"]["line"], 4, "source line 5");
    c.shutdown();
}

/// While you type, the file is syntactically broken most of the time. That path
/// must be positioned too, not collapsed to line 0.
#[test]
fn parse_errors_are_positioned() {
    let mut c = Client::start();
    let uri = "file:///tmp/openepl_lsp_parse.oir";
    c.open(uri, "module m\nsub main\n  let = = =\nend\n");
    let d = c.diagnostics(uri);
    assert_eq!(d.len(), 1, "one parse error: {d:?}");
    assert_eq!(d[0]["range"]["start"]["line"], 2, "the bad line is line 3");
    c.shutdown();
}

/// Editing republishes: fix the file and the squiggle must disappear.
#[test]
fn edits_republish_diagnostics() {
    let mut c = Client::start();
    let uri = "file:///tmp/openepl_lsp_edit.oir";
    c.open(uri, "module m\nsub main\n  call nosuch()\nend\n");
    assert_eq!(c.diagnostics(uri).len(), 1, "starts broken");

    c.send(serde_json::json!({
        "jsonrpc": "2.0", "method": "textDocument/didChange",
        "params": {
            "textDocument": { "uri": uri, "version": 2 },
            "contentChanges": [{ "text": "module m\nsub main\n  call print_text(\"ok\")\nend\n" }]
        }
    }));
    assert!(
        c.diagnostics(uri).is_empty(),
        "fixing the file must clear the diagnostics"
    );
    c.shutdown();
}

/// An unsupported request must get an error response, never silence: a client
/// waiting on a reply that never comes hangs.
#[test]
fn unsupported_requests_get_an_error_not_silence() {
    let mut c = Client::start();
    c.send(serde_json::json!({
        "jsonrpc": "2.0", "id": 7, "method": "textDocument/hover",
        "params": { "textDocument": {"uri": "file:///tmp/x.oir"},
                    "position": {"line": 0, "character": 0} }
    }));
    let reply = c.recv();
    assert_eq!(reply["id"], 7);
    assert!(reply["error"].is_object(), "expected an error reply: {reply}");
    c.shutdown();
}
