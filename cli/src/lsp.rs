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
use lsp_types::request::{
    Completion, DocumentSymbolRequest, GotoDefinition, HoverRequest, References,
    Request as _,
};
use lsp_types::notification::{
    DidChangeTextDocument, DidCloseTextDocument, DidOpenTextDocument, DidSaveTextDocument,
    Notification as _, PublishDiagnostics,
};
use lsp_types::{
    CompletionItem, CompletionItemKind, CompletionOptions, CompletionParams, Diagnostic,
    DiagnosticSeverity, DocumentSymbolParams, Hover, HoverContents, HoverProviderCapability,
    InitializeParams, Location, MarkupContent, MarkupKind, OneOf, Position,
    PublishDiagnosticsParams, Range, ReferenceParams, ServerCapabilities, SymbolInformation,
    SymbolKind, TextDocumentPositionParams, TextDocumentSyncCapability, TextDocumentSyncKind, Uri,
};

use openepl_ir::{parse, validate, Registry, Signature};

use crate::libload;
use crate::lsp_index::{Index, Occurrence, SymKind};

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
        // A client only sends requests for capabilities the server advertises,
        // so anything omitted here is dead code no matter how well it works.
        completion_provider: Some(CompletionOptions {
            // `.` opens the property/event list for a component.
            trigger_characters: Some(vec![".".to_string()]),
            ..Default::default()
        }),
        hover_provider: Some(HoverProviderCapability::Simple(true)),
        definition_provider: Some(OneOf::Left(true)),
        references_provider: Some(OneOf::Left(true)),
        document_symbol_provider: Some(OneOf::Left(true)),
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
        let id: RequestId = req.id.clone();
        // Every request gets *some* response. A client that never hears back
        // about a request it sent waits forever.
        let result: Option<serde_json::Value> = match req.method.as_str() {
            Completion::METHOD => self.on_completion(req.params),
            HoverRequest::METHOD => self.on_hover(req.params),
            GotoDefinition::METHOD => self.on_definition(req.params),
            References::METHOD => self.on_references(req.params),
            DocumentSymbolRequest::METHOD => self.on_document_symbol(req.params),
            _ => {
                let resp = Response::new_err(
                    id,
                    lsp_server::ErrorCode::MethodNotFound as i32,
                    format!("openepl-lsp does not support `{}` yet", req.method),
                );
                let _ = conn.sender.send(Message::Response(resp));
                return;
            }
        };
        // `null` is a valid, meaningful answer: "nothing here".
        let resp = Response::new_ok(id, result.unwrap_or(serde_json::Value::Null));
        let _ = conn.sender.send(Message::Response(resp));
    }

    /// Source text and cursor for a positional request, or `None` if we don't
    /// have the document.
    fn context(&self, params: serde_json::Value) -> Option<(String, usize, usize)> {
        let p: TextDocumentPositionParams = serde_json::from_value(params).ok()?;
        let src = self.docs.get(&p.text_document.uri)?.clone();
        let line = p.position.line as usize + 1; // LSP is 0-based, we are 1-based
        let col = utf16_col_to_byte(&src, line, p.position.character);
        Some((src, line, col))
    }

    fn on_completion(&mut self, params: serde_json::Value) -> Option<serde_json::Value> {
        let p: CompletionParams = serde_json::from_value(params.clone()).ok()?;
        let src = self
            .docs
            .get(&p.text_document_position.text_document.uri)?
            .clone();
        let line_no = p.text_document_position.position.line as usize + 1;
        let col = utf16_col_to_byte(&src, line_no, p.text_document_position.position.character);
        let line_text = nth_line(&src, line_no).unwrap_or_default();
        // Text to the left of the caret. `col` is a 1-based byte column.
        let cut = col.saturating_sub(1).min(line_text.len());
        let before = &line_text[..cut];

        let ix = Index::build(&src);
        let registry = self.registry_for_src(&src);
        let mut items: Vec<CompletionItem> = Vec::new();

        // After `id.` the only sensible completions are that component's
        // properties and events — offering variables there would be noise.
        if let Some(id) = trailing_member_target(before) {
            if let (Some(ty), Some(reg)) = (ix.component_types.get(id), registry.as_ref()) {
                if let Some(desc) = reg.component(ty) {
                    for prop in &desc.properties {
                        items.push(item(
                            &prop.name,
                            CompletionItemKind::PROPERTY,
                            format!("{}: {}", ty, prop.ty.as_str()),
                        ));
                    }
                    for ev in &desc.events {
                        items.push(item(
                            ev,
                            CompletionItemKind::EVENT,
                            format!("{ty} event"),
                        ));
                    }
                }
            }
            return serde_json::to_value(items).ok();
        }

        if let Some(reg) = &registry {
            for (name, cmd) in reg.iter() {
                items.push(item(
                    name,
                    CompletionItemKind::FUNCTION,
                    signature_text(name, &cmd.sig),
                ));
            }
            for name in reg.component_names() {
                items.push(item(name, CompletionItemKind::CLASS, "component".into()));
            }
        }
        for (name, kind) in ix.names_in_scope(ix.scope_at_line(line_no)) {
            let (k, detail) = match kind {
                SymKind::Sub => (CompletionItemKind::FUNCTION, "subroutine"),
                SymKind::Global => (CompletionItemKind::VARIABLE, "module variable"),
                SymKind::Local => (CompletionItemKind::VARIABLE, "local"),
                SymKind::Component => (CompletionItemKind::FIELD, "component"),
                _ => (CompletionItemKind::TEXT, ""),
            };
            items.push(item(name, k, detail.to_string()));
        }
        for kw in KEYWORDS {
            items.push(item(kw, CompletionItemKind::KEYWORD, "keyword".into()));
        }
        serde_json::to_value(items).ok()
    }

    fn on_hover(&mut self, params: serde_json::Value) -> Option<serde_json::Value> {
        let (src, line, col) = self.context(params)?;
        let ix = Index::build(&src);
        let occ = ix.at(line, col)?;
        let registry = self.registry_for_src(&src);

        let text = if let Some(cmd) = registry.as_ref().and_then(|r| r.get(&occ.name)) {
            format!("```\n{}\n```\n\ncommand", signature_text(&occ.name, &cmd.sig))
        } else if let Some(ty) = ix.component_types.get(&occ.name) {
            format!("```\n{} {}\n```\n\ncomponent", ty, occ.name)
        } else if let Some(def) = ix.definition_of(occ) {
            format!(
                "```\n{}\n```\n\n{} — declared on line {}",
                occ.name,
                describe(def.kind),
                def.line
            )
        } else {
            return None;
        };

        serde_json::to_value(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: text,
            }),
            range: Some(occ_range(&src, occ)),
        })
        .ok()
    }

    fn on_definition(&mut self, params: serde_json::Value) -> Option<serde_json::Value> {
        let p: TextDocumentPositionParams = serde_json::from_value(params.clone()).ok()?;
        let uri = p.text_document.uri.clone();
        let (src, line, col) = self.context(params)?;
        let ix = Index::build(&src);
        let occ = ix.at(line, col)?;
        // Commands live in C support libraries: there is no `.oir` position to
        // jump to. Returning null is honest; hover carries the signature.
        let def = ix.definition_of(occ)?;
        serde_json::to_value(Location {
            uri,
            range: occ_range(&src, def),
        })
        .ok()
    }

    fn on_references(&mut self, params: serde_json::Value) -> Option<serde_json::Value> {
        let p: ReferenceParams = serde_json::from_value(params.clone()).ok()?;
        let uri = p.text_document_position.text_document.uri.clone();
        let src = self.docs.get(&uri)?.clone();
        let line = p.text_document_position.position.line as usize + 1;
        let col = utf16_col_to_byte(&src, line, p.text_document_position.position.character);

        let ix = Index::build(&src);
        let occ = ix.at(line, col)?;
        let include_decl = p.context.include_declaration;
        let locs: Vec<Location> = ix
            .references_to(occ)
            .into_iter()
            .filter(|o| include_decl || !o.is_definition)
            .map(|o| Location {
                uri: uri.clone(),
                range: occ_range(&src, o),
            })
            .collect();
        serde_json::to_value(locs).ok()
    }

    /// Powers the editor's outline / breadcrumb view.
    fn on_document_symbol(&mut self, params: serde_json::Value) -> Option<serde_json::Value> {
        let p: DocumentSymbolParams = serde_json::from_value(params).ok()?;
        let src = self.docs.get(&p.text_document.uri)?.clone();
        let ix = Index::build(&src);
        let syms: Vec<SymbolInformation> = ix
            .occurrences
            .iter()
            .filter(|o| o.is_definition && o.scope.is_none())
            .map(|o| {
                #[allow(deprecated)]
                SymbolInformation {
                    name: o.name.clone(),
                    kind: match o.kind {
                        SymKind::Sub => SymbolKind::FUNCTION,
                        SymKind::Component => SymbolKind::FIELD,
                        _ => SymbolKind::VARIABLE,
                    },
                    tags: None,
                    deprecated: None,
                    location: Location {
                        uri: p.text_document.uri.clone(),
                        range: occ_range(&src, o),
                    },
                    container_name: None,
                }
            })
            .collect();
        serde_json::to_value(syms).ok()
    }

    /// Registry for a document, parsed leniently: while typing, the file often
    /// does not parse, and completion must not go dark then. Fall back to the
    /// bare `use`-less registry rather than giving up.
    fn registry_for_src(&mut self, src: &str) -> Option<Registry> {
        let uses = parse(src).map(|m| m.uses).unwrap_or_default();
        self.registry_for(&uses).ok()
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

/// Keywords offered in completion. Small enough to spell out; keeping it here
/// rather than deriving it from the lexer means completion can't accidentally
/// offer internal tokens like `Eof`.
const KEYWORDS: &[&str] = &[
    "module", "sub", "end", "let", "var", "call", "if", "else", "while", "and", "or", "not",
    "true", "false", "use", "form", "on", "int", "double", "bool", "text",
];

fn item(label: &str, kind: CompletionItemKind, detail: String) -> CompletionItem {
    CompletionItem {
        label: label.to_string(),
        kind: Some(kind),
        detail: if detail.is_empty() { None } else { Some(detail) },
        ..Default::default()
    }
}

fn describe(kind: SymKind) -> &'static str {
    match kind {
        SymKind::Sub => "subroutine",
        SymKind::Global => "module variable",
        SymKind::Local => "local variable",
        SymKind::Component => "component",
        SymKind::ComponentType => "component type",
        SymKind::Command => "command",
        SymKind::Property => "property",
    }
}

