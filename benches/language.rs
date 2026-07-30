use std::{hint::black_box, time::Duration};

use criterion::{BatchSize, BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use pima::{
    Interpreter,
    source::SourceMap,
    syntax::{lexer::lex, parser::parse},
};

fn generated_bindings(count: usize) -> String {
    let mut source = String::with_capacity(count * 24);
    for index in 0..count {
        source.push_str(&format!("val value_{index} {index}\n"));
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

fn runtime_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("pipeline");

    group.bench_function("fresh_interpreter_arithmetic", |b| {
        b.iter_batched(
            Interpreter::default,
            |mut interpreter| {
                black_box(successful_value(&mut interpreter, "[+ (20 22)]"));
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
                        .prepare_source("<benchmark>", "[+ (20 22)]")
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
        .prepare_source("<benchmark>", "[+ (20 22)]")
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
        whole_capture.run_source("<benchmark-setup>", "function identity :value { value }\n");
    assert!(setup.is_success(), "{:?}", setup.diagnostics);
    let whole_capture_program = whole_capture
        .prepare_source("<benchmark>", "[identity 42]")
        .expect("benchmark source should prepare");
    group.bench_function("prepared_user_call_whole_capture", |b| {
        b.iter(|| {
            let outcome = whole_capture.run_prepared(whole_capture_program);
            assert!(outcome.is_success(), "{:?}", outcome.diagnostics);
            black_box(outcome.value);
        });
    });

    let mut destructuring = Interpreter::default();
    let setup = destructuring.run_source(
        "<benchmark-setup>",
        "function add (:left :right) { + (left right) }\n",
    );
    assert!(setup.is_success(), "{:?}", setup.diagnostics);
    let destructuring_program = destructuring
        .prepare_source("<benchmark>", "[add (20 22)]")
        .expect("benchmark source should prepare");
    group.bench_function("prepared_user_call_list_pattern", |b| {
        b.iter(|| {
            let outcome = destructuring.run_prepared(destructuring_program);
            assert!(outcome.is_success(), "{:?}", outcome.diagnostics);
            black_box(outcome.value);
        });
    });

    let mut recursion = Interpreter::default();
    let setup = recursion.run_source(
        "<benchmark-setup>",
        "function fibonacci (:value) {\n\
             if [< (value 3)] 1 [+ ([fibonacci ([- (value 1)])] [fibonacci ([- (value 2)])])]\n\
         }\n",
    );
    assert!(setup.is_success(), "{:?}", setup.diagnostics);
    let fibonacci_program = recursion
        .prepare_source("<benchmark>", "[fibonacci (15)]")
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
        "function make_adder (:captured) {\n\
             function add (:value) { + (captured value) }\n\
         }\n\
         val add_five [make_adder (5)]\n",
    );
    assert!(setup.is_success(), "{:?}", setup.diagnostics);
    let closure_program = closure
        .prepare_source("<benchmark>", "[add_five (37)]")
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
    let sum_call = format!("[+ ({values})]");
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
                    "function make_cycle () {\n\
                         function recursive () { recursive }\n\
                     }\n",
                );
                assert!(setup.is_success(), "{:?}", setup.diagnostics);
                let program = interpreter
                    .prepare_source("<benchmark>", "[make_cycle ()]")
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
    group.finish();
}

criterion_group! {
    name = benches;
    config = Criterion::default()
        .warm_up_time(Duration::from_secs(1))
        .measurement_time(Duration::from_secs(3))
        .sample_size(30);
    targets = syntax_benchmarks, runtime_benchmarks, memory_benchmarks
}
criterion_main!(benches);
