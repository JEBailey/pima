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
    let outcome = interpreter.run_source("<test>", "function invalid (value) {}\n");

    assert!(!outcome.is_success());
    assert!(
        outcome.diagnostics[0]
            .message
            .contains("parameters must be symbols")
    );
}
