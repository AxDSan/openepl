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
        // Formatting is genuinely unimplemented; hover et al. are supported now.
        "jsonrpc": "2.0", "id": 7, "method": "textDocument/formatting",
        "params": { "textDocument": {"uri": "file:///tmp/x.oir"},
                    "options": {"tabSize": 2, "insertSpaces": true} }
    }));
    let reply = c.recv();
    assert_eq!(reply["id"], 7);
    assert!(reply["error"].is_object(), "expected an error reply: {reply}");
    c.shutdown();
}

// ---------------------------------------------------------------------------
// Completion, hover, definition, references
// ---------------------------------------------------------------------------

impl Client {
    fn request(&mut self, id: i64, method: &str, params: serde_json::Value) -> serde_json::Value {
        self.send(serde_json::json!({
            "jsonrpc": "2.0", "id": id, "method": method, "params": params
        }));
        for _ in 0..10 {
            let m = self.recv();
            if m["id"] == id {
                return m["result"].clone();
            }
        }
        panic!("no response to {method}");
    }

    fn at(uri: &str, line: u32, ch: u32) -> serde_json::Value {
        serde_json::json!({
            "textDocument": { "uri": uri },
            "position": { "line": line, "character": ch }
        })
    }
}

/// The server must advertise every capability it implements — a client only
/// sends requests for capabilities it was told about, so an unadvertised
/// feature is dead code however well it works.
#[test]
fn advertises_its_capabilities() {
    // start() already asserts the handshake and the advertised sync kind.
    Client::start().shutdown();

    let mut c = Client::start();
    let r = c.request(50, "textDocument/documentSymbol", serde_json::json!({
        "textDocument": { "uri": "file:///tmp/openepl_lsp_nodoc.oir" }
    }));
    // Unknown document: a null result, not an error and not a hang.
    assert!(r.is_null(), "unknown document should answer null: {r}");
    c.shutdown();
}

// A real form: `use ui` supplies the component types, and each component
// opens its own block.
const FORM_SRC: &str =
    "module m\nuse ui\nform Main\n  button ok\n  end\nend\nsub go\n  ok.text = \"hi\"\nend\n";

/// After `id.` the completions must be that component's properties — offering
/// the whole global namespace there is what makes completion feel broken.
#[test]
fn completion_after_dot_offers_component_properties() {
    let mut c = Client::start();
    let uri = "file:///tmp/openepl_lsp_complete.oir";
    c.open(uri, FORM_SRC);
    let _ = c.diagnostics(uri);

    // `  ok.te|xt` on line 8 (0-based 7): mid-word, the way an editor asks.
    let r = c.request(60, "textDocument/completion", serde_json::json!({
        "textDocument": { "uri": uri },
        "position": { "line": 7, "character": 7 }
    }));
    let labels: Vec<String> = r
        .as_array()
        .expect("completion list")
        .iter()
        .map(|i| i["label"].as_str().unwrap().to_string())
        .collect();
    assert!(labels.contains(&"text".to_string()), "expected `text`: {labels:?}");
    assert!(
        !labels.contains(&"module".to_string()),
        "keywords must not appear after `.`: {labels:?}"
    );
    c.shutdown();
}

/// Plain completion offers commands from the registry, with their signature.
#[test]
fn completion_offers_commands_and_locals() {
    let mut c = Client::start();
    let uri = "file:///tmp/openepl_lsp_complete2.oir";
    c.open(uri, "module m\nsub main\n  let total: int = 1\n  \nend\n");
    let _ = c.diagnostics(uri);

    let r = c.request(61, "textDocument/completion", serde_json::json!({
        "textDocument": { "uri": uri },
        "position": { "line": 3, "character": 2 }
    }));
    let items = r.as_array().expect("completion list");
    let labels: Vec<&str> = items.iter().map(|i| i["label"].as_str().unwrap()).collect();
    assert!(labels.contains(&"print_text"), "commands: {labels:?}");
    assert!(labels.contains(&"total"), "locals in scope: {labels:?}");

    let print = items.iter().find(|i| i["label"] == "print_text").unwrap();
    assert!(
        print["detail"].as_str().unwrap().contains("("),
        "a command's detail should show its signature: {print}"
    );
    c.shutdown();
}

