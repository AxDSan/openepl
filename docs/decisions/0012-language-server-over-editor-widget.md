# ADR 0012 — A language server, not a hand-written editor widget

**Status:** accepted
**Date:** 2026-08-30
**Context:** PRD §4 (RAD-first), G0, G9

## Context

Studio's code pane was a read-only view. The requirement is a *fully fledged*
code editor: editing, syntax support, completion, go-to-definition,
find-references — the things a developer takes for granted.

There are two ways to get there.

**A. Write an editor widget.** Build a text editor inside the RmlUi-based
Studio: caret movement, selection, multi-cursor, undo grouping, clipboard, IME
and dead keys for non-Latin input, bidi, soft wrap, scroll virtualization for
large files, incremental re-highlighting. Then, separately, build the language
intelligence behind it. The editor half alone is a CodeMirror-sized project, and
every hour spent on it is an hour not spent on the compiler, the component model
or the designer — which are the things nobody else can build for us.

**B. Write a language server.** Implement LSP once. Every editor that already
solved the widget problem — VS Code, Neovim, Helix, Zed, Emacs — becomes a
complete OpenEPL environment. Studio's pane later becomes one more LSP client,
and the intelligence it displays is the same intelligence everyone else gets.

## Decision

**Build `openepl lsp` first.** The language server is the product; editors are
clients. Studio's code pane will be upgraded to an LSP client afterwards, and
will get whatever the server can do at that time for free.

We will not hand-roll an editor widget for Studio. If Studio eventually needs a
richer surface than an LSP client over a modest text control, we will embed an
existing editor rather than write one.

v1 of the server ships exactly one capability: `publishDiagnostics`. It is the
feature that makes an editor genuinely useful, and it forces the whole pipeline
— framing, document sync, position mapping — to be correct before anything is
layered on top. Completion, hover, definition and references follow.

## Consequences

* Diagnostics needed real positions. Statements became `Stmt { kind, line }`,
  the lexer tracks line/column per token, and `ValidateError` carries a line: an
  LSP that reports everything at line 0 is worse than no LSP.
* The workspace gains its first external dependencies (`lsp-server`,
  `lsp-types`, `serde_json`). `lsp-server` is the synchronous stdio transport
  rust-analyzer uses; it drags in no async runtime. Hand-rolling the protocol
  structs is where off-by-spec bugs live, so we don't.
* The server caches the introspected registry per `use` list. Loading it means a
  subprocess and a `dlopen`; doing that per keystroke would make typing lag.
* The server must never exit on a bad workspace. Without a locatable runtime it
  serves parse errors and reports the degradation. An editor plugin that dies on
  open is worse than one that does half the job.
* Tested at the wire, not at the function. `cli/tests/lsp.rs` spawns the real
  binary and speaks real framed JSON-RPC, because a unit test on `diagnose()`
  would pass while the server was silent in every editor on earth — the
  "mechanism works, surface is broken" failure this project keeps hitting.
