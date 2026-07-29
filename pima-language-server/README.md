# Pima Language Server

An LSP server for `.pima` files, built on Pima's lexer and parser.

## Features

- syntax diagnostics while a document is open;
- top-level function and binding symbols;
- hover information for Pima tokens; and
- keyword completion.

## Run

```console
cargo run -p pima-language-server
```

The server communicates over standard input and output. Configure an editor
LSP client to launch that command for files with the `.pima` extension.

## VS Code

The client extension is in `editors/vscode`. Build the server first, then
install and compile the extension:

```console
cargo build -p pima-language-server
cd pima-language-server/editors/vscode
npm install
npm run compile
```

Open the `editors/vscode` directory in VS Code and press F5 to launch an
Extension Development Host with the Pima workspace open. The extension uses
the workspace's debug server build by default. Set `pima.languageServer.path`
to use a server executable elsewhere.
