use std::{hint::black_box, time::Duration};

use criterion::{BatchSize, BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use pima::{
    Interpreter,
    source::SourceMap,
    syntax::{lexer::lex, parser::parse},
    vm::{Machine, compile},
};

fn generated_bindings(count: usize) -> String {
    let mut source = String::with_capacity(count * 24);
    for index in 0..count {
        source.push_str(&format!("val :value_{index} {index}\n"));
    }
    source
}

fn syntax_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("syntax");
    for count in [100, 1_000] {
        let source = generated_bindings(count);
        let mut sources = SourceMap::default();
        let source_id = sources.add("<benchmark>", source.as_str());
        let tokens = lex(source_id, &source).expect("benchmark source should lex");

        group.throughput(Throughput::Bytes(source.len() as u64));
        group.bench_with_input(
            BenchmarkId::new("lex_bindings", count),
            &source,
            |b, source| {
                b.iter(|| {
                    black_box(
                        lex(source_id, black_box(source.as_str()))
                            .expect("benchmark source should lex"),
                    );
                });
            },
        );
        group.bench_with_input(
            BenchmarkId::new("parse_prelexed_bindings", count),
            &tokens,
            |b, tokens| {
                b.iter(|| {
                    black_box(parse(black_box(tokens)).expect("benchmark source should parse"));
                });
            },
        );
        group.bench_with_input(
            BenchmarkId::new("lex_and_parse_bindings", count),
            &source,
            |b, source| {
                b.iter(|| {
                    let mut sources = SourceMap::default();
                    let source_id = sources.add("<benchmark>", black_box(source.as_str()));
                    let tokens = lex(source_id, source).expect("benchmark source should lex");
                    black_box(parse(&tokens).expect("benchmark source should parse"));
                });
            },
        );
    }
    group.finish();
}

fn successful_value(interpreter: &mut Interpreter, source: &str) -> pima::Value {
    let outcome = interpreter.run_source("<benchmark>", source);
    assert!(outcome.is_success(), "{:?}", outcome.diagnostics);
    outcome.value.expect("benchmark should return a value")
}

fn compile_vm_source(source: &str) -> pima::vm::Program {
    let mut sources = SourceMap::default();
    let source_id = sources.add("<benchmark>", source);
    let tokens = lex(source_id, source).expect("benchmark source should lex");
    let module = parse(&tokens).expect("benchmark source should parse");
    compile(&module).expect("benchmark source should compile")
}

