# Pima Interpreter Architecture

Status: implemented baseline architecture. Sections describing future work are
called out explicitly.

The first implementation is a tree-walk interpreter. The architecture keeps
syntax, runtime data, evaluation, native functions, and host integration
separate so that a bytecode compiler can be added later without rewriting the
language model.

## 1. Crate layout

```text
pima/
├── Cargo.toml
├── src/
│   ├── lib.rs
│   ├── main.rs
│   ├── cli.rs
│   ├── source/
│   │   ├── mod.rs
│   │   ├── map.rs
│   │   └── span.rs
│   ├── diagnostic/
│   │   ├── mod.rs
│   │   └── render.rs
│   ├── syntax/
│   │   ├── mod.rs
│   │   ├── token.rs
│   │   ├── lexer.rs
│   │   ├── ast.rs
│   │   └── parser.rs
│   ├── runtime/
│   │   ├── mod.rs
│   │   ├── ids.rs
│   │   ├── symbol.rs
│   │   ├── value.rs
│   │   ├── binding.rs
│   │   ├── environment.rs
│   │   ├── function.rs
│   │   ├── namespace.rs
│   │   └── error.rs
│   ├── engine/
│   │   ├── mod.rs
│   │   ├── interpreter.rs
│   │   ├── eval.rs
│   │   ├── call.rs
│   │   ├── instantiate.rs
│   │   └── module_loader.rs
│   └── native/
│       ├── mod.rs
│       ├── registry.rs
│       ├── numbers.rs
│       ├── strings.rs
│       ├── lists.rs
│       ├── types.rs
│       ├── console.rs
│       └── io.rs
├── stdlib/
│   └── standard.pima
├── examples/
└── tests/
    ├── lexer.rs
    ├── parser.rs
    ├── evaluation.rs
    ├── functions.rs
    ├── namespaces.rs
    ├── modules.rs
    ├── errors.rs
    └── conformance.rs
```

This is a responsibility map, not a requirement that every listed file exist.
Calls and namespace instantiation live in `engine/call.rs` and
`engine/instantiate.rs`. These modules own their complete behavior rather than
forwarding to the AST dispatcher. `/pima/io` is implemented in `native/io.rs`
and exposed only through its virtual module.

## 2. Dependency direction

Dependencies point inward in this order:

```text
source
├── diagnostic
└── syntax
    └── runtime
        └── native

engine ──► source + diagnostic + syntax + runtime + native
CLI ─────► public library API
```

The important constraints are:

- `source` has no dependency on the language runtime.
- `syntax` may use source spans and host diagnostics, but never runtime values.
- `runtime` may refer to AST node handles for user functions and blocks.
- `native` operates through a narrow context trait implemented by the engine;
  it does not depend on engine internals, parse source, or control module
  loading directly.
- `engine` is the only layer that coordinates syntax, environments, calls,
  namespaces, modules, native functions, and control transfer.
- `cli` depends on the public API from `lib.rs`, not on private engine details.

## 3. Public API

`lib.rs` exposes a small embedding API:

```rust
pub struct Interpreter { /* private */ }

impl Interpreter {
    pub fn new(config: Config) -> Self;
    pub fn run_file(&mut self, path: impl AsRef<Path>) -> RunOutcome;
    pub fn run_source(&mut self, name: &str, source: &str) -> RunOutcome;
}

pub struct RunOutcome {
    pub value: Option<Value>,
    pub diagnostics: Vec<Diagnostic>,
}
```

Exact Rust signatures may evolve, but callers should not need lexer, parser, or
environment internals to run Pima code. `Value` may be inspectable through a
stable public view without exposing arena identifiers.

The CLI performs argument parsing, creates an interpreter, runs a file, renders
diagnostics, and selects the process exit code. Language behavior never belongs
in `main.rs`.

## 4. Source and diagnostics

### `source`

Owns:

- `SourceId`, a stable identifier for a loaded source;
- `Span { source, start, end }`, using byte offsets internally;
- `SourceMap`, which retains source text and maps offsets to line/column; and
- canonical source names and module paths.

The lexer uses byte offsets because Rust strings are UTF-8. User-facing columns
are calculated consistently by `SourceMap`.

### `diagnostic`

Renders host-facing syntax and uncaught-runtime diagnostics. A diagnostic has a
severity, message, primary span, optional labeled spans, and Pima stack frames.

