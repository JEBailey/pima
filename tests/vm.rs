use pima::{
    Interpreter, Value,
    source::SourceMap,
    syntax::{lexer::lex, parser::parse},
    vm::{Machine, compile},
};

fn run_vm(source: &str) -> Value {
    let program = compile_vm(source);
    let mut machine = Machine::default();
    match machine.execute(&program) {
        Ok(value) => value,
        Err(pima::vm::VmError::Typed(Value::Namespace(namespace))) => {
            let message = namespace
                .environment
                .borrow()
                .bindings
                .iter()
                .find(|(symbol, _)| machine.resolve_symbol(**symbol) == Some("message"))
                .map(|(_, binding)| format!("{:?}", binding.value))
                .unwrap_or_else(|| "<missing message>".into());
            panic!("program should execute: {source}\n{message}");
        }
        Err(error) => panic!("program should execute: {source}\n{error:?}"),
    }
}

fn compile_vm(source: &str) -> pima::vm::Program {
    let mut sources = SourceMap::default();
    let source_id = sources.add("<vm-test>", source);
    let tokens = lex(source_id, source).expect("source should lex");
    let module = parse(&tokens).expect("source should parse");
    compile(&module).expect("source should compile")
}

fn run_vm_with_standard_globals(source: &str) -> Value {
    let mut sources = SourceMap::default();
    let source_id = sources.add("<vm-native-test>", source);
    let tokens = lex(source_id, source).expect("source should lex");
    let module = parse(&tokens).expect("source should parse");
    let mut machine = Machine::default();
    let globals = machine.standard_globals();
    let program =
        pima::vm::compile_module_with_globals(&module, 0, globals).expect("source should compile");
    machine.execute(&program).expect("source should execute")
}

fn value_type_names(machine: &Machine, value: &Value) -> Vec<String> {
    let Value::Namespace(namespace) = value else {
        panic!("typed value should be a namespace");
    };
    namespace
        .types
        .iter()
        .map(|symbol| machine.resolve_symbol(*symbol).unwrap().to_owned())
        .collect()
}

fn run_interpreter(source: &str) -> Value {
    let mut interpreter = Interpreter::default();
    let outcome = interpreter.run_source("<interpreter-test>", source);
    assert!(outcome.is_success(), "{:?}", outcome.diagnostics);
    outcome.value.expect("program should return a value")
}

