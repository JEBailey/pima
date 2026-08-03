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
| `evaluation/prepared_native_arithmetic` | Prepared VM execution and native dispatch |
| `evaluation/user_call/scalar_to_whole_capture` | Baseline user dispatch with no argument list |
| `evaluation/user_call/list_to_whole_capture` | Same dispatch plus evaluating a one-element argument list |
| `evaluation/user_call/list_to_list_pattern` | Same list argument plus destructuring and capture binding |
| `evaluation/prepared_recursive_fibonacci_15` | User calls, pattern binding, scopes, and recursion |
| `evaluation/prepared_captured_closure_call` | Closure invocation and captured-environment lookup |
| `evaluation/prepared_sum_1000_integers` | List evaluation, allocation, and native dispatch |
| `memory/create_100_recursive_closures_and_collect` | VM closure/cell cycle allocation and tracing collection |
| `memory/vm_create_100_closure_cell_cycles_and_collect` | Direct-machine closure/cell cycle allocation and tracing collection |
| `engine_comparison/interpreter_vm/primitive_add` | Prepared execution through the public interpreter API |
| `engine_comparison/register_vm/primitive_add` | Compiled register-VM execution of the same expression |
| `engine_comparison/interpreter_vm/branch` | Prepared interpreter condition and branch |
| `engine_comparison/register_vm/branch` | Register-VM execution of the same branch |
| `engine_comparison/*/scalar_capture_call` | Direct user call with a whole-value capture |
| `engine_comparison/*/list_pattern_call` | Direct user call with compiled list destructuring |
| `engine_comparison/*/fibonacci_15` | Recursive user calls and function frames |
| `engine_comparison/*/captured_closure_call` | Dynamic call through an immutable lexical closure |
| `engine_comparison/*/mutable_closure_roundtrip` | Create a counter closure and mutate its captured cell twice |
| `engine_comparison/interpreter_vm/sum_loop_1000` | Interpreter mutation and 1,000 loop iterations |
| `engine_comparison/register_vm/sum_loop_1000` | Register-VM execution of the same loop |
| `engine_comparison/*/attempt_caught_error` | Construct, throw, unwind, and catch a numeric error |
| `engine_comparison/*/object_construct` | Construct an object with one public immutable member |
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
# VM optimization comparisons

Before changing VM representations, run the complete workspace test suite and
save a Criterion baseline. Afterward, rerun the same tests and compare focused
register-VM groups. In particular:

- `primitive_add` and `branch` measure dispatch overhead.
- `scalar_capture_call`, `list_pattern_call`, and `fibonacci_15` measure call
  argument and frame costs.
- `prepared_sum_1000_integers` measures large argument-pack construction.
- `object_construct` and member-oriented programs measure namespace storage
  and lookup behavior.

Treat overlapping confidence intervals as inconclusive rather than reporting a
speedup. A semantic test failure invalidates the performance result.

The `sustained_vm` group supplies longer-running comparison workloads for
arithmetic loops, user calls, member access, and mutable closures. Save a named
baseline before changing the VM:

```console
cargo bench --bench language -- sustained_vm --save-baseline before-change
```

`Machine::set_metrics_enabled(true)` enables deterministic work counters. Use
these in tests to prove that an optimization removes dispatches, allocations,
or materializations; leave metrics disabled for wall-clock measurements.

The `before-vm-1-6` comparison currently shows direct user calls about 2.7%
faster, with integer loops and mutable closure calls statistically unchanged.
Shrinking executable instructions from 88 to 48 bytes improved code density.
The subsequent fixed-namespace member instruction brought the sustained member
workload back to its original baseline (9.96 ms versus 9.95 ms, statistically
unchanged), removing the earlier regression but not establishing a speedup.
