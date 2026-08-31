# Editing OpenEPL

OpenEPL ships a language server, `openepl lsp`, that speaks LSP over stdio. Any
LSP-capable editor becomes an OpenEPL editor by pointing it at that command —
the intelligence lives in one server rather than in any single editor.

## What works today

| Feature | Status |
| --- | --- |
| Diagnostics (parse + type/semantic errors, as you type) | ✅ |
| Completion (commands, components, `id.` properties and events, locals, keywords) | ✅ |
| Hover (command signatures, declaration sites) | ✅ |
| Go to definition | ✅ |
| Find references (shadowing-aware) | ✅ |
| Document symbols (outline) | ✅ |
| Syntax highlighting | ✅ (VS Code grammar in `editors/vscode`) |
| Rename, formatting, signature help | planned |

Navigation is backed by a token-level index rather than the AST, so completion
and go-to-definition keep working while the file is half-typed and does not
parse — which is most of the time you are actually editing.

Diagnostics cover the full front end: unknown commands, argument-count and type
mismatches, assignment to a `let`, undefined variables, non-boolean conditions,
unknown component types, bad property names and types, and every parse error —
each reported at the line it occurs on.

The server locates the OpenEPL runtime by walking up from the editor's workspace
root looking for `runtime/openepl_core.h`. If it can't find one it stays up and
serves parse errors, reporting the degradation as a diagnostic on line 1 rather
than going silent.

## Neovim

With `nvim-lspconfig` (or plain `vim.lsp.start`):

```lua
vim.filetype.add({ extension = { oir = "openepl" } })

vim.api.nvim_create_autocmd("FileType", {
  pattern = "openepl",
  callback = function(args)
    vim.lsp.start({
      name = "openepl",
      cmd = { "openepl", "lsp" },
      root_dir = vim.fs.root(args.buf, { "runtime", ".git" }),
    })
  end,
})
```

## VS Code

`editors/vscode/` is a working extension: syntax highlighting, `.oir` file
association, and an LSP client that launches `openepl lsp`. It is not published
to the marketplace, so install it locally:

```sh
cd editors/vscode
npm install
# then either symlink it into your extensions folder…
ln -s "$PWD" ~/.vscode/extensions/openepl
# …or package it
npx @vscode/vsce package && code --install-extension openepl-0.1.0.vsix
```

Set `openepl.serverPath` if `openepl` is not on your `PATH`.

## Helix

In `languages.toml`:

```toml
[language-server.openepl]
command = "openepl"
args = ["lsp"]

[[language]]
name = "openepl"
scope = "source.openepl"
file-types = ["oir"]
roots = ["runtime", ".git"]
language-servers = ["openepl"]
comment-token = "#"
indent = { tab-width = 2, unit = "  " }
```

## Zed

```json
{
  "lsp": { "openepl": { "binary": { "path": "openepl", "arguments": ["lsp"] } } }
}
```

## Debugging the server

The server logs to stderr (stdout is the protocol channel — a stray print there
corrupts the stream). Watch it with your editor's LSP log, or drive it by hand:

```sh
openepl lsp   # then send framed JSON-RPC on stdin
```

`cli/tests/lsp.rs` does exactly that, and is the reference for the wire format.
