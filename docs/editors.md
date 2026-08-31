# Editing OpenEPL

OpenEPL ships a language server, `openepl lsp`, that speaks LSP over stdio. Any
LSP-capable editor becomes an OpenEPL editor by pointing it at that command —
this is why the toolchain does not ship an editor widget of its own (ADR 0012).

## What works today

| Feature | Status |
| --- | --- |
| Diagnostics (parse + type/semantic errors, on type) | ✅ |
| Completion | planned |
| Hover | planned |
| Go to definition / find references | planned |
| Formatting | planned |

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

There is no marketplace extension yet. Any generic LSP bridge extension works;
point it at `openepl lsp` with `documentSelector: ["openepl"]` and map `.oir` to
the `openepl` language id.

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