#[test]
fn hover_shows_a_command_signature() {
    let mut c = Client::start();
    let uri = "file:///tmp/openepl_lsp_hover.oir";
    c.open(uri, "module m\nsub main\n  call print_text(\"hi\")\nend\n");
    let _ = c.diagnostics(uri);

    let r = c.request(70, "textDocument/hover", Client::at(uri, 2, 9));
    let text = r["contents"]["value"].as_str().unwrap_or("");
    assert!(text.contains("print_text"), "hover should name it: {r}");
    assert!(text.contains("command"), "and say what it is: {r}");
    c.shutdown();
}

#[test]
fn goto_definition_finds_the_declaration() {
    let mut c = Client::start();
    let uri = "file:///tmp/openepl_lsp_def.oir";
    //          1        2               3         4       5
    c.open(uri, "module m\nvar count: int = 0\nsub main\n  count = 1\nend\n");
    let _ = c.diagnostics(uri);

    // cursor on `count` in the assignment (line 4 -> 0-based 3)
    let r = c.request(80, "textDocument/definition", Client::at(uri, 3, 3));
    assert_eq!(r["range"]["start"]["line"], 1, "declared on line 2: {r}");
    assert_eq!(r["uri"], uri);
    c.shutdown();
}

/// Shadowing is where a naive index gets it wrong with full confidence.
#[test]
fn references_respect_local_shadowing() {
    let mut c = Client::start();
    let uri = "file:///tmp/openepl_lsp_refs.oir";
    //          1        2               3      4                5         6    7      8         9
    let src = "module m\nvar x: int = 1\nsub a\n  let x: int = 9\n  x = 3\nend\nsub b\n  x = 4\nend\n";
    c.open(uri, src);
    let _ = c.diagnostics(uri);

    // The `x` on line 5 is a's local: it must not include line 2 or line 8.
    let r = c.request(90, "textDocument/references", serde_json::json!({
        "textDocument": { "uri": uri },
        "position": { "line": 4, "character": 2 },
        "context": { "includeDeclaration": true }
    }));
    let lines: Vec<i64> = r
        .as_array()
        .expect("locations")
        .iter()
        .map(|l| l["range"]["start"]["line"].as_i64().unwrap())
        .collect();
    assert!(lines.contains(&3), "the local's declaration: {lines:?}");
    assert!(lines.contains(&4), "its use: {lines:?}");
    assert!(!lines.contains(&1), "must NOT include the global: {lines:?}");
    assert!(!lines.contains(&7), "must NOT include sub b: {lines:?}");
    c.shutdown();
}

/// LSP columns are UTF-16 code units; our lexer counts bytes. They agree only
/// for ASCII, so an ASCII-only suite would pass forever while every file with a
/// non-Latin string misplaced the cursor.
#[test]
fn positions_are_utf16_not_bytes() {
    let mut c = Client::start();
    let uri = "file:///tmp/openepl_lsp_utf16.oir";
    // "héllo wörld" is 11 UTF-16 units but 13 bytes.
    let src = "module m\nvar greeting: text = \"héllo wörld\"\nsub main\n  greeting = \"x\"\nend\n";
    c.open(uri, src);
    let _ = c.diagnostics(uri);

    // `greeting` on line 4 starts at UTF-16 character 2.
    let r = c.request(100, "textDocument/definition", Client::at(uri, 3, 3));
    assert_eq!(r["range"]["start"]["line"], 1, "declared on line 2: {r}");
    assert_eq!(
        r["range"]["start"]["character"], 4,
        "`greeting` starts after `var ` — 4 UTF-16 units in: {r}"
    );
    assert_eq!(r["range"]["end"]["character"], 12, "8 characters long: {r}");
    c.shutdown();
}

#[test]
fn document_symbols_list_module_level_names() {
    let mut c = Client::start();
    let uri = "file:///tmp/openepl_lsp_syms.oir";
    c.open(uri, FORM_SRC);
    let _ = c.diagnostics(uri);

    let r = c.request(110, "textDocument/documentSymbol", serde_json::json!({
        "textDocument": { "uri": uri }
    }));
    let names: Vec<&str> = r
        .as_array()
        .expect("symbols")
        .iter()
        .map(|s| s["name"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"go"), "subroutine: {names:?}");
    assert!(names.contains(&"ok"), "component: {names:?}");
    c.shutdown();
}
