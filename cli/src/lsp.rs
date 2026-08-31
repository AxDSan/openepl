//! `openepl lsp` — a Language Server Protocol server for `.oir` sources.
//!
//! This is how OpenEPL gets a real editing experience without writing an editor
//! widget. Any LSP client (VS Code, Neovim, Helix, Zed — and, later, Studio's
//! own code pane) speaks to this over stdio and gets live diagnostics.
//!
//! v1 scope is deliberately one feature: `textDocument/publishDiagnostics`.
//! Completion, hover and goto-definition are follow-ups; a squiggle under the
//! actual mistake is the thing that makes an editor usable, and it is the thing
//! that exercises the whole pipeline (framing, sync, position mapping).
//!
//! Two design points worth stating:
//!
//! * **The registry is loaded once, at initialize.** Validation needs the
//!   command/component registry, which comes from introspecting support
//!   libraries — a subprocess and a `dlopen`. Doing that per keystroke would
//!   make typing lag. It is cached for the life of the server.
//! * **The server never exits on a bad workspace.** If the runtime can't be
//!   located, we still serve parse errors and report the degradation as a
//!   diagnostic. An editor plugin that dies on open is worse than one that
//!   does half the job.

use std::collections::HashMap;
use std::error::Error;
use std::path::{Path, PathBuf};

use lsp_server::{Connection, ExtractError, Message, Notification, Request, RequestId, Response};
use lsp_types::notification::{
    DidChangeTextDocument, DidCloseTextDocument, DidOpenTextDocument, DidSaveTextDocument,
    Notification as _, PublishDiagnostics,
};
use lsp_types::{
    Diagnostic, DiagnosticSeverity, InitializeParams, Position, PublishDiagnosticsParams, Range,
    ServerCapabilities, TextDocumentSyncCapability, TextDocumentSyncKind, Uri,
};

use openepl_ir::{parse, validate, Registry};

use crate::libload;

/// Entry point for the `lsp` subcommand. Returns a process exit code.
pub fn run() -> i32 {
    // Diagnostics and logging go to stderr: stdout is the protocol channel and
    // a stray `println!` corrupts the stream.
    eprintln!("openepl-lsp: starting on stdio");
    match serve() {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("openepl-lsp: {e}");
            1
        }
    }
}

fn serve() -> Result<(), Box<dyn Error + Sync + Send>> {
    let (connection, io_threads) = Connection::stdio();

    let caps = serde_json::to_value(ServerCapabilities {
        // Full-text sync: `.oir` files are small and we re-parse from scratch
        // anyway, so incremental sync would be complexity with no payoff.
        text_document_sync: Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL)),
        ..Default::default()
    })?;

    let init_params = connection.initialize(caps)?;
    let mut server = Server::new(serde_json::from_value(init_params).unwrap_or_default());

    let result = server.main_loop(&connection);

    // Drop the connection BEFORE joining. The writer thread lives until its
    // channel closes, and the channel closes when the last `Sender` — held by
    // `connection` — goes away. Joining first hangs the process forever, which
    // means every editor session would leak an `openepl lsp` behind it.
    drop(connection);
    io_threads.join()?;
    result?;
    eprintln!("openepl-lsp: shutting down");
    Ok(())
}

struct Server {
    /// Open documents, by URI. The client is the source of truth for content
    /// while a file is open — never read it back off disk.
    docs: HashMap<Uri, String>,
    /// Repo root that supplies the runtime, or `None` in degraded mode.
    repo_root: Option<PathBuf>,
    /// Cached registries, keyed by the module's `use` list. Loading is
    /// expensive; a module's imports rarely change between keystrokes.
    registries: HashMap<Vec<String>, Result<Registry, String>>,
}

impl Server {
    fn new(params: InitializeParams) -> Server {
        let repo_root = workspace_root(&params).and_then(|r| find_repo_root_from(&r));
        match &repo_root {
            Some(r) => eprintln!("openepl-lsp: runtime at {}", r.display()),
            None => eprintln!("openepl-lsp: no runtime found — parse-only mode"),
        }
        Server {
            docs: HashMap::new(),
            repo_root,
            registries: HashMap::new(),
        }
    }

    fn main_loop(&mut self, conn: &Connection) -> Result<(), Box<dyn Error + Sync + Send>> {
        for msg in &conn.receiver {
            match msg {
                Message::Request(req) => {
                    if conn.handle_shutdown(&req)? {
                        return Ok(());
                    }
                    self.on_request(conn, req);
                }
                Message::Notification(note) => self.on_notification(conn, note),
                Message::Response(_) => {}
            }
        }
        Ok(())
    }

    fn on_request(&mut self, conn: &Connection, req: Request) {
        // v1 answers no requests beyond the lifecycle ones `lsp-server` handles.
        // Reply with an error rather than silence: a client that never gets a
        // response to a request it sent will hang waiting for one.
        let resp = Response::new_err(
            req.id.clone(),
            lsp_server::ErrorCode::MethodNotFound as i32,
            format!("openepl-lsp does not support `{}` yet", req.method),
        );
        let _ = conn.sender.send(Message::Response(resp));
        let _: RequestId = req.id;
    }