Diagnostics are not Pima error values. A thrown Pima error is a runtime value;
the diagnostic layer renders that value plus runtime-attached source and stack
metadata when it escapes the interpreter.

## 5. Syntax layer

### `token`

Defines token kinds without borrowing source substrings. Important distinct
tokens include:

- physical EOL;
- identifiers and operator identifiers;
- symbol literals such as `:name`;
- integer, float, and string literals;
- `.`, `@`, `_`, brackets, braces, and parentheses; and
- reserved words.

Tokens retain spans. The lexer must not resolve bindings or determine function
arity.

### `lexer`

Performs UTF-8 validation, comments, escapes, numeric tokenization, physical
line tracking, and punctuation disambiguation.

### `ast`

Represents logical statements and expressions after EOL continuation has been
resolved. The AST distinguishes:

- ordinary line calls;
- immediate bracket calls;
- list and symbol literals;
- plain and context-annotated code blocks;
- declarations and assignment;
- function declarations;
- member access;
- control forms;
- imports, `new`, `do`, and `attempt`; and
- `throw` and other control transfer.

Every node has a span. Code blocks contain AST statements but no captured
environment.

### `parser`

Transforms physical tokens into logical statements. It owns the inline-block
continuation rule and produces recovery diagnostics for malformed source.
Parsing never depends on runtime arity.

## 6. Runtime layer

Runtime objects are owned by the interpreter and referenced with small typed
handles:

```rust
struct EnvironmentId(u32);
struct FunctionId(u32);
struct BlockId(u32);
struct NamespaceId(u32);
struct ModuleId(u32);
struct NativeFunctionId(u16);
```

Typed handles prevent accidental cross-arena access. The first version may use
generational arenas or stable index-based stores. Raw references should not be
held across interpreter mutations.

### `symbol`

Interns symbol names once per interpreter. Equality is identifier equality, and
display recreates the `:name` spelling.

### `value`

The central value enum contains only values or stable handles:

```rust
enum Value {
    Unit,
    Boolean(bool),
    Integer(i64),
    Float(f64),
    String(Arc<str>),
    Symbol(SymbolId),
    List(PersistentList),
    Function(FunctionId),
    NativeFunction(NativeFunctionId),
    Block(BlockId),
    Namespace(NamespaceId),
}
```

User and native functions both report `:function` through Pima's type system.
Float equality follows the language specification rather than Rust's derived
`Eq`.

### `binding` and `environment`

A binding records:

- its current value;
- immutable or mutable status;
- private or public visibility; and
- whether it is owned locally or is a read-only imported view.

An environment owns bindings and has an optional lexical parent. Imported views
refer to exporter-owned binding cells so that importers observe internal
changes to a `pub var` without being able to assign it.

### `function`

A user function stores its parameter symbols, body AST handle, defining
environment, declaration span, and display name. This is where closures capture
lexical scope. A block stores no environment. It may carry a validated list of
required context symbols declared by `@`. The common block-execution path
checks those symbols against the supplied environment's lexical lookup chain
before executing any statement, regardless of whether execution was initiated
by `do`, a control form, `attempt`, or `new`.

### `namespace`

A namespace owns an environment and a validated immutable type-symbol list.
Member access applies visibility rules before returning the binding value.

### `error`

Defines runtime error constructors and throw metadata. Errors presented to Pima
are namespace values classified with `:error`; Rust error enums are reserved
for failures of the host or interpreter implementation itself.

## 7. Engine

`Interpreter` owns all mutable interpreter state:

- source map and parsed modules;
- symbol interner;
- runtime object arenas;
- root and module environments;
- module lifecycle cache;
- native-function registry;
- Pima call stack; and
- configuration and host capabilities.

### Evaluation result

Non-local Pima control flow should not use Rust panics:

```rust
enum Signal {
    Return(Value),
    Break(Value),
    Continue,
    Throw(Value),
}

type EvalResult = Result<Value, Signal>;
```

Normal evaluation returns `Ok(Value)`. Functions consume `Return`, loops consume
`Break` and `Continue`, and `attempt` consumes `Throw`. Each construct forwards
signals it does not own. Function calls must reject a `Break` or `Continue`
attempting to cross their boundary.

### `eval`

