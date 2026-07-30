use pima::{
    Interpreter, Value,
    source::SourceMap,
    syntax::{lexer::lex, parser::parse},
    vm::{Machine, compile},
};

fn run_vm(source: &str) -> Value {
    let mut sources = SourceMap::default();
    let source_id = sources.add("<vm-test>", source);
    let tokens = lex(source_id, source).expect("source should lex");
    let module = parse(&tokens).expect("source should parse");
    let program = compile(&module).expect("source should compile");
    Machine.execute(&program).expect("program should execute")
}

fn run_tree(source: &str) -> Value {
    let mut interpreter = Interpreter::default();
    let outcome = interpreter.run_source("<tree-test>", source);
    assert!(outcome.is_success(), "{:?}", outcome.diagnostics);
    outcome.value.expect("program should return a value")
}

#[test]
fn register_vm_matches_tree_walker_for_supported_programs() {
    for source in [
        "42",
        "val left 20\nval right 22\n+ (left right)",
        "(1 2 [+ (3 4)])",
        "/ (9 2)",
        "< (2 3)",
        "> (3 2)",
        "= ((1 2) (1 2))",
        "if true 1 2",
        "if false 1 2",
        "if false 1",
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
        "function multiplier (:factor) {\n    function apply (:value) { * (factor value) }\n    apply\n}\nval times_six [multiplier (6)]\n[times_six (7)]",
        "function make_adder (:captured) {\n    function add (:value) { + (captured value) }\n    add\n}\nval add_two [make_adder (2)]\nval add_ten [make_adder (10)]\n+ ([add_two (5)] [add_ten (5)])",
        "function counter (:start) {\n    var value start\n    function next () {\n        let value [+ (value 1)]\n        value\n    }\n    next\n}\nval first [counter (0)]\n[first ()]\n[first ()]",
        "function counter (:start) {\n    var value start\n    function next () { let value [+ (value 1)] }\n    next\n}\nval first [counter (0)]\nval second [counter (10)]\n+ ([first ()] [first ()] [second ()])",
    ] {
        assert_eq!(run_vm(source), run_tree(source), "{source}");
    }
}

#[test]
fn compiler_rejects_unsupported_constructs_explicitly() {
    let mut sources = SourceMap::default();
    let source = "new { val value 1 }";
    let source_id = sources.add("<vm-test>", source);
    let tokens = lex(source_id, source).unwrap();
    let module = parse(&tokens).unwrap();
    let diagnostics = compile(&module).expect_err("namespaces are not implemented yet");
    assert!(
        diagnostics[0]
            .message
            .contains("not supported by the register VM yet")
    );
}
