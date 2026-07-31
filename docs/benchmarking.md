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
| `evaluation/user_call/scalar_to_whole_capture` | Baseline user dispatch with no argument list |
| `evaluation/user_call/list_to_whole_capture` | Same dispatch plus evaluating a one-element argument list |
| `evaluation/user_call/list_to_list_pattern` | Same list argument plus destructuring and capture binding |
| `evaluation/prepared_recursive_fibonacci_15` | User calls, pattern binding, scopes, and recursion |
| `evaluation/prepared_captured_closure_call` | Closure invocation and captured-environment lookup |
| `evaluation/prepared_sum_1000_integers` | List evaluation, allocation, and native dispatch |
| `memory/create_100_recursive_closures_and_collect` | Cyclic environment allocation and explicit garbage collection |
| `memory/vm_create_100_closure_cell_cycles_and_collect` | VM closure/cell cycle allocation and tracing collection |
| `engine_comparison/tree_walk/primitive_add` | Prepared tree-walk primitive evaluation |
| `engine_comparison/register_vm/primitive_add` | Compiled register-VM execution of the same expression |
| `engine_comparison/tree_walk/branch` | Prepared tree-walk condition and branch |
| `engine_comparison/register_vm/branch` | Register-VM execution of the same branch |
| `engine_comparison/*/scalar_capture_call` | Direct user call with a whole-value capture |
| `engine_comparison/*/list_pattern_call` | Direct user call with compiled list destructuring |
| `engine_comparison/*/fibonacci_15` | Recursive user calls and function frames |
| `engine_comparison/*/captured_closure_call` | Dynamic call through an immutable lexical closure |
| `engine_comparison/*/mutable_closure_roundtrip` | Create a counter closure and mutate its captured cell twice |
| `engine_comparison/tree_walk/sum_loop_1000` | Tree-walk mutation and 1,000 loop iterations |
| `engine_comparison/register_vm/sum_loop_1000` | Register-VM execution of the same loop |
| `engine_comparison/*/attempt_caught_error` | Construct, throw, unwind, and catch a numeric error |
| `engine_comparison/*/namespace_construct` | Construct a namespace with one public immutable member |
| `engine_comparison/*/do_block` | Instantiate and execute a block in the current environment |
| `engine_comparison/*/match_nested` | Match a tagged list, bind its payload, and execute the selected arm |

The prepared workloads use `Interpreter::prepare_source` once and then measure
only `Interpreter::run_prepared`. This keeps lexer and parser costs out of
runtime measurements.

The three `evaluation/user_call` cases are intentionally matched. Subtract
`scalar_to_whole_capture` from `list_to_whole_capture` to estimate argument-list
construction. Subtract `list_to_whole_capture` from `list_to_list_pattern` to
estimate list-pattern matching and destructuring. Treat these differences as
diagnostic estimates rather than independent timings because each benchmark
still measures a complete call.

Compare results only when using the same machine, power profile, Rust toolchain,
and benchmark command. Close CPU-heavy applications and avoid comparing debug
builds; `cargo bench` uses the optimized benchmark profile.

Before optimizing, record which benchmark is expected to improve and which
should remain unchanged. Always run the correctness suite as well:

```console
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```
