// Thin LSP client: everything intelligent lives in `openepl lsp` (ADR 0012),
// so this file only starts the server and gets out of the way.
const { workspace } = require("vscode");
const { LanguageClient, TransportKind } = require("vscode-languageclient/node");

let client;

function activate(context) {
  const command = workspace
    .getConfiguration("openepl")
    .get("serverPath", "openepl");

  const serverOptions = {
    run: { command, args: ["lsp"], transport: TransportKind.stdio },
    debug: { command, args: ["lsp"], transport: TransportKind.stdio },
  };

  client = new LanguageClient(
    "openepl",
    "OpenEPL Language Server",
    serverOptions,
    { documentSelector: [{ scheme: "file", language: "openepl" }] }
  );
  context.subscriptions.push(client.start());
}

function deactivate() {
  return client ? client.stop() : undefined;
}

module.exports = { activate, deactivate };
