# Pima

Pima is dual-licensed under the MIT License or Apache License 2.0, at your option.

Pima is an experimental, expression-oriented language implemented with a
register-based virtual machine in Rust. Programs are organized around commands,
values, code blocks, and objects rather than punctuation-heavy call syntax.

The language is under active design. The normative description lives in
[docs/language-reference.md](docs/language-reference.md); implementation
boundaries and invariants are described in
[docs/architecture.md](docs/architecture.md).

## Core mechanics

### Lines are commands

Each physical line begins with a command. If its first value is callable, Pima
invokes it with the remaining values packed into one argument list. Otherwise,
the line simply returns that value.

```pima
import "/pima/library/standard"

Console.println "Hello from Pima"
+ (20 22)
42
```

Square brackets invoke an expression immediately, which is useful when a call
must be nested inside another expression:

```pima
Console.println [+ 20 22]
```

Parentheses create list values; they do not call functions:

```pima
val numbers (1 2 3)
```

### Words and symbols

A bare word resolves its bound value. A leading colon requests the word itself
as a symbol, generally when the surrounding expression would otherwise resolve
it as a name:

```pima
val status :ready
```

Symbols are ordinary values and are commonly used for tags and type names.

### Bindings and mutation

`val` creates an immutable binding, `var` creates a mutable binding, and `let`
updates an existing mutable location:

```pima
val name "Pima"
var count 0
let count [+ count 1]
```

Lists and scalar data have value semantics. Objects, functions, blocks,
futures, and native handles have reference identity when assigned to another
binding.

### Blocks and objects

Braces create an inert code block. `do` executes one in the current context,
while `new` combines one or more blocks into a new object:

```pima
val Counter {
    pub var count 0

    pub function increment () {
        let this.count [+ this.count 1]
        this.count
    }
}

val counter [new Counter]
counter.increment
let counter.count 10
```

With several templates, `new` performs **ordered namespace composition**. It
selects complete definitions with leftmost precedence and then executes the
survivors to create one fresh namespace. Templates are not parent objects:
there is no hidden object chain, inheritance lookup, or `super`, and every
surviving method shares the one completed `this`.

Members are private unless declared with `pub`. A `pub var` is deliberately
readable and writable from outside the object. Inside a method, the reserved
value `this` refers to the object that owns the bound method.

If construction fails, functions and blocks created inside it are invalidated,
including references published through earlier side effects. Using one later
produces `:invalid_object` and retains the original construction error for
diagnostics.

Accessing a function member produces a bound function reference:

```pima
val increment counter.increment
increment
```

Calling `increment` is equivalent to calling `counter.increment`; its `this`
value remains `counter`.

### Errors and concurrency

Errors are typed object values. `throw` raises one and `attempt` returns a
raised error as a value so it can be inspected. Like NaN, an error is never
equal to anything, including itself; use `Types.is?` to identify its type.

Remote objects execute in isolated workers. An annotated block declares the
context it needs and how each value crosses the worker boundary:

```pima
val Worker @(
    configuration
    *workload
    &service
) {
    pub function run () {
        workload
    }
}
```

- `configuration` copies a transportable snapshot.
- `*workload` moves it and invalidates all caller-side references to its shared
  source location.
- `&service` shares a synchronized remote, future, or TCP-listener handle.

A moved location becomes a typed `:moved_value` error that records the source
span and operation responsible for the move.

Local objects, functions, and blocks remain VM-bound; `*` does not implicitly
serialize their reachable graph. Construct worker-local objects inside the
worker from transported data. If an unsendable value is encountered, worker
creation fails transactionally and every caller-side alias remains usable.

## Command line

The unified CLI runs programs and provides development tools:

```console
cargo run -- run examples/fibonacci.pima
cargo run -- check examples
cargo run -- fmt --check examples
cargo run -- doc examples -o target/pima-doc
cargo run -- doc examples/showcase.pima --format markdown
cargo run -- doc examples/showcase.pima --format json
cargo run -- lsp
```

HTML is the default documentation format and produces an indexed static site.
Markdown and JSON are available with `--format`. For compatibility,
`pima file.pima` remains an alias for
`pima run file.pima`. `check`, `fmt`, and `doc` accept files or directories and
discover `.pima` files recursively. The formatter deliberately preserves
physical line boundaries because they are significant Pima syntax.

The interpreter is also available as a Rust library:

```rust
use pima::{Config, Interpreter};

let mut pima = Interpreter::new(Config::default());
let outcome = pima.run_source("<embedded>", "[+ 20 22]\n");

assert!(outcome.is_success());
assert_eq!(outcome.value, Some(pima::Value::Integer(42)));
```

`RunOutcome::diagnostics` contains lexer, parser, and uncaught runtime errors.
An unsuccessful run has no value.

## Complete example

```pima
import "/pima/library/standard"

function factorial (number) {
    if [<= number 1] {
        1
    } {
        * number [factorial [- number 1]]
    }
}

[factorial 6]
```

The ownership-aware context contract for remote objects keeps context
requirements on the object template:

```pima
val Worker @(
    configuration
    *input
    &database
) {
    pub function run () {
        // configuration is an immutable snapshot
        // input is owned exclusively by this worker
        // database is a shared remote handle
    }
}

val configuration (:host "127.0.0.1" :port 8080)
val input [tcp.accept listener]
val database [remote Database]

val worker [remote Worker]
[worker.run]
```

`configuration`, `input`, and `database` must already be visible where
`remote Worker` is evaluated. A bare requirement copies a snapshot, `*`
transfers ownership and replaces the shared source location (including its
reference aliases) with a provenance-carrying moved-value error, and `&`
passes a synchronized remote, future, or TCP-listener handle.

More complete programs live in `examples/`, including a JSON parser and a
directory-backed static file server core.

## Performance

Run the Criterion benchmark suite with:

```console
cargo bench
```

The suite measures syntax processing, interpreter startup, repeated evaluation,
recursion, closures, and larger list workloads. See
[docs/benchmarking.md](docs/benchmarking.md) for individual commands and
comparison guidance.

The TCP module can run the Pima HTTP implementation as a real local server:

```console
cargo run -- run demos/http_file_server.pima
```

Then open `http://127.0.0.1:8080` or request it with `curl`.
The demo starts four isolated accept workers sharing one listener and four
remote file handlers, allowing independent requests to make progress in
parallel.

## Development

Use the same checks required by the repository:

```console
cargo test --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

The current implementation includes lexing, parsing, evaluation, closures,
immutable lists, objects, typed errors, source-aware stack diagnostics, the
embedded standard library, `/pima/io`, low-level `/pima/tcp` sockets, and
cached file imports with complete cycle reporting. The remaining
standard-library and host-embedding surfaces are still evolving.