Dispatches AST nodes and evaluates declarations, blocks, control forms, and
ordinary expressions. Evaluation order is explicit and left-to-right. All
block-aware forms share one execution operation that validates context
requirements, evaluates statements in order, returns the final value, and
propagates non-local control flow.

### `call`

Validates arity, creates function environments, binds parameter symbols,
dispatches native IDs, implements partial application, and maintains Pima stack
frames.

### `instantiate`

Implements `new`: create a namespace environment linked to the current scope,
execute the uninstantiated block, validate types, enforce member visibility,
and publish the namespace only after successful completion.

### `module_loader`

Owns path resolution and the `:unloaded`, `:loading`, `:loaded`, and `:failed`
state machine. It prevents partial exports, detects cycles, and creates aliased
or unaliased read-only import views.

## 8. Native functions

The registry maps `NativeFunctionId` to metadata and an implementation:

```rust
struct NativeDefinition {
    name: &'static str,
    arity: Arity,
    call: NativeCall,
}
```

Native functions receive evaluated values through a narrow `NativeContext`
interface and return `NativeResult`, conceptually
`Result<Value, Value>`, where the error variant must contain a typed Pima error
value. The engine converts that error into `Signal::Throw`. Native functions
must never panic on bad Pima input. It cannot invoke arbitrary Pima code,
inspect caller syntax, mutate caller bindings, bypass visibility, or transfer
`return`, `break`, or `continue` across the native boundary.

Only operations that require host access, primitive representation access, or
acceptable baseline performance are native:

- numeric primitives;
- primitive comparison and type inspection;
- immutable-list primitives;
- core string operations;
- console output;
- filesystem operations backing `/pima/io`; and
- socket primitives backing `/pima/tcp`.

Ranges, traversal helpers, exponentiation, and similar utilities remain Pima
standard-library code.

Special forms such as `if`, `function`, `let`, `@`, `do`, and `attempt` are engine
AST operations, not fake native functions, because they control evaluation or
scope.

## 9. Standard and native modules

`stdlib/standard.pima` is compiled through the same lexer and parser as user code
and embedded in the executable with `include_str!` for reliable availability.

The virtual modules `/pima/library/standard`, `/pima/io`, and `/pima/tcp` are
resolved by the module loader. The standard library is Pima source. Native
modules export functions through ordinary immutable module namespaces so their
behavior at the language level matches other modules.

I/O natives use internally qualified registry names such as `io.join`; the
module loader maps those to public member names such as `join`. This prevents
collisions with identically named operations in other namespaces. Filesystem
operations resolve relative paths against the interpreter's configured working
directory and translate host failures into portable typed Pima errors.

TCP listeners and connections are opaque Pima values backed by
interpreter-owned socket arenas. Rust owns resource lifetime and operating
system calls only. Framing and application protocols, including the example
HTTP server, remain Pima code.

The evaluator keeps host natives in a private implementation environment used
to construct the standard library. User modules inherit only arithmetic and
comparison operators. String, list, type, logic, numeric utility, and console
operations require an explicit standard-library import and qualified access
(or a static namespace import).

## 10. Testing strategy

Tests are divided by responsibility:

- lexer tests assert token kinds and spans;
- parser tests assert AST shape and malformed-source recovery;
- runtime unit tests cover symbols, bindings, persistent lists, and type lists;
- evaluator tests cover scope, calls, blocks, control flow, and errors;
- module tests cover visibility, aliases, cycles, cached failure, and live
  export views;
- conformance tests execute every supported example and compare its result or
  output; and
- negative conformance tests assert error type and primary source span.

Host panics are test failures. Expected Pima failures must appear as typed Pima
error values or rendered uncaught-error diagnostics.

## 11. Implementation order

1. Source map, spans, tokens, and lexer.
2. AST and parser with logical-EOL handling.
3. Symbols, primitive values, persistent lists, bindings, and environments.
4. Evaluator for literals, calls, declarations, and core native functions.
5. Functions, closures, blocks, and control transfer.
6. Namespaces, visibility, type lists, and `new`.
7. Typed errors, `throw`, `attempt`, and diagnostics.
8. Module lifecycle, imports, and bundled modules.
9. Strings, console output, and native host modules.
10. Standard library and full conformance suite.

Each stage should leave the crate compiling and add focused tests before the
next layer is introduced.