fn runtime_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("pipeline");

    group.bench_function("fresh_interpreter_arithmetic", |b| {
        b.iter_batched(
            Interpreter::default,
            |mut interpreter| {
                black_box(successful_value(&mut interpreter, "[+ 20 22]"));
            },
            BatchSize::SmallInput,
        );
    });

    group.bench_function("prepare_arithmetic", |b| {
        b.iter_batched(
            Interpreter::default,
            |mut interpreter| {
                black_box(
                    interpreter
                        .prepare_source("<benchmark>", "[+ 20 22]")
                        .expect("benchmark source should prepare"),
                );
            },
            BatchSize::SmallInput,
        );
    });
    group.finish();

    let mut group = c.benchmark_group("evaluation");

    let mut arithmetic = Interpreter::default();
    let arithmetic_program = arithmetic
        .prepare_source("<benchmark>", "[+ 20 22]")
        .expect("benchmark source should prepare");
    group.bench_function("prepared_native_arithmetic", |b| {
        b.iter(|| {
            let outcome = arithmetic.run_prepared(arithmetic_program);
            assert!(outcome.is_success(), "{:?}", outcome.diagnostics);
            black_box(outcome.value);
        });
    });

    let mut whole_capture = Interpreter::default();
    let setup =
        whole_capture.run_source("<benchmark-setup>", "function :identity value { value }\n");
    assert!(setup.is_success(), "{:?}", setup.diagnostics);
    let whole_capture_program = whole_capture
        .prepare_source("<benchmark>", "[identity 42]")
        .expect("benchmark source should prepare");
    group.bench_function("user_call/scalar_to_whole_capture", |b| {
        b.iter(|| {
            let outcome = whole_capture.run_prepared(whole_capture_program);
            assert!(outcome.is_success(), "{:?}", outcome.diagnostics);
            black_box(outcome.value);
        });
    });

    let whole_list_capture_program = whole_capture
        .prepare_source("<benchmark>", "[identity 42]")
        .expect("benchmark source should prepare");
    group.bench_function("user_call/list_to_whole_capture", |b| {
        b.iter(|| {
            let outcome = whole_capture.run_prepared(whole_list_capture_program);
            assert!(outcome.is_success(), "{:?}", outcome.diagnostics);
            black_box(outcome.value);
        });
    });

    let mut destructuring = Interpreter::default();
    let setup = destructuring.run_source(
        "<benchmark-setup>",
        "function :add (left right) { + left right }\n",
    );
    assert!(setup.is_success(), "{:?}", setup.diagnostics);
    let destructuring_program = destructuring
        .prepare_source("<benchmark>", "[add 20 22]")
        .expect("benchmark source should prepare");
    group.bench_function("user_call/list_to_list_pattern", |b| {
        b.iter(|| {
            let outcome = destructuring.run_prepared(destructuring_program);
            assert!(outcome.is_success(), "{:?}", outcome.diagnostics);
            black_box(outcome.value);
        });
    });

    let mut recursion = Interpreter::default();
    let setup = recursion.run_source(
        "<benchmark-setup>",
        "function :fibonacci (value) {\n\
             if [< value 3] 1 [+ [fibonacci [- value 1]] [fibonacci [- value 2]]]\n\
         }\n",
    );
    assert!(setup.is_success(), "{:?}", setup.diagnostics);
    let fibonacci_program = recursion
        .prepare_source("<benchmark>", "[fibonacci 15]")
        .expect("benchmark source should prepare");
    group.bench_function("prepared_recursive_fibonacci_15", |b| {
        b.iter(|| {
            let outcome = recursion.run_prepared(fibonacci_program);
            assert!(outcome.is_success(), "{:?}", outcome.diagnostics);
            black_box(outcome.value);
        });
    });

    let mut closure = Interpreter::default();
    let setup = closure.run_source(
        "<benchmark-setup>",
        "function :make_adder (captured) {\n\
             function :add (value) { + captured value }\n\
         }\n\
         val :add_five [make_adder 5]\n",
    );
    assert!(setup.is_success(), "{:?}", setup.diagnostics);
    let closure_program = closure
        .prepare_source("<benchmark>", "[add_five 37]")
        .expect("benchmark source should prepare");
    group.bench_function("prepared_captured_closure_call", |b| {
        b.iter(|| {
            let outcome = closure.run_prepared(closure_program);
            assert!(outcome.is_success(), "{:?}", outcome.diagnostics);
            black_box(outcome.value);
        });
    });

    let values = (0..1_000)
        .map(|value| value.to_string())
        .collect::<Vec<_>>()
        .join(" ");
    let sum_call = format!("[+ {values}]");
    let mut collection = Interpreter::default();
    let collection_program = collection
        .prepare_source("<benchmark>", &sum_call)
        .expect("benchmark source should prepare");
    group.throughput(Throughput::Elements(1_000));
    group.bench_function("prepared_sum_1000_integers", |b| {
        b.iter(|| {
            let outcome = collection.run_prepared(collection_program);
            assert!(outcome.is_success(), "{:?}", outcome.diagnostics);
            black_box(outcome.value);
        });
    });

    group.finish();
}

fn memory_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("memory");
    group.throughput(Throughput::Elements(100));
    group.bench_function("create_100_recursive_closures_and_collect", |b| {
        b.iter_batched(
            || {
                let mut interpreter = Interpreter::default();
                let setup = interpreter.run_source(
                    "<benchmark-setup>",
                    "function :make_cycle () {\n\
                         function :recursive () { recursive }\n\
                     }\n",
                );
                assert!(setup.is_success(), "{:?}", setup.diagnostics);
                let program = interpreter
                    .prepare_source("<benchmark>", "[make_cycle]")
                    .expect("benchmark source should prepare");
                (interpreter, program)
            },
            |(mut interpreter, program)| {
                for _ in 0..100 {
                    let outcome = interpreter.run_prepared(program);
                    assert!(outcome.is_success(), "{:?}", outcome.diagnostics);
                    black_box(outcome.value);
                }
                dumpster::unsync::collect();
            },
            BatchSize::SmallInput,
        );
    });

    let vm_cycle_program = compile_vm_source(
        "function :build_cycle () {\n\
             var :captured ()\n\
             function :cycle () { captured }\n\
             let :captured cycle\n\
             42\n\
         }\n\
         [build_cycle]",
    );
    let mut machine = Machine::default();
    group.bench_function("vm_create_100_closure_cell_cycles_and_collect", |b| {
        b.iter(|| {
            for _ in 0..100 {
                black_box(
                    machine
                        .execute(&vm_cycle_program)
                        .expect("benchmark program should execute"),
                );
            }
            dumpster::unsync::collect();
        });
    });
    group.finish();
}