fn signature_text(name: &str, sig: &Signature) -> String {
    let params = sig
        .params
        .iter()
        .map(|t| t.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    match sig.ret {
        Some(r) => format!("{name}({params}) -> {}", r.as_str()),
        None => format!("{name}({params})"),
    }
}

/// The component id in `... id.partial` to the left of the caret, if the user
/// is completing a member.
///
/// The caret is normally *inside* the member name, not glued to the dot: you
/// type `ok.te` and still expect `text`. So skip back over the partial word
/// first, then require the dot.
fn trailing_member_target(before: &str) -> Option<&str> {
    let is_word = |c: char| c.is_alphanumeric() || c == '_';
    let stem = before.trim_end_matches(is_word).strip_suffix('.')?;
    let start = stem.rfind(|c: char| !is_word(c)).map_or(0, |i| i + 1);
    let id = &stem[start..];
    (!id.is_empty()).then_some(id)
}

fn nth_line(src: &str, line_1based: usize) -> Option<String> {
    src.lines().nth(line_1based.checked_sub(1)?).map(str::to_string)
}

/// Convert an LSP character offset (UTF-16 code units, 0-based) to our 1-based
/// **byte** column.
///
/// LSP measures columns in UTF-16 code units, our lexer in bytes. They agree
/// only for ASCII — so a file containing `"héllo"` would silently misplace
/// every cursor to its right if we skipped this.
fn utf16_col_to_byte(src: &str, line_1based: usize, character: u32) -> usize {
    let Some(line) = nth_line(src, line_1based) else {
        return 1;
    };
    let mut units = 0u32;
    for (byte_idx, ch) in line.char_indices() {
        if units >= character {
            return byte_idx + 1;
        }
        units += ch.len_utf16() as u32;
    }
    line.len() + 1
}

/// Convert our 1-based byte column to an LSP character offset.
fn byte_col_to_utf16(src: &str, line_1based: usize, byte_col: usize) -> u32 {
    let Some(line) = nth_line(src, line_1based) else {
        return 0;
    };
    let upto = byte_col.saturating_sub(1).min(line.len());
    line[..upto].chars().map(|c| c.len_utf16() as u32).sum()
}

/// The LSP range covering one identifier occurrence.
fn occ_range(src: &str, occ: &Occurrence) -> Range {
    let line = occ.line.saturating_sub(1) as u32;
    Range {
        start: Position::new(line, byte_col_to_utf16(src, occ.line, occ.col)),
        end: Position::new(line, byte_col_to_utf16(src, occ.line, occ.end_col())),
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
