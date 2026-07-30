# Benchmarking Pima

Pima uses Criterion for repeatable local performance measurements. Run the
complete suite in an otherwise idle terminal:

```console
cargo bench
```

Run one group or workload while developing:

```console
cargo bench --bench language -- syntax
cargo bench --bench language -- evaluation/prepared_recursive_fibonacci_15
```

Criterion stores measurements under `target/criterion`. A later run compares
against the preceding measurements and reports whether the observed change is
statistically significant.

The suite separates several costs:

| Group/workload | Primary cost represented |
|---|---|
| `syntax/lex_bindings` | Lexer and token allocation |
| `syntax/parse_prelexed_bindings` | Parser and AST allocation with lexing removed |
| `syntax/lex_and_parse_bindings` | Complete syntax pipeline and source scaling |
| `pipeline/fresh_interpreter_arithmetic` | Interpreter construction, parsing, and evaluation |
| `pipeline/prepare_arithmetic` | Source storage, lexing, parsing, and module storage |
| `evaluation/prepared_native_arithmetic` | AST evaluation and native dispatch |
| `evaluation/prepared_user_call_whole_capture` | User dispatch with a scalar capture pattern |
| `evaluation/prepared_user_call_list_pattern` | User dispatch, list-pattern binding, and scope creation |
| `evaluation/prepared_recursive_fibonacci_15` | User calls, pattern binding, scopes, and recursion |
| `evaluation/prepared_captured_closure_call` | Closure invocation and captured-environment lookup |
| `evaluation/prepared_sum_1000_integers` | List evaluation, allocation, and native dispatch |
| `memory/create_100_recursive_closures_and_collect` | Cyclic environment allocation and explicit garbage collection |

The prepared workloads use `Interpreter::prepare_source` once and then measure
only `Interpreter::run_prepared`. This keeps lexer and parser costs out of
runtime measurements.

Compare results only when using the same machine, power profile, Rust toolchain,
and benchmark command. Close CPU-heavy applications and avoid comparing debug
builds; `cargo bench` uses the optimized benchmark profile.

Before optimizing, record which benchmark is expected to improve and which
should remain unchanged. Always run the correctness suite as well:

```console
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```