#[test]
fn public_interpreter_matches_direct_vm_execution() {
    for source in [
        "42",
        "val value 42",
        "val left 20\nval right 22\n+ (left right)",
        "val (left (middle right)) (10 (20 12))\n+ (left middle right)",
        "var (left right) (20 22)\nlet left 10\n+ (left right)",
        "var left 0\nvar right 0\nlet (left right) (20 22)\n+ (left right)",
        "var a 0\nvar b 0\nvar c 0\nval failure [attempt {\n    let ((a b) (c _)) ((1 2) (3))\n}]\n(a b c)",
        "val result (:ok 42)\nmatch result (\n    (ok :value) { + (value 1) }\n    (error :error) { error }\n)",
        "match (:point (3 4)) (\n    (point (3 :y)) { y }\n    _ { 0 }\n)",
        "match \"ready\" (\n    \"waiting\" { 0 }\n    \"ready\" { 1 }\n    _ { 2 }\n)",
        "(1 2 [+ (3 4)])",
        "/ (9 2)",
        "< (2 3)",
        "> (3 2)",
        "= ((1 2) (1 2))",
        "if true 1 2",
        "if false 1 2",
        "if false 1",
        "attempt { 42 }",
        "var value 0\nval deferred { let value 1 }\nvalue",
        "val deferred @(:value) { + (value 1) }\nval value 41\ndo deferred",
        "do { + (20 22) }",
        "val Template {\n    pub val answer 42\n}\nval object [new Template]\nobject.answer",
        "val base 40\nval object [new {\n    pub val answer [+ (base 2)]\n}]\nobject.answer",
        "var value 0\nval failure [attempt {\n    let value 1\n    / (1 0)\n}]\nvalue",
        "function early () {\n    attempt { return 42 }\n    0\n}\n[early ()]",
        "var value 0\nwhile true {\n    attempt {\n        let value 1\n        break 42\n    }\n}\nvalue",
        "var index 0\nwhile [< (index 3)] {\n    attempt {\n        let index [+ (index 1)]\n        continue\n    }\n}\nindex",
        "var value 1\nlet value [+ (value 2)]\nvalue",
        "var value 0\nwhile [< (value 5)] {\n    let value [+ (value 1)]\n}\nvalue",
        "var value 0\nuntil [= (value 5)] {\n    let value [+ (value 1)]\n}\nvalue",
        "var value 0\nwhile true {\n    let value [+ (value 1)]\n    if [= (value 3)] { break value }\n}\nvalue",
        "var index 0\nvar total 0\nwhile [< (index 5)] {\n    let index [+ (index 1)]\n    if [= (index 3)] { continue }\n    let total [+ (total index)]\n}\ntotal",
        "function identity :value { value }\n[identity 42]",
        "function add (:left :right) { + (left right) }\n[add (20 22)]",
        "function nested ((:left :right) :tail) { + (left right tail) }\n[nested ((10 20) 12)]",
        "function choose (:condition :value) {\n    if condition { return value }\n    0\n}\n[choose (true 42)]",
        "function fibonacci (:value) {\n    if [< (value 3)] 1 [+ ([fibonacci ([- (value 1)])] [fibonacci ([- (value 2)])])]\n}\n[fibonacci (10)]",
        "function read () { later }\nval reader read\nval later 42\n[reader ()]",
        "function read () { later }\nif true { val later 42 }\n[read ()]",
        "function multiplier (:factor) {\n    function apply (:value) { * (factor value) }\n    apply\n}\nval times_six [multiplier (6)]\n[times_six (7)]",
        "function make_adder (:captured) {\n    function add (:value) { + (captured value) }\n    add\n}\nval add_two [make_adder (2)]\nval add_ten [make_adder (10)]\n+ ([add_two (5)] [add_ten (5)])",
        "function make_adder (:captured) {\n    function add (:value) { + (captured value) }\n    add\n}\nval method [make_adder (2)]\nval (:from_list) (method)\n[from_list (40)]",
        "function make_adder (:captured) {\n    function add (:value) { + (captured value) }\n    add\n}\nval method [make_adder (2)]\nval object [new { pub val call method }]\n[object.call (40)]",
        "function counter (:start) {\n    var value start\n    function next () {\n        let value [+ (value 1)]\n        value\n    }\n    next\n}\nval first [counter (0)]\n[first ()]\n[first ()]",
        "function counter (:start) {\n    var value start\n    function next () { let value [+ (value 1)] }\n    next\n}\nval first [counter (0)]\nval second [counter (10)]\n+ ([first ()] [first ()] [second ()])",
        "function make_reader () {\n    function read () { later }\n    val reader read\n    val later 42\n    [reader ()]\n}\n[make_reader ()]",
        "function make_reader () {\n    function read () { later }\n    if true { val later 42 }\n    [read ()]\n}\n[make_reader ()]",
        "function make_reader () {\n    function read () { later }\n    attempt { val later 42 }\n    [read ()]\n}\n[make_reader ()]",
        "val setup { val later 42 }\nfunction run () {\n    do setup\n    later\n}\n[run ()]",
        "val base { val later 42 }\nval setup base\nfunction run () {\n    do setup\n    later\n}\n[run ()]",
        "var retained 0\nval failure [attempt { let (retained missing) (1 2) }]\nretained",
        "function outer (:value) {\n    function count (:remaining) {\n        if [= (remaining 0)] value [count ([- (remaining 1)])]\n    }\n    [count (3)]\n}\n[outer (42)]",
    ] {
        assert_eq!(run_vm(source), run_interpreter(source), "{source}");
    }
}

#[test]
fn register_vm_calls_standard_native_namespaces() {
    assert_eq!(
        run_vm_with_standard_globals("Math.int 42.9"),
        Value::Integer(42)
    );
    assert_eq!(
        run_vm_with_standard_globals("String.concat (\"pi\" \"ma\")"),
        Value::String("pima".into())
    );
    assert_eq!(
        run_vm_with_standard_globals("Logic.not false"),
        Value::Boolean(true)
    );
}

