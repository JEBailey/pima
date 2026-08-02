# Pima Interpreter Architecture

Concurrency foundation: the VM has transport-only remote object and future
handles, a synchronized concurrency hub, and `remote`/`await` AST boundaries.
Remote object requests run in isolated worker VMs and always produce
futures; only `await` waits for the transported value. See
[`remote-objects.md`](remote-objects.md).

Status: register VM implemented as Pima's sole execution engine.

The register compiler and VM implement the Pima execution path. The architecture keeps
syntax, runtime data, evaluation, native functions, and host integration
separate so VM coverage can grow without rewriting the language model.

## Register VM

The VM pipeline is:

```text
AST -> scope analysis -> register lowering -> compiler pass pipeline -> VM
```

The compiler pass pipeline is an explicit extension point between lowering and
execution. Passes run in insertion order and visit both the module body and all
compiled function bodies. Custom pipelines can add analysis or transformation
stages; an empty pipeline is also available for measurement and debugging. The
standard pipeline currently normalizes control flow by threading jump chains
and removing no-op moves and jumps while preserving source-span alignment.
`compile_with_pipeline` and its module/global variants accept a caller-built
`PassPipeline`; `PassPipeline::standard` selects production passes, while
`PassPipeline::new` starts empty for baselines and compiler experiments.

The IR supports constants, moves, immutable lists, direct primitive calls,
private immutable and mutable bindings, conditional and unconditional jumps,
`if`, ordered `branch`, `while`, `until`, `break`, `continue`, first-class functions, recursive
calls, compiled capture/list patterns, and function return.
It also supports `throw` and `attempt` through explicit handler records that
unwind VM call frames without using Rust stack unwinding.
Object construction supports immutable and mutable state, functions,
lexical initializer reads, validated custom type lists, and visibility-checked
member access. Each object scope contains a private immutable `this` cell that
is populated with the completed namespace after construction; methods capture
the cell and preserve exact object identity. That cell is also the construction
lifecycle token retained by functions and blocks created during `new`. An
internal VM handler records the causal error if initialization fails, leaving
the token invalid. Later use produces `:invalid_object`, whose
`construction_error` member preserves the original typed error and diagnostic
metadata. Block literals produce inert linked block values. `do` dispatches
both local and cross-module blocks in the caller's current environment, including
blocks passed through function parameters. Placeholder-based partial application
is compiled as an ordinary VM callable value.

Multiple `new` operands use ordered namespace composition. Lowering selects
complete declarations before emitting initializer code, rejects partial
selection of destructuring declarations, and emits exactly one namespace.
Contributing templates never become runtime objects; all surviving closures
bind to the single completed `this` cell. `types` contributions are merged as
the explicitly documented metadata exception.

Namespaces cache whether their validated type set includes `:error`.
Language equality checks this marker before identity or structural comparison,
making every error object unequal even to itself. Because list equality recurs
through the same operation, lists containing errors are unequal at those
positions without requiring special collection logic.

Annotated block requirements are checked when the block is instantiated.
Missing bindings execute a typed-error instruction and therefore remain
catchable by `attempt` as `(:error :name_error :missing_context)` rather than
being reported as VM compiler failures.
Requirements also retain a worker-transfer mode: bare names copy, `*name`
moves, and `&name` shares a synchronized remote or future handle. Remote
lowering records the originating register for moved values. The machine
invalidates the root shared location only after transport and worker
construction succeed. Reference-like assignment links bindings to that
location, so every alias observes its replacement with
`(:error :move_error :moved_value)`. The error carries structured operation
and source-span provenance. Shared handles remain usable because
related worker interpreters use the same concurrency hub while keeping
separate VM heaps.
Transport deliberately has no local heap-graph serializer. `TransportValue`
recurses only through persistent lists of transportable data and rejects local
namespaces, closures, blocks, cells, and connections. Remote/future handles
have explicit transport representations, while shared TCP listeners are
admitted only by the `&` path. Conversion completes before source invalidation,
so discovering an unsendable value at any depth leaves all caller locations
unchanged.
Unsupported AST constructs produce compiler diagnostics rather than executing
through an alternate runtime.

