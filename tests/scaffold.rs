use pima::{Config, Interpreter};

#[test]
fn interpreter_can_be_created() {
    let interpreter = Interpreter::new(Config::default());
    drop(interpreter);
}

#[test]
fn empty_source_evaluation_returns_unit() {
    let mut interpreter = Interpreter::default();
    let outcome = interpreter.run_source("<test>", "");

    assert!(outcome.is_success());
    assert!(outcome.value.is_some());
}

#[test]
fn interpreter_reports_lexer_errors_before_evaluation() {
    let mut interpreter = Interpreter::default();
    let outcome = interpreter.run_source("<test>", "\"unterminated");

    assert!(!outcome.is_success());
    assert_eq!(
        outcome.diagnostics[0].message,
        "unterminated string literal"
    );
}

#[test]
fn interpreter_reports_parser_errors_before_evaluation() {
    let mut interpreter = Interpreter::default();
    let outcome = interpreter.run_source("<test>", "function invalid (:value)\n");

    assert!(!outcome.is_success());
    assert!(
        outcome.diagnostics[0]
            .message
            .contains("expected function body expression")
    );
}

#[test]
fn prepared_program_can_be_executed_repeatedly() {
    let mut interpreter = Interpreter::default();
    let program = interpreter
        .prepare_source("<prepared>", "[+ (20 22)]")
        .expect("source should prepare");

    for _ in 0..2 {
        let outcome = interpreter.run_prepared(program);
        assert!(outcome.is_success(), "{:?}", outcome.diagnostics);
        assert_eq!(outcome.value, Some(pima::Value::Integer(42)));
    }
}

#[test]
fn prepared_program_is_rejected_by_another_interpreter() {
    let mut owner = Interpreter::default();
    let program = owner
        .prepare_source("<prepared>", "42")
        .expect("source should prepare");
    let mut other = Interpreter::default();

    let outcome = other.run_prepared(program);
    assert!(!outcome.is_success());
    assert!(
        outcome.diagnostics[0]
            .message
            .contains("different interpreter")
    );
}