#[test]
fn register_vm_user_bindings_shadow_standard_globals() {
    let source = "val Counter {\n    var value 0\n    pub function next () { let value [+ (value 1)] }\n}\nval counter [new Counter]\n[counter.next ()]\n[counter.next ()]";
    let mut sources = SourceMap::default();
    let source_id = sources.add("<vm-shadow-test>", source);
    let module = parse(&lex(source_id, source).unwrap()).unwrap();
    let mut machine = Machine::default();
    let program =
        pima::vm::compile_module_with_globals(&module, 0, machine.standard_globals()).unwrap();
    assert_eq!(
        machine
            .execute(&program)
            .map_err(|error| machine.diagnostic(error))
            .unwrap(),
        Value::Integer(2)
    );
}

#[test]
fn compiler_supports_mutable_namespace_bindings() {
    let mut sources = SourceMap::default();
    let source = "new { var value 1 }";
    let source_id = sources.add("<vm-test>", source);
    let tokens = lex(source_id, source).unwrap();
    let module = parse(&tokens).unwrap();
    let program = compile(&module).expect("mutable namespace should compile");
    assert!(Machine::default().execute(&program).is_ok());
}

#[test]
fn register_vm_block_literals_are_inert_values() {
    let Value::Block(_) = run_vm("{ 42 }") else {
        panic!("a block literal should produce a block value");
    };
}