Object namespace entries retain both visibility and declaration mutability.
`let object.member value` lowers to a member-store instruction; the VM resolves
the object and replacement before committing the store, then enforces
visibility and `var` mutability. Stores through `this` carry object-private
access, while ordinary member stores require `pub var`. Mutable namespace
entries link back to their VM binding cells so lexical reads and member reads
cannot diverge after an update.

Functions compile to ordinary runtime closure values containing a compiled
program id, function id, and captured slots. The machine retains loaded
programs by id, and each call frame selects its owning program's constants,
function table, and module identity. This lets exported closures cross compiled
module boundaries without interpreting them against the caller's tables. Declarations
bind shared, initially uninitialized cells at the point where the declaration
executes. A separate analysis pass finds bindings and static block identities
in expressions and executable blocks that share the current scope, while
stopping at actual child scopes. It expands literal and named `do` blocks,
resolves block aliases, and guards expansion cycles.
This preserves declaration timing while allowing recursion and references to
bindings initialized later. Dynamic calls restore captures into the callee
frame. Mutable bindings use the same cells, so closures observe later `let`
updates instead of copied snapshots.

Destructuring first validates and extracts the complete pattern, then commits
its bindings or assignments. A failed nested pattern therefore cannot leave a
partial mutation behind. Shape failures and invalid runtime binding operations,
including unknown or immutable `let` targets, become typed Pima errors and can
be caught by `attempt`.

VM cells participate in the tracing collector used by runtime object values.
Slots trace language values, closure capture arrays, and nested cells. Closure
and cell values may therefore be stored in captured cells without leaking
unreachable cycles.

The traceable VM representation lives in `runtime::vm_value`, separate from the
instruction dispatcher. Closures are also ordinary `Value` variants, so they
can cross lists, object members, results, registers, and captures without
construct-specific conversion code. VM binding cells remain an internal slot
detail and participate in the same tracing collector.

Working-directory and TCP resource ownership live in
`native::host::HostResources` and are exposed through the VM native context.

The register VM is used by `Interpreter::new`, `run_source`, and
`run_prepared`. The interpreter retains one machine, its loaded compiled
programs, module cache, and session globals, so declarations and closures remain
usable across source submissions.

VM module initialization resolves file and virtual imports recursively before
lowering the importing module. Compiled modules publish bindings with their
declared visibility; unaliased imports expose public members, aliases retain a
object value, and selected object imports are resolved from the linked
globals. Imported closures retain their owning program id and dispatch through
that program's constants and function table. `/pima/io`, `/pima/tcp`, and the
Pima standard library use this same path.

Primitive VM instructions resolve through the native registry. Native failures remain
typed Pima object values in `VmError::Typed`; internal bytecode faults remain
separate host diagnostics. The VM currently lowers numeric primitives and
symbol/error services. Shared filesystem and TCP host operations are available
to native functions as their objects and imports gain VM lowering.

`attempt` records its frame depth, catch instruction, and destination register.
A typed native failure or explicit `throw` unwinds to the nearest record and
places the error in that register. Normal completion removes the record, while
`return`, `break`, and `continue` emit cleanup for any handlers they cross.
Thrown values and object type lists use shared runtime validation. Compiled
instructions carry AST source spans, functions retain their
declared names, and VM errors attach their origin and cross-program call stack
before they are caught or returned as host diagnostics.

Registers are numbered function-local value slots. The initial IR is kept
readable and structurally explicit; compact byte encoding is deferred until the
instruction set and semantics stabilize.

The migration sequence is:

