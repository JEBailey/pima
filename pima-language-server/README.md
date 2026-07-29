# Pima Language Server

An LSP server for `.pima` files, built on Pima's lexer and parser.

## Features

- syntax diagnostics while a document is open;
- top-level function and binding symbols;
- recoverable analysis while a document contains syntax errors;
- lexical hover information for functions, parameters, bindings, and pattern
  captures;
- standard-library member hover and member completion;
- scope- and declaration-aware local completion;
- signature help for standard and user-defined functions;
- semantic highlighting for declarations, references, literals, and standard
  members;
- multiline block and collection folding;
- hierarchical selection ranges;
- parameter-name inlay hints for known calls;
- workspace indexing for `.pima` modules;
- cross-file completion and go to definition through aliased and unaliased
  imports;
- go to definition and find references;
- scope-aware rename with symbolic declaration preservation;
- nested namespace and function document symbols;
- keyword completion;
- naming-convention warnings for functions, parameters, and captures; and
- static errors for immutable assignment and excess arguments to known user
  functions.

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
