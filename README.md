# Pima

Pima is dual-licensed under the MIT License or Apache License 2.0, at your option.

Pima is an experimental, expression-oriented language implemented with a
register-based virtual machine in Rust. Its central ideas are:

- physical line endings terminate statements;
- square brackets invoke immediately;
- braces produce uninstantiated code blocks;
- `@(:name...) { ... }` declares a block's required execution context;
- `do` executes a block in the current environment;
- bindings are immutable unless declared with `var`;
- namespaces are private by default and expose members with `pub`;
- lists are immutable persistent values; and
- errors are typed namespace values handled with `throw` and `attempt`.

The language is under active design. The normative description lives in
[docs/language-reference.md](docs/language-reference.md); implementation
boundaries and invariants are described in
[docs/architecture.md](docs/architecture.md).

## Running Pima

The CLI accepts one source file:

```console
cargo run -- examples/fibonacci.pima
```

The interpreter is also available as a Rust library:

```rust
use pima::{Config, Interpreter};

let mut pima = Interpreter::new(Config::default());
let outcome = pima.run_source("<embedded>", "[+ (20 22)]\n");

assert!(outcome.is_success());
assert_eq!(outcome.value, Some(pima::Value::Integer(42)));
```

`RunOutcome::diagnostics` contains lexer, parser, and uncaught runtime errors.
An unsuccessful run has no value.

## Example

```pima
import "/pima/library/standard"

function factorial (:number) {
    if [<= (number 1)] {
        1
    } {
        * (number [factorial ([- (number 1)])])
    }
}

[factorial (6)]
```

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
cargo run -- demos/http_file_server.pima
```

Then open `http://127.0.0.1:8080` or request it with `curl`.

## Development

Use the same checks required by the repository:

```console
cargo test --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

The current implementation includes lexing, parsing, evaluation, closures,
immutable lists, namespaces, typed errors, source-aware stack diagnostics, the
embedded standard library, `/pima/io`, low-level `/pima/tcp` sockets, and
cached file imports with complete cycle reporting. The remaining
standard-library and host-embedding surfaces are still evolving.