1. literals, lists, primitive calls, immutable locals (implemented);
2. branches, mutation, loops, `break`, and `continue` (implemented);
3. direct user functions and compiled parameter patterns (implemented);
4. immutable closures and mutable captured cells (implemented);
5. shared numeric native dispatch and typed native errors (implemented);
6. `throw`, `attempt`, and frame-aware typed-error unwinding (implemented);
7. direct object construction, custom types, and member reads (implemented);
8. inert blocks, statically known named templates, and `do` (implemented);
9. recursive declaration and assignment destructuring (implemented);
10. `match` expressions, scoped captures, literals, and fallthrough (implemented);
11. indirect cross-module block dispatch and mutable object bindings (implemented);
12. modules and remaining native integration (implemented for file modules,
    standard/native virtual modules, aliases, and selected imports);
13. source-aware bytecode diagnostics and VM call stacks (implemented);
14. optimized call conventions and compact bytecode.

Conformance tests and every shipped example exercise the VM through the public
`Interpreter` API. Lower-level VM tests cover instruction behavior and direct
compiler-to-machine execution.

## 1. Crate layout

The crate is divided by responsibility:

- `source` and `diagnostic` own source text, spans, and rendered errors;
- `syntax` owns tokens, lexing, AST nodes, and parsing;
- `runtime` owns language values, binding metadata, objects, symbols, VM cells, closures, and linked blocks;
- `native` owns host-callable definitions and shared host resources;
- `vm` owns analysis, compilation, IR, native context, and execution;
- `engine` owns the public interpreter, module path resolution, and VM module orchestration;
- `cli` depends only on the public interpreter API.
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
- `runtime` may refer to stable AST block identities used by linked block values.
- `native` operates through a narrow context trait implemented by the engine;
  it does not depend on engine internals, parse source, or control module
  loading directly.
- `engine` coordinates source preparation and module loading; `vm` coordinates
  compilation, calls, objects, native functions, and control transfer.
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

`Value` contains Pima's scalar values, persistent lists, native function IDs,
compiled closures and partial applications, linked blocks, objects, and host
resource handles. VM closures capture traced binding cells, so immutable and
mutable lexical captures share one representation and participate in garbage
collection.

Objects use runtime environments to retain visibility and mutability
metadata. VM-linked binding values make imports aliases of exported storage,
preserving `pub var` writability and `pub val` immutability without duplicating
state. Member cells retain their owning namespaces, keeping the complete object
alive while any extracted reference exists. These ownership cycles participate
in traced garbage collection and are reclaimed when externally unreachable.
Symbols are interned by the VM's native context.

## 7. Engine

`Interpreter` owns source text, parsed modules, module identity resolution, the
compiled-program cache, session globals, and one `Machine`. `run_source` parses
then delegates to `vm_runner`, while `run_prepared` skips reparsing.

The VM runner resolves and initializes file and virtual modules, detects import
cycles, links live exports, compiles the requested module, and executes it. The
machine owns native dispatch, VM frames, attempt handlers, source-aware stack
metadata, and linked function/block execution.

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
value. The VM either unwinds it to the nearest `attempt` handler or returns it
as an uncaught runtime diagnostic. Native functions
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
modules export functions through ordinary immutable module objects so their
behavior at the language level matches other modules.

I/O natives use internally qualified registry names such as `io.join`; the
module loader maps those to public member names such as `join`. This prevents
collisions with identically named operations in other objects. Filesystem
operations resolve relative paths against the interpreter's configured working
directory and translate host failures into portable typed Pima errors.

TCP listeners and connections are opaque Pima values backed by
interpreter-owned socket arenas. Rust owns resource lifetime and operating
system calls only. Framing and application protocols, including the example
HTTP server, remain Pima code.

The evaluator keeps host natives in a private implementation environment used
to construct the standard library. User modules receive only arithmetic and
comparison operators. String, list, type, logic, numeric utility, and console
operations require an explicit standard-library import and qualified access
(or a static object import).

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
6. Objects, visibility, type lists, and `new`.
7. Typed errors, `throw`, `attempt`, and diagnostics.
8. Module lifecycle, imports, and bundled modules.
9. Strings, console output, and native host modules.
10. Standard library and full conformance suite.

Each stage should leave the crate compiling and add focused tests before the
next layer is introduced.
