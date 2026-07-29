# Pima

Pima is an experimental, expression-oriented language implemented as a
tree-walk interpreter in Rust. Its central ideas are:

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
let outcome = pima.run_source("<embedded>", "[+ 20 22]\n");

assert!(outcome.is_success());
assert_eq!(outcome.value, Some(pima::Value::Integer(42)));
```

`RunOutcome::diagnostics` contains lexer, parser, and uncaught runtime errors.
An unsuccessful run has no value.

## Example

```pima
import "/pima/library/standard"

function factorial (:number) {
    if [<= number 1] {
        1
    } {
        * number [factorial [- number 1]]
    }
}

[factorial 6]
```

## Development

Use the same checks required by the repository:

```console
cargo test --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

The current implementation includes lexing, parsing, evaluation, closures,
immutable lists, namespaces, typed errors, source-aware stack diagnostics, the
embedded standard library, `/pima/io`, and cached file imports with complete
cycle reporting. The remaining standard-library and host-embedding surfaces
are still evolving.