fn engine_comparison_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("engine_comparison");
    let source = "+ 20 22";

    let mut interpreter = Interpreter::default();
    let interpreter_program = interpreter
        .prepare_source("<benchmark>", source)
        .expect("benchmark source should prepare");
    group.bench_function("interpreter_vm/primitive_add", |b| {
        b.iter(|| {
            let outcome = interpreter.run_prepared(interpreter_program);
            assert!(outcome.is_success(), "{:?}", outcome.diagnostics);
            black_box(outcome.value);
        });
    });

    let mut sources = SourceMap::default();
    let source_id = sources.add("<benchmark>", source);
    let tokens = lex(source_id, source).expect("benchmark source should lex");
    let module = parse(&tokens).expect("benchmark source should parse");
    let program = compile(&module).expect("benchmark source should compile");
    let mut machine = Machine::default();
    group.bench_function("register_vm/primitive_add", |b| {
        b.iter(|| {
            black_box(
                machine
                    .execute(&program)
                    .expect("benchmark program should execute"),
            );
        });
    });

    let branch_source = "if [< 20 22] 1 0";
    let mut interpreter_branch = Interpreter::default();
    let interpreter_branch_program = interpreter_branch
        .prepare_source("<benchmark>", branch_source)
        .expect("benchmark source should prepare");
    group.bench_function("interpreter_vm/branch", |b| {
        b.iter(|| {
            let outcome = interpreter_branch.run_prepared(interpreter_branch_program);
            assert!(outcome.is_success(), "{:?}", outcome.diagnostics);
            black_box(outcome.value);
        });
    });
    let mut branch_sources = SourceMap::default();
    let branch_source_id = branch_sources.add("<benchmark>", branch_source);
    let branch_tokens = lex(branch_source_id, branch_source).expect("benchmark source should lex");
    let branch_module = parse(&branch_tokens).expect("benchmark source should parse");
    let branch_program = compile(&branch_module).expect("benchmark source should compile");
    group.bench_function("register_vm/branch", |b| {
        b.iter(|| {
            black_box(
                machine
                    .execute(&branch_program)
                    .expect("benchmark program should execute"),
            );
        });
    });

    let mut interpreter_scalar_call = Interpreter::default();
    let setup = interpreter_scalar_call
        .run_source("<benchmark-setup>", "function :identity value { value }\n");
    assert!(setup.is_success(), "{:?}", setup.diagnostics);
    let interpreter_scalar_program = interpreter_scalar_call
        .prepare_source("<benchmark>", "[identity 42]")
        .expect("benchmark source should prepare");
    group.bench_function("interpreter_vm/scalar_capture_call", |b| {
        b.iter(|| {
            let outcome = interpreter_scalar_call.run_prepared(interpreter_scalar_program);
            assert!(outcome.is_success(), "{:?}", outcome.diagnostics);
            black_box(outcome.value);
        });
    });
    let vm_scalar_program = compile_vm_source("function :identity value { value }\n[identity 42]");
    group.bench_function("register_vm/scalar_capture_call", |b| {
        b.iter(|| {
            black_box(
                machine
                    .execute(&vm_scalar_program)
                    .expect("benchmark program should execute"),
            );
        });
    });

    let mut interpreter_list_call = Interpreter::default();
    let setup = interpreter_list_call.run_source(
        "<benchmark-setup>",
        "function :add (left right) { + left right }\n",
    );
    assert!(setup.is_success(), "{:?}", setup.diagnostics);
    let interpreter_list_program = interpreter_list_call
        .prepare_source("<benchmark>", "[add 20 22]")
        .expect("benchmark source should prepare");
    group.bench_function("interpreter_vm/list_pattern_call", |b| {
        b.iter(|| {
            let outcome = interpreter_list_call.run_prepared(interpreter_list_program);
            assert!(outcome.is_success(), "{:?}", outcome.diagnostics);
            black_box(outcome.value);
        });
    });
    let vm_list_program =
        compile_vm_source("function :add (left right) { + left right }\n[add 20 22]");
    group.bench_function("register_vm/list_pattern_call", |b| {
        b.iter(|| {
            black_box(
                machine
                    .execute(&vm_list_program)
                    .expect("benchmark program should execute"),
            );
        });
    });

    let fibonacci_declaration = "function :fibonacci (value) {\n\
         if [< value 3] 1 [+ [fibonacci [- value 1]] [fibonacci [- value 2]]]\n\
     }\n";
    let mut interpreter_fibonacci = Interpreter::default();
    let setup = interpreter_fibonacci.run_source("<benchmark-setup>", fibonacci_declaration);
    assert!(setup.is_success(), "{:?}", setup.diagnostics);
    let interpreter_fibonacci_program = interpreter_fibonacci
        .prepare_source("<benchmark>", "[fibonacci 15]")
        .expect("benchmark source should prepare");
    group.bench_function("interpreter_vm/fibonacci_15", |b| {
        b.iter(|| {
            let outcome = interpreter_fibonacci.run_prepared(interpreter_fibonacci_program);
            assert!(outcome.is_success(), "{:?}", outcome.diagnostics);
            black_box(outcome.value);
        });
    });
    let vm_fibonacci_program = compile_vm_source(&format!("{fibonacci_declaration}[fibonacci 15]"));
    group.bench_function("register_vm/fibonacci_15", |b| {
        b.iter(|| {
            black_box(
                machine
                    .execute(&vm_fibonacci_program)
                    .expect("benchmark program should execute"),
            );
        });
    });

    let closure_source = "function :make_adder (captured) {\n\
         function :add (value) { + captured value }\n\
         add\n\
     }\n\
     val :add_five [make_adder 5]\n";
    let mut interpreter_closure = Interpreter::default();
    let setup = interpreter_closure.run_source("<benchmark-setup>", closure_source);
    assert!(setup.is_success(), "{:?}", setup.diagnostics);
    let interpreter_closure_program = interpreter_closure
        .prepare_source("<benchmark>", "[add_five 37]")
        .expect("benchmark source should prepare");
    group.bench_function("interpreter_vm/captured_closure_call", |b| {
        b.iter(|| {
            let outcome = interpreter_closure.run_prepared(interpreter_closure_program);
            assert!(outcome.is_success(), "{:?}", outcome.diagnostics);
            black_box(outcome.value);
        });
    });
    let vm_closure_program = compile_vm_source(&format!("{closure_source}[add_five 37]"));
    group.bench_function("register_vm/captured_closure_call", |b| {
        b.iter(|| {
            black_box(
                machine
                    .execute(&vm_closure_program)
                    .expect("benchmark program should execute"),
            );
        });
    });

    let mutable_closure_source = "function :counter (start) {\n\
         var :value start\n\
         function :next () {\n\
             let :value [+ value 1]\n\
             value\n\
         }\n\
         next\n\
     }\n\
     val :instance [counter 0]\n\
     [instance]\n\
     [instance]";
    group.bench_function("interpreter_vm/mutable_closure_roundtrip", |b| {
        b.iter_batched(
            || {
                let mut interpreter = Interpreter::default();
                let program = interpreter
                    .prepare_source("<benchmark>", mutable_closure_source)
                    .expect("benchmark source should prepare");
                (interpreter, program)
            },
            |(mut interpreter, program)| {
                let outcome = interpreter.run_prepared(program);
                assert!(outcome.is_success(), "{:?}", outcome.diagnostics);
                black_box(outcome.value);
            },
            BatchSize::SmallInput,
        );
    });
    let vm_mutable_closure_program = compile_vm_source(mutable_closure_source);
    group.bench_function("register_vm/mutable_closure_roundtrip", |b| {
        b.iter(|| {
            black_box(
                machine
                    .execute(&vm_mutable_closure_program)
                    .expect("benchmark program should execute"),
            );
        });
    });

    let loop_source = "var :index 0\n\
                       var :total 0\n\
                       while [< index 1000] {\n\
                           let :index [+ index 1]\n\
                           let :total [+ total index]\n\
                       }\n\
                       total";
    group.throughput(Throughput::Elements(1_000));
    group.bench_function("interpreter_vm/sum_loop_1000", |b| {
        b.iter_batched(
            || {
                let mut interpreter = Interpreter::default();
                let program = interpreter
                    .prepare_source("<benchmark>", loop_source)
                    .expect("benchmark source should prepare");
                (interpreter, program)
            },
            |(mut interpreter, program)| {
                let outcome = interpreter.run_prepared(program);
                assert!(outcome.is_success(), "{:?}", outcome.diagnostics);
                black_box(outcome.value);
            },
            BatchSize::SmallInput,
        );
    });
    let mut loop_sources = SourceMap::default();
    let loop_source_id = loop_sources.add("<benchmark>", loop_source);
    let loop_tokens = lex(loop_source_id, loop_source).expect("benchmark source should lex");
    let loop_module = parse(&loop_tokens).expect("benchmark source should parse");
    let loop_program = compile(&loop_module).expect("benchmark source should compile");
    group.bench_function("register_vm/sum_loop_1000", |b| {
        b.iter(|| {
            black_box(
                machine
                    .execute(&loop_program)
                    .expect("benchmark program should execute"),
            );
        });
    });

    let attempt_source = "attempt { / 1 0 }";
    group.throughput(Throughput::Elements(1));
    let mut interpreter_attempt = Interpreter::default();
    let interpreter_attempt_program = interpreter_attempt
        .prepare_source("<benchmark>", attempt_source)
        .expect("benchmark source should prepare");
    group.bench_function("interpreter_vm/attempt_caught_error", |b| {
        b.iter(|| {
            let outcome = interpreter_attempt.run_prepared(interpreter_attempt_program);
            assert!(outcome.is_success(), "{:?}", outcome.diagnostics);
            black_box(outcome.value);
        });
    });
    let attempt_program = compile_vm_source(attempt_source);
    group.bench_function("register_vm/attempt_caught_error", |b| {
        b.iter(|| {
            black_box(
                machine
                    .execute(&attempt_program)
                    .expect("attempt should catch the benchmark error"),
            );
        });
    });

    let namespace_source = "new { pub val :answer 42 }";
    let mut interpreter_namespace = Interpreter::default();
    let interpreter_namespace_program = interpreter_namespace
        .prepare_source("<benchmark>", namespace_source)
        .expect("benchmark source should prepare");
    group.bench_function("interpreter_vm/object_construct", |b| {
        b.iter(|| {
            let outcome = interpreter_namespace.run_prepared(interpreter_namespace_program);
            assert!(outcome.is_success(), "{:?}", outcome.diagnostics);
            black_box(outcome.value);
        });
    });
    let namespace_program = compile_vm_source(namespace_source);
    group.bench_function("register_vm/object_construct", |b| {
        b.iter(|| {
            black_box(
                machine
                    .execute(&namespace_program)
                    .expect("object benchmark should execute"),
            );
        });
    });

    let do_source = "do { + 20 22 }";
    let mut interpreter_do = Interpreter::default();
    let interpreter_do_program = interpreter_do
        .prepare_source("<benchmark>", do_source)
        .expect("benchmark source should prepare");
    group.bench_function("interpreter_vm/do_block", |b| {
        b.iter(|| {
            let outcome = interpreter_do.run_prepared(interpreter_do_program);
            assert!(outcome.is_success(), "{:?}", outcome.diagnostics);
            black_box(outcome.value);
        });
    });
    let do_program = compile_vm_source(do_source);
    group.bench_function("register_vm/do_block", |b| {
        b.iter(|| {
            black_box(
                machine
                    .execute(&do_program)
                    .expect("do benchmark should execute"),
            );
        });
    });

    let match_source = "match (:ok 42) ( (:ok value) { + value 1 } _ { 0 } )";
    let mut interpreter_match = Interpreter::default();
    let interpreter_match_program = interpreter_match
        .prepare_source("<benchmark>", match_source)
        .expect("benchmark source should prepare");
    group.bench_function("interpreter_vm/match_nested", |b| {
        b.iter(|| {
            let outcome = interpreter_match.run_prepared(interpreter_match_program);
            assert!(outcome.is_success(), "{:?}", outcome.diagnostics);
            black_box(outcome.value);
        });
    });
    let match_program = compile_vm_source(match_source);
    group.bench_function("register_vm/match_nested", |b| {
        b.iter(|| {
            black_box(
                machine
                    .execute(&match_program)
                    .expect("match benchmark should execute"),
            );
        });
    });

    group.finish();
}

criterion_group! {
    name = benches;
    config = Criterion::default()
        .warm_up_time(Duration::from_secs(1))
        .measurement_time(Duration::from_secs(3))
        .sample_size(30);
    targets =
        syntax_benchmarks,
        runtime_benchmarks,
        memory_benchmarks,
        engine_comparison_benchmarks
}
criterion_main!(benches);