    fn on_notification(&mut self, conn: &Connection, note: Notification) {
        match note.method.as_str() {
            DidOpenTextDocument::METHOD => {
                if let Ok(p) = cast_note::<DidOpenTextDocument>(note) {
                    let uri = p.text_document.uri.clone();
                    self.docs.insert(uri.clone(), p.text_document.text);
                    self.publish(conn, &uri);
                }
            }
            DidChangeTextDocument::METHOD => {
                if let Ok(p) = cast_note::<DidChangeTextDocument>(note) {
                    // FULL sync: the last change carries the whole document.
                    if let Some(change) = p.content_changes.into_iter().next_back() {
                        let uri = p.text_document.uri.clone();
                        self.docs.insert(uri.clone(), change.text);
                        self.publish(conn, &uri);
                    }
                }
            }
            DidSaveTextDocument::METHOD => {
                if let Ok(p) = cast_note::<DidSaveTextDocument>(note) {
                    self.publish(conn, &p.text_document.uri);
                }
            }
            DidCloseTextDocument::METHOD => {
                if let Ok(p) = cast_note::<DidCloseTextDocument>(note) {
                    let uri = p.text_document.uri;
                    self.docs.remove(&uri);
                    // Clear the squiggles: stale diagnostics on a closed file
                    // linger in the client's problem list otherwise.
                    send_diagnostics(conn, &uri, Vec::new());
                }
            }
            _ => {}
        }
    }

    fn publish(&mut self, conn: &Connection, uri: &Uri) {
        let Some(src) = self.docs.get(uri).cloned() else {
            return;
        };
        send_diagnostics(conn, uri, self.diagnose(&src));
    }

    /// Compile far enough to collect diagnostics, and no further.
    fn diagnose(&mut self, src: &str) -> Vec<Diagnostic> {
        let module = match parse(src) {
            Ok(m) => m,
            // A parse error stops everything downstream — while you are typing,
            // this is the common case, so it must be positioned well.
            Err(e) => return vec![diag(e.line, e.msg)],
        };

        let registry = match self.registry_for(&module.uses) {
            Ok(r) => r,
            Err(msg) => {
                // Degraded: we parsed, but can't type-check. Say so once, at the
                // top of the file, instead of pretending the file is clean.
                return vec![diag(1, format!("OpenEPL runtime unavailable: {msg}"))];
            }
        };

        match validate(&module, &registry) {
            Ok(()) => Vec::new(),
            Err(errs) => errs
                .into_iter()
                // `msg`, not `to_string()`: Display prefixes "line N:", which
                // the editor already shows via the range.
                .map(|e| diag(e.line, e.msg))
                .collect(),
        }
    }

    fn registry_for(&mut self, uses: &[String]) -> Result<Registry, String> {
        let key = uses.to_vec();
        if let Some(cached) = self.registries.get(&key) {
            return cached.clone();
        }
        let result = match &self.repo_root {
            None => Err("could not locate runtime/openepl_core.h from the workspace root".into()),
            Some(root) => libload::load(root, uses).map(|p| p.registry),
        };
        self.registries.insert(key, result.clone());
        result
    }
}

/// Where to start looking for the runtime. Prefer the client's workspace,
/// falling back to our own working directory.
fn workspace_root(params: &InitializeParams) -> Option<PathBuf> {
    #[allow(deprecated)]
    if let Some(folders) = &params.workspace_folders {
        if let Some(f) = folders.first() {
            if let Some(p) = uri_to_path(&f.uri) {
                return Some(p);
            }
        }
    }
    #[allow(deprecated)]
    if let Some(root) = &params.root_uri {
        if let Some(p) = uri_to_path(root) {
            return Some(p);
        }
    }
    std::env::current_dir().ok()
}

/// Turn a `file:` URI into a path.
///
/// `lsp-types` 0.97 exposes a bare RFC-3986 `Uri` with no filesystem mapping,
/// so we do the two things that actually matter: require the `file` scheme and
/// percent-decode the path. Anything else (a URI from a virtual filesystem) is
/// not something we can locate a runtime relative to.
fn uri_to_path(uri: &Uri) -> Option<PathBuf> {
    if uri.scheme().map(|s| s.as_str().to_ascii_lowercase()) != Some("file".into()) {
        return None;
    }
    let raw = uri.path().as_str();
    let bytes = raw.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).ok()?;
            if let Ok(b) = u8::from_str_radix(hex, 16) {
                out.push(b);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    Some(PathBuf::from(String::from_utf8(out).ok()?))
}

/// Walk up from `start` looking for `runtime/openepl_core.h`, mirroring the
/// build path's search but rooted at the editor's workspace rather than at CWD.
fn find_repo_root_from(start: &Path) -> Option<PathBuf> {
    let mut dir = start.to_path_buf();
    loop {
        if dir.join("runtime/openepl_core.h").is_file() {
            return Some(dir);
        }
        if !dir.pop() {
            return None;
        }
    }
}

/// Build a diagnostic covering a whole 1-based source line.
///
/// The validator has no columns yet, so a line-wide range is the honest
/// rendering: it underlines exactly what we actually know is wrong.
fn diag(line_1based: usize, msg: String) -> Diagnostic {
    // LSP lines are 0-based; ours are 1-based, and 0 means "unknown".
    let line = line_1based.saturating_sub(1) as u32;
    Diagnostic {
        range: Range {
            start: Position::new(line, 0),
            // u32::MAX is clamped by the client to the real end of the line.
            end: Position::new(line, u32::MAX),
        },
        severity: Some(DiagnosticSeverity::ERROR),
        source: Some("openepl".into()),
        message: msg,
        ..Default::default()
    }
}

fn send_diagnostics(conn: &Connection, uri: &Uri, diagnostics: Vec<Diagnostic>) {
    let params = PublishDiagnosticsParams {
        uri: uri.clone(),
        diagnostics,
        version: None,
    };
    if let Ok(params) = serde_json::to_value(params) {
        let _ = conn.sender.send(Message::Notification(Notification {
            method: PublishDiagnostics::METHOD.to_string(),
            params,
        }));
    }
}

fn cast_note<N>(note: Notification) -> Result<N::Params, ExtractError<Notification>>
where
    N: lsp_types::notification::Notification,
    N::Params: serde::de::DeserializeOwned,
{
    note.extract(N::METHOD)
}
