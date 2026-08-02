# Pima language-server capabilities

The server uses Pima's lexer, recoverable parser, and AST. It communicates over
standard input and output and analyzes the unsaved text supplied by the editor.

## Analysis

Each open document has one immutable analysis snapshot containing tokens, a
partial AST, diagnostics, lexical scopes, declarations, and resolved
references. Editor requests share that snapshot. Full-text changes are
coalesced for 75 milliseconds, stale document versions are discarded, and a
request arriving during the delay analyzes the newest text immediately.

Parser recovery preserves valid declarations around an incomplete statement.
Lexer errors prevent AST-based features because token boundaries are not
reliable in that case.

## Editor features

- lexer, parser, naming, mutation, and known-arity diagnostics;
- hover for lexical symbols and catalogued standard-library members;
- lexical and imported completion;
- signature help and active-parameter tracking;
- document symbols, definitions, references, and rename;
- semantic tokens, folding ranges, selection ranges, and inlay hints;
- workspace indexing, cross-file definitions, references, and rename;
- workspace file watching and diagnostics for unopened project files;
- conservative inferred value kinds in hover and completion details;
- standard-library catalog support for `Reference.same?` storage-identity calls;
- `remote` request inference as `:future`, with future-member completion,
  hover, and signature help for `complete?`;
- document formatting; and
- naming quick fixes.

Workspace indexing follows relative file imports. It records only public
top-level declarations and public object members. Open buffers replace
their disk snapshots until closed.

## Dynamic limits

Pima deliberately supports behavior that cannot always be established
statically:

- `do` may add bindings to its caller's environment;
- code blocks may execute in different compatible environments;
- object values can be selected or returned dynamically;
- functions support partial application; and
- values and object types are checked at runtime.

The server reports an error only when the static model can prove it, such as an
assignment to a resolved immutable binding or excess arguments to a known user
function. It does not report every unresolved identifier, because the name may
come from a module import, an object import, or a caller-environment block.

Cross-file references and rename follow statically resolved file imports,
including aliased member access. Dynamic object selection and bindings
introduced by `do` remain intentionally outside project-wide refactoring.

Inference reports only values established directly from syntax (literals,
lists, blocks, functions, objects, and compatible conditional results). It
does not guess call results or treat runtime object types as static types.

## VS Code

Build the Rust server:

```console
cargo build -p pima-language-server
```

Then build and run the extension:

```console
cd pima-language-server/editors/vscode
npm install
npm run compile
```

Open that directory in VS Code and press F5. The extension launches the
workspace debug build by default. Set `pima.languageServer.path` to select
another executable.

The client uses full-document synchronization because Pima's physical line
endings participate in parsing. The server still caches and coalesces analysis,
so multiple feature requests do not repeatedly parse the same version.

## Release artifacts

Tagged builds produce platform-specific VSIX packages and standalone optimized
language-server binaries for Windows x64, Linux x64, Intel macOS, and Apple
Silicon. Every packaged binary completes an LSP initialize/shutdown smoke test,
and each VSIX is inspected for its client, grammar, icon, and matching server.
Release artifacts include SHA-256 checksums. Tags containing `-` are published
as GitHub prereleases.
