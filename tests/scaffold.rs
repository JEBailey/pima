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
    let outcome = interpreter.run_source("<test>", "function :invalid (value)\n");

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
        .prepare_source("<prepared>", "[+ 20 22]")
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

#[test]
fn interpreter_executes_supported_sources_with_the_register_vm() {
    let mut interpreter = Interpreter::new(Config::default());
    let outcome = interpreter.run_source(
        "<vm-interpreter-test>",
        "function :answer () { Math.int 42.9 }\n[answer ]",
    );
    assert!(outcome.is_success(), "{:?}", outcome.diagnostics);
    assert_eq!(outcome.value, Some(pima::Value::Integer(42)));
}

#[test]
fn register_vm_diagnostics_retain_source_and_call_stack() {
    let mut interpreter = Interpreter::default();
    let outcome = interpreter.run_source(
        "<vm-diagnostic-test>",
        "function :fail () { / 1 0 }\n[fail ]",
    );
    assert!(!outcome.is_success());
    let diagnostic = &outcome.diagnostics[0];
    assert!(diagnostic.primary_span.is_some());
    assert!(diagnostic.stack.iter().any(|frame| frame.name == "fail"));
}

#[test]
fn register_vm_loads_modules_and_calls_exported_functions() {
    let directory = std::env::temp_dir().join(format!(
        "pima-vm-modules-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(
        directory.join("math.pima"),
        "pub function :add (left right) { + left right }\n",
    )
    .unwrap();
    let mut interpreter = Interpreter::new(Config {
        working_directory: Some(directory.clone()),
    });
    let outcome = interpreter.run_source(
        "<vm-module-test>",
        "import \"math.pima\" as :custom\n[custom.add 20 22]",
    );
    assert!(outcome.is_success(), "{:?}", outcome.diagnostics);
    assert_eq!(outcome.value, Some(pima::Value::Integer(42)));
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn register_vm_loads_native_modules_and_selected_members() {
    let mut interpreter = Interpreter::default();
    let native = interpreter.run_source(
        "<vm-native-module-test>",
        "import /pima/io as :io\n[io.current_directory ]",
    );
    assert!(native.is_success(), "{:?}", native.diagnostics);
    assert!(matches!(native.value, Some(pima::Value::String(_))));

    let selected = interpreter.run_source(
        "<vm-selected-import-test>",
        "import Math.int as :integer\n[integer 42.9]",
    );
    assert!(selected.is_success(), "{:?}", selected.diagnostics);
    assert_eq!(selected.value, Some(pima::Value::Integer(42)));
}

#[test]
fn register_vm_loads_the_standard_library() {
    let mut interpreter = Interpreter::default();
    let outcome = interpreter.run_source(
        "<vm-standard-library-test>",
        "import /pima/library/standard\n[Math.sum (10 20 12)]",
    );
    assert!(outcome.is_success(), "{:?}", outcome.diagnostics);
    assert_eq!(outcome.value, Some(pima::Value::Integer(42)));
}

#[test]
fn register_vm_interpreter_retains_session_bindings() {
    let mut interpreter = Interpreter::default();
    let declaration = interpreter.run_source(
        "<vm-session-declaration>",
        "var :value 40\nfunction :add_two () { + value 2 }",
    );
    assert!(declaration.is_success(), "{:?}", declaration.diagnostics);
    let call = interpreter.run_source("<vm-session-call>", "[add_two ]");
    assert!(call.is_success(), "{:?}", call.diagnostics);
    assert_eq!(call.value, Some(pima::Value::Integer(42)));
}

#[test]
fn register_vm_reports_import_cycles() {
    let directory = std::env::temp_dir().join(format!(
        "pima-vm-cycle-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(directory.join("left.pima"), "import \"right.pima\"\n").unwrap();
    std::fs::write(directory.join("right.pima"), "import \"left.pima\"\n").unwrap();
    let mut interpreter = Interpreter::new(Config {
        working_directory: Some(directory.clone()),
    });
    let outcome = interpreter.run_source("<vm-cycle-test>", "import \"left.pima\"\n");
    assert!(!outcome.is_success());
    assert!(outcome.diagnostics[0].message.contains("import cycle"));
    std::fs::remove_dir_all(directory).unwrap();
}
