# Pima Language Server

The language server and editor integration are dual-licensed under the MIT
License or Apache License 2.0, at your option.

An LSP server for `.pima` files, built on Pima's lexer and parser.

See [docs/capabilities.md](docs/capabilities.md) for analysis behavior,
dynamic-language limits, and the complete editor integration guide.

It provides diagnostics, semantic navigation and completion, signature help,
workspace import indexing, formatting, semantic tokens, inlay hints, and
`snake_case` quick fixes. Workspace references, rename, watched-file diagnostics,
and conservative inferred value kinds are supported. The capability guide is the single source of truth
for the complete feature list and known dynamic-language limits.

## Run

```console
cargo run -p pima-language-server
```

The server communicates over standard input and output.

## VS Code

The client extension is in `editors/vscode`. Build the server first, then
install and compile the extension:

```console
cargo build -p pima-language-server
cd pima-language-server/editors/vscode
npm install
npm run compile
```

For launch configuration and runtime behavior, follow the
[VS Code guide](docs/capabilities.md#vs-code).
