use pima::{Config, Interpreter};

fn run(source: &str) -> pima::RunOutcome {
    let mut interpreter = Interpreter::new(Config::default());
    interpreter.run_source("<test>", source)
}

fn run_ok(source: &str) -> pima::Value {
    let outcome = run(source);
    assert!(
        outcome.is_success(),
        "expected success but got diagnostics: {:?}",
        outcome.diagnostics
    );
    outcome.value.expect("expected a return value")
}

// ── Literals ──

#[test]
fn evaluates_integer_literal() {
    assert_eq!(run_ok("42"), pima::Value::Integer(42));
}

#[test]
fn evaluates_negative_integer() {
    assert_eq!(run_ok("-99"), pima::Value::Integer(-99));
}

#[test]
fn evaluates_float_literal() {
    let value = run_ok("3.125");
    assert_eq!(value, pima::Value::Float(3.125));
}

#[test]
fn evaluates_boolean_true() {
    assert_eq!(run_ok("true"), pima::Value::Boolean(true));
}

#[test]
fn evaluates_boolean_false() {
    assert_eq!(run_ok("false"), pima::Value::Boolean(false));
}

#[test]
fn evaluates_string_literal() {
    let value = run_ok(r#""hello""#);
    assert_eq!(value, pima::Value::String(std::sync::Arc::from("hello")));
}

#[test]
fn evaluates_string_with_escapes() {
    let value = run_ok(r#""line\nbreak""#);
    assert_eq!(
        value,
        pima::Value::String(std::sync::Arc::from("line\nbreak"))
    );
}

#[test]
fn evaluates_symbol_literal() {
    // Symbols evaluate to themselves; compare by display
    let value = run_ok(":name");
    assert!(matches!(value, pima::Value::Symbol(_)));
}

#[test]
fn evaluates_empty_list() {
    let value = run_ok("()");
    assert!(matches!(value, pima::Value::List(_)));
}

#[test]
fn evaluates_list_with_elements() {
    let value = run_ok("(1 2 3)");
    if let pima::Value::List(list) = value {
        let elems = list.to_vec();
        assert_eq!(elems.len(), 3);
        assert_eq!(elems[0], pima::Value::Integer(1));
        assert_eq!(elems[1], pima::Value::Integer(2));
        assert_eq!(elems[2], pima::Value::Integer(3));
    } else {
        panic!("expected list");
    }
}

#[test]
fn evaluates_nested_list() {
    let value = run_ok("(1 (2 3))");
    assert!(matches!(value, pima::Value::List(_)));
}

#[test]
fn evaluates_block_to_code_block_value() {
    let value = run_ok("{}");
    assert!(matches!(value, pima::Value::Block(_)));
}

#[test]
fn evaluates_empty_source_to_unit() {
    assert_eq!(run_ok(""), pima::Value::Unit);
}

// ── Bindings ──

#[test]
fn set_creates_immutable_binding() {
    let value = run_ok("set x 10\nx");
    assert_eq!(value, pima::Value::Integer(10));
}

#[test]
fn var_creates_mutable_binding() {
    let value = run_ok("var x 10\nx");
    assert_eq!(value, pima::Value::Integer(10));
}

#[test]
fn let_updates_mutable_binding() {
    let value = run_ok("var x 10\nlet x 20\nx");
    assert_eq!(value, pima::Value::Integer(20));
}

#[test]
fn let_on_immutable_is_error() {
    let outcome = run("set x 10\nlet x 20");
    assert!(!outcome.is_success());
}

#[test]
fn let_on_unbound_is_error() {
    let outcome = run("let x 10");
    assert!(!outcome.is_success());
}

#[test]
fn set_duplicate_is_error() {
    let outcome = run("set x 1\nset x 2");
    assert!(!outcome.is_success());
}

#[test]
fn var_duplicate_is_error() {
    let outcome = run("var x 1\nvar x 2");
    assert!(!outcome.is_success());
}

#[test]
fn shadowing_in_function_scope() {
    let value = run_ok("set x 1\nfunction f () {\n  set x 2\n  x\n}\n[f]");
    assert_eq!(value, pima::Value::Integer(2));
}

#[test]
fn outer_binding_still_visible_after_function() {
    let value = run_ok("set x 1\nfunction f () {\n  set x 2\n  x\n}\n[f]\nx");
    assert_eq!(value, pima::Value::Integer(1));
}

#[test]
fn unbound_identifier_is_error() {
    let outcome = run("missing_name");
    assert!(!outcome.is_success());
}

// ── Numeric arithmetic ──

#[test]
fn integer_addition() {
    assert_eq!(run_ok("[+ 2 3]"), pima::Value::Integer(5));
}

#[test]
fn integer_subtraction() {
    assert_eq!(run_ok("[- 10 3]"), pima::Value::Integer(7));
}

#[test]
fn integer_multiplication() {
    assert_eq!(run_ok("[* 4 5]"), pima::Value::Integer(20));
}

#[test]
fn division_returns_float() {
    assert_eq!(run_ok("[/ 10 4]"), pima::Value::Float(2.5));
}

#[test]
fn integer_division() {
    assert_eq!(run_ok("[div 10 3]"), pima::Value::Integer(3));
}

#[test]
fn integer_modulo() {
    assert_eq!(run_ok("[mod 10 3]"), pima::Value::Integer(1));
}

#[test]
fn euclidean_mod_with_negative() {
    // mod always returns >= 0
    assert_eq!(run_ok("[mod -10 3]"), pima::Value::Integer(2));
}

#[test]
fn mixed_int_float_promotes_to_float() {
    assert_eq!(run_ok("[+ 2 3.5]"), pima::Value::Float(5.5));
}

#[test]
fn division_by_zero_is_error() {
    assert!(!run("[/ 1 0]").is_success());
}

#[test]
fn div_by_zero_is_error() {
    assert!(!run("[div 1 0]").is_success());
}

#[test]
fn integer_overflow_is_error() {
    assert!(!run("[* 9223372036854775807 2]").is_success());
}

#[test]
fn chained_arithmetic() {
    // + 2 3 4 = 9 (left-to-right: + (+ 2 3) 4)
    assert_eq!(run_ok("[+ 2 3 4]"), pima::Value::Integer(9));
}

// ── Comparison ──

#[test]
fn less_than() {
    assert_eq!(run_ok("[< 1 2]"), pima::Value::Boolean(true));
    assert_eq!(run_ok("[< 2 1]"), pima::Value::Boolean(false));
}

#[test]
fn greater_than() {
    assert_eq!(run_ok("[> 2 1]"), pima::Value::Boolean(true));
}

#[test]
fn equality_same_value() {
    assert_eq!(run_ok("[= 5 5]"), pima::Value::Boolean(true));
}

#[test]
fn equality_different_types() {
    assert_eq!(run_ok("[= 5 \"hello\" ]"), pima::Value::Boolean(false));
}

#[test]
fn equality_int_float_same_math_value() {
    assert_eq!(run_ok("[= 5 5.0]"), pima::Value::Boolean(true));
}

#[test]
fn equality_lists_structural() {
    assert_eq!(run_ok("[= (1 2) (1 2)]"), pima::Value::Boolean(true));
    assert_eq!(run_ok("[= (1 2) (1 3)]"), pima::Value::Boolean(false));
}

#[test]
fn not_boolean() {
    assert_eq!(run_ok("[not true]"), pima::Value::Boolean(false));
    assert_eq!(run_ok("[not false]"), pima::Value::Boolean(true));
}

// ── Types ──

#[test]
fn types_returns_list_of_symbols() {
    let value = run_ok("[types 42]");
    assert!(matches!(value, pima::Value::List(_)));
}

#[test]
fn is_predicate() {
    assert_eq!(run_ok("[is? 42 :integer]"), pima::Value::Boolean(true));
    assert_eq!(run_ok("[is? 42 :string]"), pima::Value::Boolean(false));
}

// ── Conditionals ──

#[test]
fn if_true_branch() {
    assert_eq!(run_ok("if true 1 2"), pima::Value::Integer(1));
}

#[test]
fn if_false_branch() {
    assert_eq!(run_ok("if false 1 2"), pima::Value::Integer(2));
}

#[test]
fn if_with_blocks() {
    let value = run_ok("if true { 10 }{ 20 }");
    assert_eq!(value, pima::Value::Integer(10));
}

#[test]
fn if_with_block_consequent_expr_alternative() {
    let value = run_ok("if false { 10 } 20");
    assert_eq!(value, pima::Value::Integer(20));
}

// ── println ──

#[test]
fn println_prints_and_returns_unit() {
    // We can verify it doesn't error and returns unit-like outcome
    let value = run_ok(r#"println "hello""#);
    assert!(matches!(value, pima::Value::Unit));
}

// ── Functions ──

#[test]
fn function_declaration_and_call() {
    let value = run_ok("function add (:x :y) {\n  + x y\n}\n[add 3 4]");
    assert_eq!(value, pima::Value::Integer(7));
}

#[test]
fn function_return_last_expression() {
    let value = run_ok("function greet (:name) {\n  name\n}\n[greet \"world\"]");
    assert_eq!(value, pima::Value::String(std::sync::Arc::from("world")));
}

#[test]
fn explicit_return() {
    let value = run_ok("function f (:x) {\n  return x\n  999\n}\n[f 5]");
    assert_eq!(value, pima::Value::Integer(5));
}

#[test]
fn bare_return_returns_unit() {
    let value = run_ok("function f () {\n  return\n  999\n}\n[f]");
    assert_eq!(value, pima::Value::Unit);
}

#[test]
fn simple_recursion() {
    let value = run_ok(
        "function countdown (:n) {\n  if [= n 0] 0 [+ 1 [countdown [- n 1]]]\n}\n[countdown 3]",
    );
    assert_eq!(value, pima::Value::Integer(3));
}

#[test]
fn wrong_arity_is_error() {
    let outcome = run("function f (:x) { x }\n[f 1 2]");
    assert!(!outcome.is_success());
}

#[test]
fn zero_arg_call_with_brackets() {
    let value = run_ok("function f () { 42 }\n[f]");
    assert_eq!(value, pima::Value::Integer(42));
}

// ── Closures ──

#[test]
fn closure_captures_environment() {
    let value = run_ok(
        "function make_adder (:n) {\n  function inner (:x) {\n    + x n\n  }\n}\nset add5 [make_adder 5]\n[add5 3]",
    );
    assert_eq!(value, pima::Value::Integer(8));
}

// ── Partial Application ──

#[test]
fn partial_application_with_underscore() {
    let value = run_ok("function add (:x :y) {\n  + x y\n}\nset add5 [add 5 _]\n[add5 10]");
    assert_eq!(value, pima::Value::Integer(15));
}

// ── Loops ──

#[test]
fn while_loop() {
    let value = run_ok("var x 0\nwhile [< x 5] {\n  let x [+ x 1]\n}\nx");
    assert_eq!(value, pima::Value::Integer(5));
}

#[test]
fn until_loop() {
    let value = run_ok("var x 0\nuntil [= x 5] {\n  let x [+ x 1]\n}\nx");
    assert_eq!(value, pima::Value::Integer(5));
}

#[test]
fn break_exits_loop() {
    let value =
        run_ok("var x 0\nwhile true {\n  let x [+ x 1]\n  if [= x 3] { break x } { }\n}\nx");
    assert_eq!(value, pima::Value::Integer(3));
}

#[test]
fn bare_break_exits_with_unit() {
    // After loop, x should be the value of break (unit)
    // but we check the loop itself terminates
    let value = run_ok("var x 0\nwhile true {\n  let x [+ x 1]\n  break\n}\nx");
    // x retains last assigned value since break doesn't reassign
    assert_eq!(value, pima::Value::Integer(1));
}

#[test]
fn continue_skips_iteration() {
    // This test uses >= indirectly — skip until stdlib is ready
    // Actually, it only uses < and = which are native
    let value = run_ok(
        "var x 0\nvar s 0\nwhile [< x 5] {\n  let x [+ x 1]\n  if [= x 3] { continue } { }\n  let s [+ s x]\n}\ns",
    );
    // s = 1 + 2 + 4 + 5 = 12
    assert_eq!(value, pima::Value::Integer(12));
}

#[test]
fn loop_returns_last_value() {
    let value = run_ok("var x 10\nwhile [< x 13] {\n  let x [+ x 1]\n}\nx");
    assert_eq!(value, pima::Value::Integer(13));
}

#[test]
fn empty_loop_returns_unit() {
    let value = run_ok("while false { }");
    assert_eq!(value, pima::Value::Unit);
}

// ── eval ──

#[test]
fn eval_executes_block_in_current_scope() {
    let value = run_ok("set code { x }\nset x 42\neval code");
    assert_eq!(value, pima::Value::Integer(42));
}

#[test]
fn eval_can_create_bindings_in_caller_scope() {
    let value = run_ok("set code { set y 99 }\neval code\ny");
    assert_eq!(value, pima::Value::Integer(99));
}

// ── attempt ──

#[test]
fn attempt_catches_error() {
    let outcome = run("set result [attempt {\n  throw_error_here\n}]\nresult");
    // Should not crash - result should be bound to the error value
    assert!(outcome.is_success());
}

// ── Namespaces ──

#[test]
fn new_creates_namespace() {
    let value = run_ok("set Template {\n  pub set x 10\n}\nset obj [new Template]\nobj");
    assert!(matches!(value, pima::Value::Namespace(_)));
}

#[test]
fn member_access() {
    let value = run_ok("set Template {\n  pub set x 10\n}\nset obj [new Template]\nobj.x");
    assert_eq!(value, pima::Value::Integer(10));
}

#[test]
fn member_function_call() {
    let value = run_ok(
        "set Counter {\n  var v 0\n  pub function inc () {\n    let v [+ v 1]\n    v\n  }\n}\nset c [new Counter]\n[c.inc]",
    );
    assert_eq!(value, pima::Value::Integer(1));
}

#[test]
fn private_member_access_is_error() {
    let outcome = run("set Template {\n  set x 10\n}\nset obj [new Template]\nobj.x");
    assert!(!outcome.is_success());
}

#[test]
fn namespace_independence() {
    // Two instances have independent state
    let value = run_ok(
        "set Counter {\n  var v 0\n  pub function inc () {\n    let v [+ v 1]\n  }\n  pub function get () { v }\n}\nset a [new Counter]\nset b [new Counter]\n[a.inc]\n[a.get]",
    );
    assert_eq!(value, pima::Value::Integer(1));
}

// ── String operations ──

#[test]
fn concat_strings() {
    assert_eq!(
        run_ok(r#"[concat "hello" " " "world"]"#),
        pima::Value::String(std::sync::Arc::from("hello world"))
    );
}

#[test]
fn string_length() {
    assert_eq!(run_ok(r#"[length "hello"]"#), pima::Value::Integer(5));
}

#[test]
fn string_slice() {
    assert_eq!(
        run_ok(r#"[slice "hello" 1 4]"#),
        pima::Value::String(std::sync::Arc::from("ell"))
    );
}

#[test]
fn string_chars() {
    let value = run_ok(r#"[chars "abc"]"#);
    assert!(matches!(value, pima::Value::List(_)));
}

#[test]
fn string_value_conversion() {
    let value = run_ok(r#"[string 42]"#);
    assert_eq!(value, pima::Value::String(std::sync::Arc::from("42")));
}

// ── List operations ──

#[test]
fn push_prepends() {
    let value = run_ok("[push (2 3) 1]");
    if let pima::Value::List(l) = value {
        let e = l.to_vec();
        assert_eq!(e.len(), 3);
        assert_eq!(e[0], pima::Value::Integer(1));
    } else {
        panic!("expected list");
    }
}

#[test]
fn append_appends() {
    let value = run_ok("[append (1 2) 3]");
    if let pima::Value::List(l) = value {
        let e = l.to_vec();
        assert_eq!(e.len(), 3);
        assert_eq!(e[2], pima::Value::Integer(3));
    } else {
        panic!("expected list");
    }
}

#[test]
fn head_returns_first() {
    assert_eq!(run_ok("[head (1 2 3)]"), pima::Value::Integer(1));
}

#[test]
fn rest_returns_tail() {
    let value = run_ok("[rest (1 2 3)]");
    if let pima::Value::List(l) = value {
        let e = l.to_vec();
        assert_eq!(e.len(), 2);
        assert_eq!(e[0], pima::Value::Integer(2));
    } else {
        panic!("expected list");
    }
}

#[test]
fn empty_list_predicate() {
    assert_eq!(run_ok("[empty? ()]"), pima::Value::Boolean(true));
    assert_eq!(run_ok("[empty? (1)]"), pima::Value::Boolean(false));
}

#[test]
fn head_empty_list_is_error() {
    assert!(!run("[head ()]").is_success());
}

#[test]
fn rest_empty_list_is_error() {
    assert!(!run("[rest ()]").is_success());
}

// ── int conversion ──

#[test]
fn int_from_integer() {
    assert_eq!(run_ok("[int 42]"), pima::Value::Integer(42));
}

#[test]
fn int_from_float_truncates() {
    assert_eq!(run_ok("[int 3.7]"), pima::Value::Integer(3));
}

// int(0.0) is valid — converts to 0
// Actual NaN/Infinity tests would need float literals that produce them

// ── Error handling ──

#[test]
fn throw_and_attempt() {
    // This requires error namespaces to be implemented
    // Placeholder - will work after error system
}

// ── Imports ──

#[test]
fn import_standard_library() {
    // This requires module loader + standard library
    // Placeholder
}

// ── Call non-callable ──

#[test]
fn call_non_callable_is_error() {
    assert!(!run("[42]").is_success());
}

#[test]
fn call_string_is_error() {
    assert!(!run(r#"["hello"]"#).is_success());
}

// ── Control flow errors ──

#[test]
fn return_outside_function_is_error() {
    assert!(!run("return 42").is_success());
}

#[test]
fn break_outside_loop_is_error() {
    assert!(!run("break").is_success());
}

#[test]
fn continue_outside_loop_is_error() {
    assert!(!run("continue").is_success());
}

// ── Conformance examples ──

#[test]
fn fibonacci_example_runs() {
    // The fibonacci example expects [fibonacci 12] = 144
    // Uses <= from standard lib
    // Placeholder until stdlib + imports work
}

#[test]
fn namespace_test_example_runs() {
    // square1.area = 400 (length=5, width=80)
    // square2.area = 400 (independent)
    // After set_width 40: square1.area = 200
    // Placeholder until namespaces fully work
}

// ── Immediate call syntax ──

#[test]
fn immediate_call_basic() {
    assert_eq!(run_ok("[+ 6 7]"), pima::Value::Integer(13));
}

#[test]
fn immediate_call_nested() {
    // + [+ 1 2] 3 = 6
    let value = run_ok("[+ [+ 1 2] 3]");
    assert_eq!(value, pima::Value::Integer(6));
}

#[test]
fn zero_arg_invocation() {
    let value = run_ok("function f () { 99 }\n[f]");
    assert_eq!(value, pima::Value::Integer(99));
}

// ── Multiple statements, last value wins ──

#[test]
fn last_statement_value() {
    let value = run_ok("set x 1\nset y 2\ny");
    assert_eq!(value, pima::Value::Integer(2));
}

// ── pub declarations ──

#[test]
fn pub_set_in_namespace() {
    let value = run_ok("set T {\n  pub set val 42\n}\nset o [new T]\no.val");
    assert_eq!(value, pima::Value::Integer(42));
}

// ── Non-local control flow through eval ──

#[test]
fn eval_can_return_from_enclosing_function() {
    let value = run_ok("function f () {\n  eval { return 99 }\n  1\n}\n[f]");
    assert_eq!(value, pima::Value::Integer(99));
}

#[test]
fn list_elements_evaluate_left_to_right() {
    let value = run_ok("var x 0\nset values ([let x [+ x 1]] [let x [+ x 1]])\nvalues");
    let pima::Value::List(values) = value else {
        panic!("expected list");
    };
    assert_eq!(
        values.to_vec(),
        vec![pima::Value::Integer(1), pima::Value::Integer(2)]
    );
}

#[test]
fn float_division_checks_the_denominator() {
    assert_eq!(run_ok("[/ 0.0 2]"), pima::Value::Float(0.0));
    assert!(!run("[/ 2 0.0]").is_success());
}

#[test]
fn integer_division_truncates_toward_zero() {
    assert_eq!(run_ok("[div -5 2]"), pima::Value::Integer(-2));
    assert_eq!(run_ok("[div 5 -2]"), pima::Value::Integer(-2));
}

#[test]
fn integer_division_overflow_is_a_pima_error() {
    assert!(!run("[div -9223372036854775808 -1]").is_success());
    assert_eq!(
        run_ok("[mod -9223372036854775808 -1]"),
        pima::Value::Integer(0)
    );
}

#[test]
fn eval_uses_the_block_origin_module() {
    let mut interpreter = Interpreter::default();
    let declaration = interpreter.run_source("<first>", "set code { 42 }\n");
    assert!(declaration.is_success(), "{:?}", declaration.diagnostics);

    let outcome = interpreter.run_source("<second>", "eval code\n");
    assert!(outcome.is_success(), "{:?}", outcome.diagnostics);
    assert_eq!(outcome.value, Some(pima::Value::Integer(42)));
}

#[test]
fn caught_loop_condition_error_does_not_leave_a_loop_frame() {
    let mut interpreter = Interpreter::default();
    let first = interpreter.run_source("<loop>", "while missing { }\n");
    assert!(!first.is_success());

    let outcome = interpreter.run_source("<after-loop>", "break\n");
    assert!(!outcome.is_success());
    assert!(
        outcome.diagnostics[0].message.contains("outside of a loop"),
        "{:?}",
        outcome.diagnostics
    );
}

#[test]
fn partial_application_rejects_excess_arguments() {
    let outcome = run("function add (:x :y) { + x y }\nset partial [add 1 _ 3]\n");
    assert!(!outcome.is_success());
}

#[test]
fn repeated_method_access_preserves_function_identity() {
    assert_eq!(
        run_ok(
            r#"set Template {
    pub function method () { 1 }
}
set instance [new Template]
[= instance.method instance.method]
"#,
        ),
        pima::Value::Boolean(true)
    );
}

#[test]
fn namespace_types_reject_duplicates_and_fundamental_types() {
    assert!(!run("set Bad { pub set types (:thing :thing) }\nnew Bad\n").is_success());
    assert!(!run("set Bad { pub set types (:integer) }\nnew Bad\n").is_success());
}

fn module_test_directory(name: &str) -> std::path::PathBuf {
    static NEXT_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let id = NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let directory = std::env::temp_dir().join(format!("pima-{name}-{}-{id}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    directory
}

#[test]
fn file_imports_resolve_from_the_working_directory() {
    let directory = module_test_directory("file-import");
    std::fs::write(
        directory.join("answer.po"),
        "pub set answer 42\nset hidden 9\n",
    )
    .unwrap();
    let mut interpreter = Interpreter::new(Config {
        working_directory: Some(directory),
    });

    let outcome = interpreter.run_source("<test>", "import \"answer.po\"\nanswer\n");
    assert!(outcome.is_success(), "{:?}", outcome.diagnostics);
    assert_eq!(outcome.value, Some(pima::Value::Integer(42)));
}

#[test]
fn module_aliases_share_the_cached_module() {
    let directory = module_test_directory("module-cache");
    std::fs::write(
        directory.join("identity.po"),
        "pub function identity (:value) { value }\n",
    )
    .unwrap();
    let mut interpreter = Interpreter::new(Config {
        working_directory: Some(directory),
    });

    let outcome = interpreter.run_source(
        "<test>",
        "import \"identity.po\" as first\nimport \"identity.po\" as second\n[= first.identity second.identity]\n",
    );
    assert!(outcome.is_success(), "{:?}", outcome.diagnostics);
    assert_eq!(outcome.value, Some(pima::Value::Boolean(true)));
}

#[test]
fn unaliased_imports_are_live_read_only_views() {
    let directory = module_test_directory("live-import");
    std::fs::write(
        directory.join("counter.po"),
        "pub var count 0\npub function bump () { let count [+ count 1] }\n",
    )
    .unwrap();
    let mut interpreter = Interpreter::new(Config {
        working_directory: Some(directory),
    });

    let outcome = interpreter.run_source("<test>", "import \"counter.po\"\n[bump]\ncount\n");
    assert!(outcome.is_success(), "{:?}", outcome.diagnostics);
    assert_eq!(outcome.value, Some(pima::Value::Integer(1)));
}

#[test]
fn import_cycles_are_reported_as_pima_errors() {
    let directory = module_test_directory("module-cycle");
    std::fs::write(directory.join("a.po"), "import \"b.po\"\npub set a 1\n").unwrap();
    std::fs::write(directory.join("b.po"), "import \"a.po\"\npub set b 2\n").unwrap();
    let mut interpreter = Interpreter::new(Config {
        working_directory: Some(directory),
    });

    let outcome = interpreter.run_source("<test>", "import \"a.po\"\n");
    assert!(!outcome.is_success());
    assert!(outcome.diagnostics[0].message.contains("import cycle"));
    let message = &outcome.diagnostics[0].message;
    let first_a = message.find("a.po").expect("cycle should include a.po");
    let b = message.find("b.po").expect("cycle should include b.po");
    let second_a = message.rfind("a.po").expect("cycle should close with a.po");
    assert!(first_a < b && b < second_a, "{message}");
}

#[test]
fn runtime_diagnostics_include_origin_and_function_stack() {
    let source =
        "function inner () {\n    missing\n}\nfunction outer () {\n    [inner]\n}\n[outer]\n";
    let outcome = run(source);
    assert!(!outcome.is_success());

    let diagnostic = &outcome.diagnostics[0];
    let origin = diagnostic
        .primary_span
        .expect("runtime error should have an origin");
    assert_eq!(&source[origin.start..origin.end], "missing");
    assert_eq!(
        diagnostic
            .stack
            .iter()
            .map(|frame| frame.name.as_str())
            .collect::<Vec<_>>(),
        ["inner", "outer"]
    );
}

#[test]
fn io_module_reads_and_writes_relative_to_working_directory() {
    let directory = module_test_directory("io");
    let mut interpreter = Interpreter::new(Config {
        working_directory: Some(directory.clone()),
    });
    let outcome = interpreter.run_source(
        "<test>",
        "import \"/po/io\" as io\n[io.write_text \"message.txt\" \"hello\"]\n[io.read_text \"message.txt\"]\n",
    );

    assert!(outcome.is_success(), "{:?}", outcome.diagnostics);
    assert_eq!(
        outcome.value,
        Some(pima::Value::String(std::sync::Arc::from("hello")))
    );
    assert_eq!(
        std::fs::read_to_string(directory.join("message.txt")).unwrap(),
        "hello"
    );
}

#[test]
fn io_module_classifies_invalid_utf8() {
    let directory = module_test_directory("io-encoding");
    std::fs::write(directory.join("invalid.txt"), [0xff, 0xfe]).unwrap();
    let mut interpreter = Interpreter::new(Config {
        working_directory: Some(directory),
    });
    let outcome = interpreter.run_source(
        "<test>",
        "import \"/po/io\" as io\nset error [attempt {\n    [io.read_text \"invalid.txt\"]\n}]\n[is? error :invalid_encoding]\n",
    );

    assert!(outcome.is_success(), "{:?}", outcome.diagnostics);
    assert_eq!(outcome.value, Some(pima::Value::Boolean(true)));
}

#[test]
fn imports_are_rejected_outside_module_scope() {
    let directory = module_test_directory("nested-import");
    std::fs::write(directory.join("dependency.po"), "pub set answer 42\n").unwrap();
    let mut interpreter = Interpreter::new(Config {
        working_directory: Some(directory),
    });
    let outcome = interpreter.run_source(
        "<test>",
        "function load () {\n    import \"dependency.po\"\n}\n[load]\n",
    );

    assert!(!outcome.is_success());
    assert!(
        outcome.diagnostics[0]
            .message
            .contains("only at module scope")
    );
}

// ── Namespace types ──

#[test]
fn namespace_custom_types() {
    let value =
        run_ok("set Square {\n  pub set types (:square :shape)\n}\nset s [new Square]\n[types s]");
    // Should be a list containing :namespace, :square, :shape
    assert!(matches!(value, pima::Value::List(_)));
}

#[test]
fn is_type_on_namespace() {
    assert_eq!(
        run_ok("set T {\n  pub set types (:my_type)\n}\nset o [new T]\n[is? o :my_type]"),
        pima::Value::Boolean(true)
    );
}

// ── Member access on function returns captured namespace ──

#[test]
fn member_access_returns_bound_function() {
    // square.area should return the function with namespace env captured
    let value = run_ok(
        "set Square {\n  set w 10\n  pub function area () { w }\n}\nset s [new Square]\n[s.area]",
    );
    assert_eq!(value, pima::Value::Integer(10));
}

// ── not requires boolean ──

#[test]
fn not_non_boolean_is_error() {
    assert!(!run("[not 42]").is_success());
}

// ── Comparison with multiple operands ──

#[test]
fn comparison_accepts_two_operands() {
    // = compares two values
    assert_eq!(run_ok("[= 1 1]"), pima::Value::Boolean(true));
}

// ── if condition must be boolean ──

#[test]
fn if_non_boolean_condition_is_error() {
    assert!(!run("if 1 true false").is_success());
}

// ── Loop condition must be boolean ──

#[test]
fn while_non_boolean_condition_is_error() {
    assert!(!run("while 1 { }").is_success());
}