#[test]
fn register_vm_block_context_failures_are_catchable() {
    let source = "attempt {\n\
                      do @(:missing) { 42 }\n\
                  }";
    let mut sources = SourceMap::default();
    let source_id = sources.add("<vm-block-context-test>", source);
    let tokens = lex(source_id, source).unwrap();
    let module = parse(&tokens).unwrap();
    let program = compile(&module).unwrap();
    let mut machine = Machine::default();

    let Value::Namespace(namespace) = machine.execute(&program).unwrap() else {
        panic!("attempt should catch the missing-context error");
    };
    let types = namespace
        .types
        .iter()
        .map(|symbol| machine.resolve_symbol(*symbol).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(types, ["error", "name_error", "missing_context"]);
}

#[test]
fn register_vm_match_failures_are_catchable() {
    for source in [
        "attempt { match 1 ( 2 { 0 } ) }",
        "attempt { match (1 2) ( (:value :value) { value } ) }",
    ] {
        let mut sources = SourceMap::default();
        let source_id = sources.add("<vm-match-error-test>", source);
        let tokens = lex(source_id, source).unwrap();
        let module = parse(&tokens).unwrap();
        let program = compile(&module).unwrap();
        let mut machine = Machine::default();

        let Value::Namespace(namespace) = machine.execute(&program).unwrap() else {
            panic!("attempt should catch the match error");
        };
        let types = namespace
            .types
            .iter()
            .map(|symbol| machine.resolve_symbol(*symbol).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(types, ["error", "match_error"]);
    }
}

#[test]
fn register_vm_tracks_runtime_binding_initialization() {
    for (source, expected) in [
        (
            "if false { val hidden 1 }\nattempt { hidden }",
            vec!["error", "name_error"],
        ),
        (
            "val failed [attempt { val (left right) (1) }]\nattempt { left }",
            vec!["error", "name_error"],
        ),
        (
            "attempt { val value 1\nval value 2 }",
            vec!["error", "name_error"],
        ),
        ("attempt { if 1 2 }", vec!["error", "type_error"]),
        ("attempt { [1 2] }", vec!["error", "type_error"]),
        (
            "val failure [attempt { [later ()] }]\nfunction later () { 42 }\nfailure",
            vec!["error", "name_error"],
        ),
        ("attempt { let missing 1 }", vec!["error", "name_error"]),
        (
            "function update (:value) { attempt { let value 2 } }\n[update (1)]",
            vec!["error", "mutation_error"],
        ),
    ] {
        let program = compile_vm(source);
        let mut machine = Machine::default();
        let value = machine.execute(&program).unwrap();
        assert_eq!(value_type_names(&machine, &value), expected, "{source}");
    }
}

#[test]
fn register_vm_namespace_enforces_member_visibility() {
    let source = "val caught [attempt {\n\
                      val object [new { val hidden 42 }]\n\
                      object.hidden\n\
                  }]\n\
                  caught";
    let mut sources = SourceMap::default();
    let source_id = sources.add("<vm-namespace-visibility-test>", source);
    let tokens = lex(source_id, source).unwrap();
    let module = parse(&tokens).unwrap();
    let program = compile(&module).unwrap();
    let mut machine = Machine::default();

    let Value::Namespace(namespace) = machine.execute(&program).unwrap() else {
        panic!("attempt should catch the visibility error");
    };
    let types = namespace
        .types
        .iter()
        .map(|symbol| machine.resolve_symbol(*symbol).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(types, ["error", "visibility_error"]);
}

#[test]
fn register_vm_constructs_and_throws_custom_errors() {
    let source = "attempt {\n\
                      throw [new {\n\
                          pub val types (:error :custom_error)\n\
                          pub val message \"custom failure\"\n\
                      }]\n\
                  }";
    let mut sources = SourceMap::default();
    let source_id = sources.add("<vm-custom-error-test>", source);
    let tokens = lex(source_id, source).unwrap();
    let module = parse(&tokens).unwrap();
    let program = compile(&module).unwrap();
    let mut machine = Machine::default();

    let Value::Namespace(namespace) = machine.execute(&program).unwrap() else {
        panic!("attempt should catch the custom error");
    };
    let types = namespace
        .types
        .iter()
        .map(|symbol| machine.resolve_symbol(*symbol).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(types, ["error", "custom_error"]);
}

#[test]
fn register_vm_collects_unreachable_closure_cell_cycles() {
    let source = "function build_cycle () {\n\
        var captured ()\n\
        function cycle () { captured }\n\
        let captured cycle\n\
        42\n\
    }\n\
    [build_cycle ()]";
    let mut sources = SourceMap::default();
    let source_id = sources.add("<vm-cycle-test>", source);
    let tokens = lex(source_id, source).unwrap();
    let module = parse(&tokens).unwrap();
    let program = compile(&module).unwrap();
    dumpster::unsync::collect();
    let baseline = pima::vm::live_cell_count();

    assert_eq!(
        Machine::default().execute(&program).unwrap(),
        pima::Value::Integer(42)
    );
    assert!(pima::vm::live_cell_count() > baseline);

    dumpster::unsync::collect();
    assert_eq!(pima::vm::live_cell_count(), baseline);
}

#[test]
fn register_vm_preserves_native_typed_errors() {
    let source = "/ (1 0)";
    let mut sources = SourceMap::default();
    let source_id = sources.add("<vm-error-test>", source);
    let tokens = lex(source_id, source).unwrap();
    let module = parse(&tokens).unwrap();
    let program = compile(&module).unwrap();
    let mut machine = Machine::default();

    let error = machine.execute(&program).expect_err("division should fail");
    let pima::Value::Namespace(namespace) = error.value().expect("native error should be typed")
    else {
        panic!("typed error should be a namespace");
    };
    let types = namespace
        .types
        .iter()
        .map(|symbol| machine.resolve_symbol(*symbol).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(types, ["error", "numeric_error"]);
}

#[test]
fn register_vm_attempt_catches_errors_across_function_frames() {
    let source = "function fail () { / (1 0) }\n\
                  val caught [attempt { [fail ()] }]\n\
                  caught";
    let mut sources = SourceMap::default();
    let source_id = sources.add("<vm-attempt-test>", source);
    let tokens = lex(source_id, source).unwrap();
    let module = parse(&tokens).unwrap();
    let program = compile(&module).unwrap();
    let mut machine = Machine::default();

    let Value::Namespace(namespace) = machine.execute(&program).unwrap() else {
        panic!("attempt should return the caught error");
    };
    let types = namespace
        .types
        .iter()
        .map(|symbol| machine.resolve_symbol(*symbol).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(types, ["error", "numeric_error"]);
}

#[test]
fn register_vm_throw_validates_values_and_uses_the_nearest_attempt() {
    let source = "attempt {\n\
                      throw [attempt { throw 42 }]\n\
                  }";
    let mut sources = SourceMap::default();
    let source_id = sources.add("<vm-throw-test>", source);
    let tokens = lex(source_id, source).unwrap();
    let module = parse(&tokens).unwrap();
    let program = compile(&module).unwrap();
    let mut machine = Machine::default();

    let Value::Namespace(namespace) = machine.execute(&program).unwrap() else {
        panic!("outer attempt should catch the rethrown validation error");
    };
    let types = namespace
        .types
        .iter()
        .map(|symbol| machine.resolve_symbol(*symbol).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(types, ["error", "type_error"]);
}
