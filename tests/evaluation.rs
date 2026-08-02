use pima::{Config, Interpreter};

fn run(source: &str) -> pima::RunOutcome {
    let mut interpreter = Interpreter::new(Config::default());
    if source.contains("/pima/library/standard") {
        interpreter.run_source("<test>", source)
    } else {
        let uses_list = ["push", "append", "head", "rest", "empty?"]
            .iter()
            .any(|name| source.contains(name));
        let uses_string = ["concat", "length", "slice", "chars", "string"]
            .iter()
            .any(|name| source.contains(name));
        let mut prelude = String::from(
            "import \"/pima/library/standard\"\nimport Math.*\nimport Console.*\nimport Logic.*\nval types Types.of\nval is? Types.is?\n",
        );
        if uses_list {
            prelude.push_str("import List.*\n");
        }
        if uses_string {
            prelude.push_str("import String.*\n");
        }
        prelude.push_str(source);
        interpreter.run_source("<test>", &prelude)
    }
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

#[test]
fn user_scope_contains_only_primitive_operators_by_default() {
    let mut interpreter = Interpreter::default();
    let operators = interpreter.run_source("<operators>", "[+ 20 22]\n");
    assert!(operators.is_success(), "{:?}", operators.diagnostics);
    assert_eq!(operators.value, Some(pima::Value::Integer(42)));

    for name in ["concat", "head", "println", "int", "not", "types"] {
        let outcome = interpreter.run_source("<name>", name);
        assert!(
            !outcome.is_success(),
            "`{name}` should require its standard-library namespace"
        );
        assert!(outcome.diagnostics[0].message.contains("unbound"));
    }
}

#[test]
fn standard_library_exposes_core_functions_through_namespaces() {
    let value = run_ok(
        "import \"/pima/library/standard\"\n\
         ([String.concat \"pi\" \"ma\"] [Math.int 2.9] [Logic.not false] [Types.is? 1 :integer])",
    );
    assert_eq!(
        value,
        pima::Value::List(
            [
                pima::Value::String("pima".into()),
                pima::Value::Integer(2),
                pima::Value::Boolean(true),
                pima::Value::Boolean(true),
            ]
            .into_iter()
            .collect(),
        )
    );
}

#[test]
fn remote_futures_transport_values() {
    let value = run_ok(
        "val Worker { pub function echo (value) value }\n\
         val worker [remote Worker]\n\
         val pending [worker.echo (1 42)]\n\
         val result [await pending]\n\
         ([Types.is? pending :future] result)",
    );
    assert_eq!(
        value,
        pima::Value::List(
            [
                pima::Value::Boolean(true),
                pima::Value::List(
                    [pima::Value::Integer(1), pima::Value::Integer(42),]
                        .into_iter()
                        .collect(),
                ),
            ]
            .into_iter()
            .collect()
        )
    );
}

#[test]
fn awaiting_a_future_is_repeatable() {
    let awaited_twice = run_ok(
        "val Worker { pub val value 42 }\n\
         val worker [remote Worker]\n\
         val pending worker.value\n\
         val first [await pending]\n\
         val second [await pending]\n\
         [= first second]",
    );
    assert_eq!(awaited_twice, pima::Value::Boolean(true));
}

#[test]
fn remote_constructs_a_namespace_in_an_isolated_vm() {
    let value = run_ok(
        "val Template {\n\
             pub val value 41\n\
             pub function add (x) { + value x }\n\
         }\n\
         val worker [remote Template]\n\
         ([await worker.value] [await [worker.add 1]] [Types.is? worker :object])",
    );
    assert_eq!(
        value,
        pima::Value::List(
            [
                pima::Value::Integer(41),
                pima::Value::Integer(42),
                pima::Value::Boolean(true),
            ]
            .into_iter()
            .collect()
        )
    );
}

#[test]
fn ordered_namespace_composition_uses_leftmost_precedence() {
    let value = run_ok(
        "val Base {\n\
             pub val value 1\n\
             pub val base_only 2\n\
             pub function get () value\n\
         }\n\
         val Specific { pub val value 10 }\n\
         val composed [new (Specific Base)]\n\
         (composed.value composed.base_only [composed.get])",
    );
    assert_eq!(
        value,
        pima::Value::List(
            [
                pima::Value::Integer(10),
                pima::Value::Integer(2),
                pima::Value::Integer(10),
            ]
            .into_iter()
            .collect()
        )
    );
}

#[test]
fn ordered_namespace_composition_runs_only_surviving_definitions() {
    let value = run_ok(
        "var events ()\n\
         function record (event) {\n\
             let events [List.append events event]\n\
             event\n\
         }\n\
         val Base {\n\
             pub val value [record :discarded]\n\
             pub val base_only [record :base]\n\
         }\n\
         val Specific { pub val value [record :specific] }\n\
         [new (Specific Base)]\n\
         [String.from events]",
    );
    assert_eq!(value, pima::Value::String("(:base :specific)".into()));
}

#[test]
fn ordered_namespace_composition_merges_types_in_source_order() {
    let value = run_ok(
        "val Base { pub val types (:base :shared) }\n\
         val Specific { pub val types (:specific :shared) }\n\
         val composed [new (Specific Base)]\n\
         [String.from [Types.of composed]]",
    );
    assert_eq!(
        value,
        pima::Value::String("(:object :specific :shared :base)".into())
    );
}

#[test]
fn ordered_namespace_composition_can_assign_a_surviving_mutable_binding() {
    let value = run_ok(
        "val Base {\n\
             var count 0\n\
             pub function get () count\n\
         }\n\
         val StartAtTen { let count 10 }\n\
         val composed [new (StartAtTen Base)]\n\
         [composed.get]",
    );
    assert_eq!(value, pima::Value::Integer(10));
}

#[test]
fn ordered_namespace_composition_creates_one_object_for_every_method() {
    let value = run_ok(
        "val General { pub function current () this }\n\
         val Specific { pub val value 42 }\n\
         val composed [new (Specific General)]\n\
         [= composed [composed.current]]",
    );
    assert_eq!(value, pima::Value::Boolean(true));
}

#[test]
fn ordered_namespace_composition_rejects_partial_destructuring_selection() {
    let outcome = run("val First { pub val left 1 }\n\
         val Second { pub val (left right) (2 3) }\n\
         new First Second");
    assert!(!outcome.is_success());
    assert!(outcome.diagnostics.iter().any(|diagnostic| {
        diagnostic
            .message
            .contains("cannot select only part of a destructuring declaration")
    }));
}

#[test]
fn remote_uses_the_same_template_composition_as_new() {
    let value = run_ok(
        "val Base {\n\
             pub val value 1\n\
             pub val base_only 2\n\
             pub function get () value\n\
         }\n\
         val Specific { pub val value 10 }\n\
         val worker [remote (Specific Base)]\n\
         ([await worker.value] [await worker.base_only] [await [worker.get]])",
    );
    assert_eq!(
        value,
        pima::Value::List(
            [
                pima::Value::Integer(10),
                pima::Value::Integer(2),
                pima::Value::Integer(10),
            ]
            .into_iter()
            .collect()
        )
    );
}

#[test]
fn remote_transports_required_context_as_an_immutable_snapshot() {
    let value = run_ok(
        "var seed 10\n\
         val Worker @(seed) {\n\
             var local seed\n\
             pub function read () (seed local)\n\
             pub function increment () { let local [+ local 1] }\n\
         }\n\
         val worker [remote Worker]\n\
         let seed 20\n\
         [await [worker.increment]]\n\
         [await [worker.read]]",
    );
    assert_eq!(
        value,
        pima::Value::List(
            [pima::Value::Integer(10), pima::Value::Integer(11)]
                .into_iter()
                .collect()
        )
    );
}

#[test]
fn remote_move_transfers_then_replaces_the_source_binding_with_an_error() {
    let value = run_ok(
        "val workload (20 22)\n\
         val Worker @(*workload) { pub val result workload }\n\
         val worker [remote Worker]\n\
         ([await worker.result] [Types.is? workload :moved_value])",
    );
    assert_eq!(
        value,
        pima::Value::List(
            [
                pima::Value::List(
                    [pima::Value::Integer(20), pima::Value::Integer(22)]
                        .into_iter()
                        .collect()
                ),
                pima::Value::Boolean(true),
            ]
            .into_iter()
            .collect()
        )
    );
}

#[test]
fn remote_move_invalidates_every_binding_linked_to_the_same_reference() {
    let value = run_ok(
        "val Service { pub val value 42 }\n\
         val service [remote Service]\n\
         val alias service\n\
         val Worker @(*alias) { pub val received alias }\n\
         val worker [remote Worker]\n\
         ([Types.is? service :moved_value] [Types.is? alias :moved_value])",
    );
    assert_eq!(
        value,
        pima::Value::List(
            [pima::Value::Boolean(true), pima::Value::Boolean(true)]
                .into_iter()
                .collect()
        )
    );
}

#[test]
fn moving_an_object_invalidates_an_extracted_bound_remote_method() {
    let value = run_ok(
        "val Service { pub function read () 42 }\n\
         val service [remote Service]\n\
         val read service.read\n\
         val Worker @(*service) { pub val received service }\n\
         val worker [remote Worker]\n\
         ([Types.is? service :moved_value] [Types.is? read :moved_value])",
    );
    assert_eq!(
        value,
        pima::Value::List(
            [pima::Value::Boolean(true), pima::Value::Boolean(true)]
                .into_iter()
                .collect()
        )
    );
}

#[test]
fn moved_value_error_records_the_move_operation_and_source_span() {
    let value = run_ok(
        "val workload (20 22)\n\
         val Worker @(*workload) { pub val result workload }\n\
         val worker [remote Worker]\n\
         (workload.move_operation workload.move_source workload.move_start workload.move_end)",
    );
    let pima::Value::List(fields) = value else {
        panic!("expected provenance fields");
    };
    let fields = fields.to_vec();
    assert_eq!(fields[0], pima::Value::String("remote construction".into()));
    assert!(matches!(fields[1], pima::Value::Integer(_)));
    assert!(matches!(fields[2], pima::Value::Integer(_)));
    assert!(matches!(fields[3], pima::Value::Integer(_)));
}

#[test]
fn failed_remote_move_leaves_the_source_binding_unchanged() {
    let value = run_ok(
        "val workload [new { pub val value 42 }]\n\
         val alias workload\n\
         val Worker @(*workload) { pub val result workload }\n\
         val failure [attempt { remote Worker }]\n\
         ([Types.is? failure :unsendable_value] workload.value alias.value)",
    );
    assert_eq!(
        value,
        pima::Value::List(
            [
                pima::Value::Boolean(true),
                pima::Value::Integer(42),
                pima::Value::Integer(42),
            ]
            .into_iter()
            .collect()
        )
    );
}

#[test]
fn failed_remote_move_preserves_vm_bound_function_aliases() {
    let value = run_ok(
        "function task () 42\n\
         val alias task\n\
         val Worker @(*task) { pub val received task }\n\
         val failure [attempt { remote Worker }]\n\
         ([Types.is? failure :unsendable_value] [task] [alias])",
    );
    assert_eq!(
        value,
        pima::Value::List(
            [
                pima::Value::Boolean(true),
                pima::Value::Integer(42),
                pima::Value::Integer(42),
            ]
            .into_iter()
            .collect()
        )
    );
}

#[test]
fn failed_remote_move_preserves_vm_bound_blocks() {
    let value = run_ok(
        "val work { 42 }\n\
         val Worker @(*work) { pub val received work }\n\
         val failure [attempt { remote Worker }]\n\
         val result do work\n\
         ([Types.is? failure :unsendable_value] result)",
    );
    assert_eq!(
        value,
        pima::Value::List(
            [pima::Value::Boolean(true), pima::Value::Integer(42)]
                .into_iter()
                .collect()
        )
    );
}

#[test]
fn unsendable_value_nested_in_a_list_fails_the_whole_move_transaction() {
    let value = run_ok(
        "function task () 42\n\
         val payload (task)\n\
         val Worker @(*payload) { pub val received payload }\n\
         val failure [attempt { remote Worker }]\n\
         val preserved [head payload]\n\
         ([Types.is? failure :unsendable_value] [preserved])",
    );
    assert_eq!(
        value,
        pima::Value::List(
            [pima::Value::Boolean(true), pima::Value::Integer(42)]
                .into_iter()
                .collect()
        )
    );
}

#[test]
fn remote_share_accepts_handles_and_rejects_local_values() {
    let value = run_ok(
        "val Service { pub val value 42 }\n\
         val service [remote Service]\n\
         val Worker @(&service) { pub function read () { await service.value } }\n\
         val worker [remote Worker]\n\
         val scalar 1\n\
         val Invalid @(&scalar) { pub val value scalar }\n\
         val failure [attempt { remote Invalid }]\n\
         ([await [worker.read]] [Types.is? failure :unsendable_value])",
    );
    assert_eq!(
        value,
        pima::Value::List(
            [pima::Value::Integer(42), pima::Value::Boolean(true)]
                .into_iter()
                .collect()
        )
    );
}

#[test]
fn remote_composition_transports_the_union_of_required_context() {
    let value = run_ok(
        "val left 3\n\
         val right 4\n\
         val Base @(right) { pub function sum () [+ left right] }\n\
         val Specific @(left) { pub val marker left }\n\
         val worker [remote (Specific Base)]\n\
         ([await worker.marker] [await [worker.sum]])",
    );
    assert_eq!(
        value,
        pima::Value::List(
            [pima::Value::Integer(3), pima::Value::Integer(7)]
                .into_iter()
                .collect()
        )
    );
}

#[test]
fn remote_rejects_unsendable_required_context() {
    let value = run_ok(
        "val local [new { pub val value 1 }]\n\
         val Worker @(local) { pub function read () local }\n\
         val error [attempt { remote Worker }]\n\
         [Types.is? error :unsendable_value]",
    );
    assert_eq!(value, pima::Value::Boolean(true));
}

#[test]
fn remote_reports_missing_required_context_before_starting_a_worker() {
    let value = run_ok(
        "val Worker @(missing) { pub val value missing }\n\
         val error [attempt { remote Worker }]\n\
         [Types.is? error :missing_context]",
    );
    assert_eq!(value, pima::Value::Boolean(true));
}

#[test]
fn remote_reads_and_calls_return_futures() {
    let value = run_ok(
        "val Worker {\n\
             pub val value 41\n\
             pub function add (amount) [+ value amount]\n\
         }\n\
         val worker [remote Worker]\n\
         val read worker.value\n\
         val call [worker.add 1]\n\
         ([Types.is? read :future] [Types.is? [read.complete?] :boolean] [await read] [await call])",
    );
    assert_eq!(
        value,
        pima::Value::List(
            [
                pima::Value::Boolean(true),
                pima::Value::Boolean(true),
                pima::Value::Integer(41),
                pima::Value::Integer(42),
            ]
            .into_iter()
            .collect()
        )
    );
}

#[test]
fn await_requires_a_future_and_remote_calls_require_transportable_arguments() {
    let local = run("val value 1\nawait value\n");
    assert!(!local.is_success());
    assert!(local.diagnostics[0].message.contains("requires a future"));

    let value = run_ok(
        "val Worker { pub function accept (value) value }\n\
         val worker [remote Worker]\n\
         val local [new { pub val value 1 }]\n\
         val error [attempt { [worker.accept local] }]\n\
         [Types.is? error :unsendable_value]",
    );
    assert_eq!(value, pima::Value::Boolean(true));
}

#[test]
fn bindings_destructure_lists_recursively() {
    assert_eq!(
        run_ok("val (x (y _)) (3 (4 5))\n(x y)"),
        pima::Value::List(
            [pima::Value::Integer(3), pima::Value::Integer(4)]
                .into_iter()
                .collect()
        )
    );
}

#[test]
fn let_destructuring_updates_mutable_bindings_atomically() {
    assert_eq!(
        run_ok(
            "var x 1\nvar y 2\n\
             let (x y) (3 4)\n\
             (x y)"
        ),
        pima::Value::List(
            [pima::Value::Integer(3), pima::Value::Integer(4)]
                .into_iter()
                .collect()
        )
    );

    let value = run_ok(
        "import \"/pima/library/standard\"\n\
         var x 1\nval y 2\n\
         val failure [attempt { let (x y) (3 4) }]\n\
         (x [Types.is? failure :mutation_error])",
    );
    assert_eq!(
        value,
        pima::Value::List(
            [pima::Value::Integer(1), pima::Value::Boolean(true)]
                .into_iter()
                .collect()
        )
    );
}

#[test]
fn match_selects_by_literal_and_exposes_captures_to_its_arm() {
    let value = run_ok(
        "val result (:ok 42)\n\
         match result (\n\
             (:ok value) { + value 1 }\n\
             (:error error) { error }\n\
         )",
    );
    assert_eq!(value, pima::Value::Integer(43));
}

#[test]
fn match_supports_nested_patterns_and_wildcards() {
    let value = run_ok(
        "match (:point (3 4)) (\n\
             (:point (3 y)) { y }\n\
             _ { 0 }\n\
         )",
    );
    assert_eq!(value, pima::Value::Integer(4));
}

#[test]
fn ordinary_pattern_literals_match_themselves() {
    let value = run_ok(
        "match \"ready\" (\n\
             \"waiting\" { 0 }\n\
             \"ready\" { 1 }\n\
             _ { 2 }\n\
         )",
    );
    assert_eq!(value, pima::Value::Integer(1));
}

#[test]
fn failed_pattern_throws_match_error() {
    let value = run_ok(
        "import \"/pima/library/standard\"\n\
         val failure [attempt { val (x y) (1) }]\n\
         [Types.is? failure :match_error]",
    );
    assert_eq!(value, pima::Value::Boolean(true));
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
fn val_creates_immutable_binding() {
    let value = run_ok("val x 10\nx");
    assert_eq!(value, pima::Value::Integer(10));
}

#[test]
fn set_is_not_a_declaration_keyword() {
    let outcome = run("set (x 10)");
    assert!(!outcome.is_success());
    assert!(
        outcome.diagnostics[0]
            .message
            .contains("unbound identifier `set`")
    );
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
    let outcome = run("val x 10\nlet x 20");
    assert!(!outcome.is_success());
}

#[test]
fn let_on_unbound_is_error() {
    let outcome = run("let x 10");
    assert!(!outcome.is_success());
}

#[test]
fn val_duplicate_is_error() {
    let outcome = run("val x 1\nval x 2");
    assert!(!outcome.is_success());
}

#[test]
fn var_duplicate_is_error() {
    let outcome = run("var x 1\nvar x 2");
    assert!(!outcome.is_success());
}

#[test]
fn shadowing_in_function_scope() {
    let value = run_ok("val x 1\nfunction f () {\n  val x 2\n  x\n}\n[f ]");
    assert_eq!(value, pima::Value::Integer(2));
}

#[test]
fn outer_binding_still_visible_after_function() {
    let value = run_ok("val x 1\nfunction f () {\n  val x 2\n  x\n}\n[f ]\nx");
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
fn native_functions_compare_by_identity() {
    assert_eq!(run_ok("[= + +]"), pima::Value::Boolean(true));
    assert_eq!(run_ok("[= + -]"), pima::Value::Boolean(false));
}

#[test]
fn mixed_numeric_equality_requires_the_same_mathematical_integer() {
    assert_eq!(run_ok("[= 42 42.0]"), pima::Value::Boolean(true));
    assert_eq!(
        run_ok("[= 9223372036854775807 9223372036854775808.0]"),
        pima::Value::Boolean(false)
    );
    assert_ne!(
        pima::Value::Integer(i64::MAX),
        pima::Value::Float(9_223_372_036_854_775_808.0)
    );
}

#[test]
fn mixed_numeric_comparison_is_exact_above_the_float_integer_range() {
    assert_eq!(
        run_ok("[< 9007199254740993 9007199254740994.0]"),
        pima::Value::Boolean(true)
    );
    assert_eq!(
        run_ok("[> 9007199254740993.0 9007199254740992]"),
        pima::Value::Boolean(false)
    );
    assert_eq!(
        run_ok("[< 9223372036854775807 9223372036854775808.0]"),
        pima::Value::Boolean(true)
    );
}

#[test]
fn mixed_numeric_comparison_handles_fractional_values_on_both_sides_of_zero() {
    assert_eq!(run_ok("[< 1 1.5]"), pima::Value::Boolean(true));
    assert_eq!(run_ok("[> -1 -1.5]"), pima::Value::Boolean(true));
    assert_eq!(run_ok("[< -2 -1.5]"), pima::Value::Boolean(true));
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
fn errors_are_never_equal_even_to_the_same_reference() {
    let value = run_ok(
        "val failure [attempt { Math.div 1 0 }]\n\
         val alias failure\n\
         ([= failure failure]\n\
          [= failure alias]\n\
          [= (failure) (failure)]\n\
          [Types.is? failure :error])",
    );
    assert_eq!(
        value,
        pima::Value::List(
            [
                pima::Value::Boolean(false),
                pima::Value::Boolean(false),
                pima::Value::Boolean(false),
                pima::Value::Boolean(true),
            ]
            .into_iter()
            .collect()
        )
    );
}

#[test]
fn user_defined_error_objects_are_never_equal() {
    let value = run_ok(
        "val Failure {\n\
             pub val types (:error :failure)\n\
             pub val message \"failed\"\n\
         }\n\
         val failure [new Failure]\n\
         ([= failure failure] [Types.is? failure :failure])",
    );
    assert_eq!(
        value,
        pima::Value::List(
            [pima::Value::Boolean(false), pima::Value::Boolean(true)]
                .into_iter()
                .collect()
        )
    );
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
fn branch_selects_the_first_true_arm() {
    assert_eq!(
        run_ok(
            "var visited 0\nbranch (\n false { let visited 1 }\n true { let visited 2\n 20 }\n true { let visited 3\n 30 }\n)\n+ visited 22"
        ),
        pima::Value::Integer(24),
    );
}

#[test]
fn branch_returns_unit_when_no_condition_matches() {
    assert_eq!(run_ok("branch (false 1 false 2)"), pima::Value::Unit);
    assert_eq!(run_ok("branch ()"), pima::Value::Unit);
}

#[test]
fn branch_conditions_share_the_current_scope() {
    assert_eq!(
        run_ok("val threshold 10\nval value 12\nbranch ([> value threshold] value true 0)"),
        pima::Value::Integer(12),
    );
}

#[test]
fn branch_results_can_be_unwrapped_expressions() {
    assert_eq!(
        run_ok(
            "val score 75\n\
             val response branch (\n\
                 [< score 60] \"fail\"\n\
                 [< score 90] \"pass\"\n\
                 true \"excellent\"\n\
             )\n\
             response",
        ),
        pima::Value::String("pass".into()),
    );
}

#[test]
fn branch_rejects_non_boolean_conditions() {
    let outcome = run("branch (1 2)");
    assert!(!outcome.is_success());
    assert!(
        outcome.diagnostics[0]
            .message
            .contains("branch condition must be a boolean")
    );
}

#[test]
fn if_true_branch() {
    assert_eq!(run_ok("if true 1 2"), pima::Value::Integer(1));
}

#[test]
fn if_false_branch() {
    assert_eq!(run_ok("if false 1 2"), pima::Value::Integer(2));
}

#[test]
fn if_without_alternative_returns_consequent_or_unit() {
    assert_eq!(run_ok("if true 42"), pima::Value::Integer(42));
    assert_eq!(run_ok("if false 42"), pima::Value::Unit);
    assert_eq!(run_ok("[if true 42]"), pima::Value::Integer(42));
    assert_eq!(run_ok("[if false 42]"), pima::Value::Unit);
}

#[test]
fn if_without_alternative_accepts_a_block_consequent() {
    assert_eq!(run_ok("if true { + 20 22 }"), pima::Value::Integer(42));
    assert_eq!(run_ok("if false { missing }"), pima::Value::Unit);
}

#[test]
fn if_with_blocks() {
    let value = run_ok("if true { 10 }{ 20 }");
    assert_eq!(value, pima::Value::Integer(10));
}

#[test]
fn single_expression_and_block_if_branches_are_equivalent() {
    assert_eq!(
        run_ok("if true [+ 20 22] 0"),
        run_ok("if true { + 20 22 } { 0 }")
    );
}

#[test]
fn immediate_and_block_branches_propagate_return_equally() {
    assert_eq!(
        run_ok(
            "function direct () {\n\
                 if true [return \"this\"] \"other\"\n\
             }\n\
             [direct ]\n"
        ),
        run_ok(
            "function blocked () {\n\
                 if true {\n\
                     return \"this\"\n\
                 } {\n\
                     \"other\"\n\
                 }\n\
             }\n\
             [blocked ]\n"
        )
    );
}

#[test]
fn selected_annotated_if_branch_enforces_its_context() {
    let outcome = run("var changed false\n\
         if true @(missing) { let changed true } { 0 }\n");
    assert!(!outcome.is_success());
    assert!(
        outcome.diagnostics[0]
            .message
            .contains("required context binding `missing` is unavailable")
    );
}

#[test]
fn unselected_annotated_if_branch_is_not_validated() {
    assert_eq!(
        run_ok("if false @(missing) { 1 } { 2 }"),
        pima::Value::Integer(2)
    );
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
    let value = run_ok("function add (x y) {\n  + x y\n}\n[add 3 4]");
    assert_eq!(value, pima::Value::Integer(7));
}

#[test]
fn function_pattern_can_capture_the_complete_argument_value() {
    let value = run_ok("function identity value value\n[identity 1 2 3]");
    assert_eq!(
        value,
        pima::Value::List(
            [
                pima::Value::Integer(1),
                pima::Value::Integer(2),
                pima::Value::Integer(3)
            ]
            .into_iter()
            .collect()
        )
    );
}

#[test]
fn function_body_can_be_a_single_non_block_expression() {
    assert_eq!(
        run_ok("function add_one (value) [+ value 1]\n[add_one 41]"),
        pima::Value::Integer(42)
    );
}

#[test]
fn function_argument_pattern_mismatch_is_typed() {
    let outcome = run("function pair (left right) left\n[pair 1]");
    assert!(!outcome.is_success());
    assert!(
        outcome.diagnostics[0]
            .message
            .contains("does not match its parameter pattern")
    );
}

#[test]
fn function_return_last_expression() {
    let value = run_ok("function greet (name) {\n  name\n}\n[greet \"world\"]");
    assert_eq!(value, pima::Value::String(std::sync::Arc::from("world")));
}

#[test]
fn explicit_return() {
    let value = run_ok("function f (x) {\n  return x\n  999\n}\n[f 5]");
    assert_eq!(value, pima::Value::Integer(5));
}

#[test]
fn bare_return_returns_unit() {
    let value = run_ok("function f () {\n  return\n  999\n}\n[f ]");
    assert_eq!(value, pima::Value::Unit);
}

#[test]
fn simple_recursion() {
    let value = run_ok(
        "function countdown (n) {\n  if [= n 0] 0 [+ 1 [countdown [- n 1]]]\n}\n[countdown 3]",
    );
    assert_eq!(value, pima::Value::Integer(3));
}

#[test]
fn wrong_arity_is_error() {
    let outcome = run("function f (x) { x }\n[f 1 2]");
    assert!(!outcome.is_success());
}

#[test]
fn zero_arg_call_with_brackets() {
    let value = run_ok("function f () { 42 }\n[f ]");
    assert_eq!(value, pima::Value::Integer(42));
}

// ── Closures ──

#[test]
fn closure_captures_environment() {
    let value = run_ok(
        "function make_adder (n) {\n  function inner (x) {\n    + x n\n  }\n}\nval add5 [make_adder 5]\n[add5 3]",
    );
    assert_eq!(value, pima::Value::Integer(8));
}

// ── Partial Application ──

#[test]
fn partial_application_with_underscore() {
    let value = run_ok("function add (x y) {\n  + x y\n}\nval add5 [add 5 _]\n[add5 10]");
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

// ── do ──

#[test]
fn do_executes_block_in_current_scope() {
    let value = run_ok("val code { x }\nval x 42\ndo code");
    assert_eq!(value, pima::Value::Integer(42));
}

#[test]
fn do_can_create_bindings_in_caller_scope() {
    let value = run_ok("val code { val y 99 }\ndo code\ny");
    assert_eq!(value, pima::Value::Integer(99));
}

#[test]
fn annotated_blocks_require_visible_context_bindings() {
    let value = run_ok(
        "val report @(name score) { score }\n\
         function render (report name score) { do report }\n\
         render report \"Ada\" 96\n",
    );
    assert_eq!(value, pima::Value::Integer(96));
}

#[test]
fn annotated_blocks_fail_before_execution_when_context_is_missing() {
    let outcome = run("var changed false\n\
         val report @(name score) { let changed true }\n\
         function render (report name) { do report }\n\
         render report \"Ada\"\n");
    assert!(!outcome.is_success());
    assert!(
        outcome.diagnostics[0]
            .message
            .contains("required context binding `score` is unavailable")
    );
}

#[test]
fn annotated_block_requirements_use_the_lexical_lookup_chain() {
    let value = run_ok(
        "val prefix \"Result: \"\n\
         val report @(prefix name) { name }\n\
         function render (report name) { do report }\n\
         render report \"Ada\"\n",
    );
    assert_eq!(value, pima::Value::String("Ada".into()));
}

// ── attempt ──

#[test]
fn attempt_catches_error() {
    let outcome = run("val result [attempt {\n  throw_error_here\n}]\nresult");
    // Should not crash - result should be bound to the error value
    assert!(outcome.is_success());
}

#[test]
fn attempt_accepts_a_non_block_expression() {
    assert_eq!(run_ok("attempt 42"), pima::Value::Integer(42));
}

#[test]
fn match_arms_accept_non_block_expressions() {
    assert_eq!(run_ok("match :ok (:ok 42)"), pima::Value::Integer(42));
}

#[test]
fn loops_accept_non_block_expressions() {
    assert_eq!(
        run_ok("var running true\nwhile running [let running false]"),
        pima::Value::Boolean(false)
    );
}

// ── Namespaces ──

#[test]
fn new_creates_namespace() {
    let value = run_ok("val Template {\n  pub val x 10\n}\nval obj [new Template]\nobj");
    assert!(matches!(value, pima::Value::Namespace(_)));
}

#[test]
fn new_enforces_annotated_block_context_requirements() {
    let value = run_ok(
        "val seed 42\n\
         val object [new @(seed) { pub val value seed }]\n\
         object.value\n",
    );
    assert_eq!(value, pima::Value::Integer(42));

    let outcome = run("new @(missing) { pub val value 1 }\n");
    assert!(!outcome.is_success());
    assert!(
        outcome.diagnostics[0]
            .message
            .contains("required context binding `missing` is unavailable")
    );
}

#[test]
fn member_access() {
    let value = run_ok("val Template {\n  pub val x 10\n}\nval obj [new Template]\nobj.x");
    assert_eq!(value, pima::Value::Integer(10));
}

#[test]
fn member_function_call() {
    let value = run_ok(
        "val Counter {\n  var v 0\n  pub function inc () {\n    let v [+ v 1]\n    v\n  }\n}\nval c [new Counter]\n[c.inc ]",
    );
    assert_eq!(value, pima::Value::Integer(1));
}

#[test]
fn this_refers_to_the_current_object_inside_methods() {
    let value = run_ok(
        "val Counter {\n\
             pub val count 42\n\
             pub function current () this\n\
             pub function read () this.count\n\
         }\n\
         val counter [new Counter]\n\
         ([= counter [counter.current]] [counter.read])",
    );
    assert_eq!(
        value,
        pima::Value::List(
            [pima::Value::Boolean(true), pima::Value::Integer(42)]
                .into_iter()
                .collect()
        )
    );
}

#[test]
fn this_is_unavailable_outside_an_object_and_cannot_be_redeclared() {
    let outcome = run("this\n");
    assert!(!outcome.is_success());
    assert!(
        outcome.diagnostics[0]
            .message
            .contains("unbound identifier `this`")
    );

    let outcome = run("val this 1\n");
    assert!(!outcome.is_success());
    assert!(outcome.diagnostics[0].message.contains("binding pattern"));
}

#[test]
fn private_member_access_is_error() {
    let outcome = run("val Template {\n  val x 10\n}\nval obj [new Template]\nobj.x");
    assert!(!outcome.is_success());
}

#[test]
fn public_mutable_members_can_be_assigned_externally() {
    let value = run_ok(
        "val counter [new { pub var count 0 }]\n\
         let counter.count 10\n\
         counter.count",
    );
    assert_eq!(value, pima::Value::Integer(10));
}

#[test]
fn methods_can_assign_private_mutable_members_through_this() {
    let value = run_ok(
        "val counter [new {\n\
             var count 0\n\
             pub function increment () { let this.count [+ this.count 1] }\n\
             pub function read () this.count\n\
         }]\n\
         counter.increment\n\
         counter.read",
    );
    assert_eq!(value, pima::Value::Integer(1));
}

#[test]
fn private_and_immutable_members_reject_assignment() {
    let value = run_ok(
        "val object [new {\n\
             var private 1\n\
             pub val fixed 2\n\
         }]\n\
         val private_failure [attempt { let object.private 3 }]\n\
         val fixed_failure [attempt { let object.fixed 3 }]\n\
         ([Types.is? private_failure :visibility_error]\n\
          [Types.is? fixed_failure :mutation_error])",
    );
    assert_eq!(
        value,
        pima::Value::List(
            [pima::Value::Boolean(true), pima::Value::Boolean(true)]
                .into_iter()
                .collect()
        )
    );
}

#[test]
fn failed_member_assignment_preserves_the_previous_value() {
    let value = run_ok(
        "val object [new { pub var value 7 }]\n\
         val failure [attempt { let object.value [Math.div 1 0] }]\n\
         object.value",
    );
    assert_eq!(value, pima::Value::Integer(7));
}

#[test]
fn invalid_member_target_is_rejected_before_evaluating_the_replacement() {
    let value = run_ok(
        "var evaluated false\n\
         val object [new { pub val fixed 7 }]\n\
         val failure [attempt {\n\
             let object.fixed do {\n\
                 let evaluated true\n\
                 9\n\
             }\n\
         }]\n\
         (evaluated object.fixed [Types.is? failure :mutation_error])",
    );
    assert_eq!(
        value,
        pima::Value::List(
            [
                pima::Value::Boolean(false),
                pima::Value::Integer(7),
                pima::Value::Boolean(true),
            ]
            .into_iter()
            .collect()
        )
    );
}

#[test]
fn namespace_independence() {
    // Two instances have independent state
    let value = run_ok(
        "val Counter {\n  var v 0\n  pub function inc () {\n    let v [+ v 1]\n  }\n  pub function get () { v }\n}\nval a [new Counter]\nval b [new Counter]\n[a.inc ]\n[a.get ]",
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
fn string_code_point_conversions_use_unicode_scalars() {
    let value = run_ok(
        "import \"/pima/library/standard\"\n\
         (\
             [String.code_point \"A\"]\
             [String.code_point \"😀\"]\
             [String.from_code_point 65]\
             [String.from_code_point 128512]\
         )\n",
    );
    assert_eq!(
        value,
        pima::Value::List(
            [
                pima::Value::Integer(65),
                pima::Value::Integer(128512),
                pima::Value::String("A".into()),
                pima::Value::String("😀".into()),
            ]
            .into_iter()
            .collect()
        )
    );
}

#[test]
fn string_code_point_conversions_reject_invalid_values() {
    let value = run_ok(
        "import \"/pima/library/standard\"\n\
         val empty [attempt { String.code_point \"\" }]\n\
         val multiple [attempt { String.code_point \"ab\" }]\n\
         val surrogate [attempt { String.from_code_point 55296 }]\n\
         val too_large [attempt { String.from_code_point 1114112 }]\n\
         (\
             [Types.is? empty :value_error]\
             [Types.is? multiple :value_error]\
             [Types.is? surrogate :value_error]\
             [Types.is? too_large :value_error]\
         )\n",
    );
    assert_eq!(
        value,
        pima::Value::List(
            [
                pima::Value::Boolean(true),
                pima::Value::Boolean(true),
                pima::Value::Boolean(true),
                pima::Value::Boolean(true),
            ]
            .into_iter()
            .collect()
        )
    );
}

#[test]
fn string_value_conversion() {
    let value = run_ok(r#"[string 42]"#);
    assert_eq!(value, pima::Value::String(std::sync::Arc::from("42")));
}

#[test]
fn string_conversion_preserves_symbol_names() {
    assert_eq!(
        run_ok("[string :foo]"),
        pima::Value::String(std::sync::Arc::from(":foo"))
    );
}

#[test]
fn string_conversion_uses_the_shared_recursive_display() {
    assert_eq!(
        run_ok(r#"[string ("hello" :world 2.0)]"#),
        pima::Value::String(std::sync::Arc::from("(hello :world 2.0)"))
    );
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

// ── Imports ──

#[test]
fn import_standard_library() {
    assert_eq!(
        run_ok("import \"/pima/library/standard\"\n[Math.pow 2 8]\n"),
        pima::Value::Integer(256)
    );
    assert_eq!(
        run_ok("import \"/pima/library/standard\"\n[String.concat \"Pi\" \"ma\"]\n"),
        pima::Value::String(std::sync::Arc::from("Pima"))
    );
}

#[test]
fn standard_library_provides_baseline_collection_and_string_utilities() {
    assert_eq!(
        run_ok(
            "import \"/pima/library/standard\"\nfunction above_two (value) { > value 2 }\nval selected [List.filter above_two (1 2 3 4)]\n[String.join [List.map String.from selected] \",\"]\n"
        ),
        pima::Value::String(std::sync::Arc::from("3,4"))
    );
    assert_eq!(
        run_ok("import \"/pima/library/standard\"\n[Math.sum (1 2 3 4)]\n"),
        pima::Value::Integer(10)
    );
    assert_eq!(
        run_ok("import \"/pima/library/standard\"\n[String.upper [String.trim \"  Pima  \"]]\n"),
        pima::Value::String(std::sync::Arc::from("PIMA"))
    );
}

#[test]
fn wildcard_namespace_import_exposes_public_members() {
    assert_eq!(
        run_ok("import \"/pima/library/standard\"\nimport Math.*\n[pow 2 10]\n"),
        pima::Value::Integer(1024)
    );
}

#[test]
fn wildcard_namespace_import_rejects_collisions_without_partial_binding() {
    let outcome = run("import \"/pima/library/standard\"\nval PI 3\nimport Math.*\n");
    assert!(!outcome.is_success());
    assert!(outcome.diagnostics[0].message.contains("collision"));
}

#[test]
fn selected_namespace_import_supports_existing_and_aliased_names() {
    assert_eq!(
        run_ok("import \"/pima/library/standard\"\nimport Logic.not\n[not false]\n"),
        pima::Value::Boolean(true)
    );
    assert_eq!(
        run_ok(
            "import \"/pima/library/standard\" as standard\nimport standard.Logic.not as negate\n[negate true]\n"
        ),
        pima::Value::Boolean(false)
    );
}

#[test]
fn selected_namespace_imports_are_live_read_only_views() {
    let outcome = run(
        "val Template {\n    pub var count 0\n    pub function bump () { let count [+ count 1] }\n}\nval counter [new Template]\nimport counter.count\n[counter.bump ]\ncount\n",
    );
    assert!(outcome.is_success(), "{:?}", outcome.diagnostics);
    assert_eq!(outcome.value, Some(pima::Value::Integer(1)));

    let outcome = run(
        "val Template { pub var count 0 }\nval counter [new Template]\nimport counter.count\nlet count 1\n",
    );
    assert!(!outcome.is_success());
    assert!(outcome.diagnostics[0].message.contains("imported binding"));
}

#[test]
fn selected_namespace_import_enforces_visibility_and_collisions() {
    let private =
        run("val Template { val hidden 1 }\nval object [new Template]\nimport object.hidden\n");
    assert!(!private.is_success());
    assert!(private.diagnostics[0].message.contains("private"));

    let collision =
        run("import \"/pima/library/standard\"\nval negate 1\nimport Logic.not as negate\n");
    assert!(!collision.is_success());
    assert!(collision.diagnostics[0].message.contains("collision"));
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

// ── Immediate call syntax ──

#[test]
fn immediate_call_basic() {
    assert_eq!(run_ok("[+ 6 7]"), pima::Value::Integer(13));
}

#[test]
fn immediate_call_nested() {
    // + [+ (1 2)] 3 = 6
    let value = run_ok("[+ [+ 1 2] 3]");
    assert_eq!(value, pima::Value::Integer(6));
}

#[test]
fn calls_pack_multiple_trailing_expressions_into_a_list() {
    assert_eq!(run_ok("[+ 6 7]"), pima::Value::Integer(13));
    assert_eq!(run_ok("+ 6 7"), pima::Value::Integer(13));
}

#[test]
fn calls_pack_one_trailing_expression_into_a_singleton_list() {
    let value = run_ok("function arguments values { values }\n[arguments 42]");
    assert_eq!(
        value,
        pima::Value::List([pima::Value::Integer(42)].into_iter().collect())
    );
}

#[test]
fn zero_arg_invocation() {
    let value = run_ok("function f () { 99 }\n[f ]");
    assert_eq!(value, pima::Value::Integer(99));
}

#[test]
fn zero_operand_line_commands_call_functions_and_return_other_values() {
    assert_eq!(
        run_ok("function answer () 42\nanswer\n"),
        pima::Value::Integer(42)
    );
    assert_eq!(run_ok("val answer 42\nanswer\n"), pima::Value::Integer(42));
    assert_eq!(run_ok("\"answer\"\n"), pima::Value::String("answer".into()));
    assert_eq!(
        run_ok(
            "val Answer { pub function read () 42 }\n\
             val answer [new Answer]\n\
             answer.read\n"
        ),
        pima::Value::Integer(42)
    );
}

#[test]
fn bracketed_callee_without_operands_receives_an_empty_list() {
    let value = run_ok("function f () { 99 }\n[f]");
    assert_eq!(value, pima::Value::Integer(99));
}

// ── Multiple statements, last value wins ──

#[test]
fn last_statement_value() {
    let value = run_ok("val x 1\nval y 2\ny");
    assert_eq!(value, pima::Value::Integer(2));
}

// ── pub declarations ──

#[test]
fn pub_val_in_namespace() {
    let value = run_ok("val T {\n  pub val value 42\n}\nval o [new T]\no.value");
    assert_eq!(value, pima::Value::Integer(42));
}

// ── Non-local control flow through do ──

#[test]
fn do_can_return_from_enclosing_function() {
    let value = run_ok("function f () {\n  do { return 99 }\n  1\n}\n[f ]");
    assert_eq!(value, pima::Value::Integer(99));
}

#[test]
fn list_elements_evaluate_left_to_right() {
    let value = run_ok("var x 0\nval values ([let x [+ x 1]] [let x [+ x 1]])\nvalues");
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
fn do_uses_the_block_origin_module() {
    let mut interpreter = Interpreter::default();
    let declaration = interpreter.run_source("<first>", "val code { 42 }\n");
    assert!(declaration.is_success(), "{:?}", declaration.diagnostics);

    let outcome = interpreter.run_source("<second>", "do code\n");
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
    let outcome = run("function add (x y) { + x y }\nval partial [add 1 _ 3]\n");
    assert!(!outcome.is_success());
}

#[test]
fn repeated_method_access_preserves_function_identity() {
    assert_eq!(
        run_ok(
            r#"val Template {
    pub function method () { 1 }
}
val instance [new Template]
[= instance.method instance.method]
"#,
        ),
        pima::Value::Boolean(true)
    );
}

#[test]
fn namespace_types_reject_duplicates_and_fundamental_types() {
    assert!(!run("val Bad { pub val types (:thing :thing) }\nnew Bad\n").is_success());
    assert!(!run("val Bad { pub val types (:integer) }\nnew Bad\n").is_success());
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
        directory.join("answer.pima"),
        "pub val answer 42\nval hidden 9\n",
    )
    .unwrap();
    let mut interpreter = Interpreter::new(Config {
        working_directory: Some(directory),
    });

    let outcome = interpreter.run_source("<test>", "import \"answer.pima\"\nanswer\n");
    assert!(outcome.is_success(), "{:?}", outcome.diagnostics);
    assert_eq!(outcome.value, Some(pima::Value::Integer(42)));
}

#[test]
fn module_aliases_share_the_cached_module() {
    let directory = module_test_directory("module-cache");
    std::fs::write(
        directory.join("identity.pima"),
        "pub function identity (value) { value }\n",
    )
    .unwrap();
    let mut interpreter = Interpreter::new(Config {
        working_directory: Some(directory),
    });

    let outcome = interpreter.run_source(
        "<test>",
        "import \"identity.pima\" as first\nimport \"identity.pima\" as second\n[= first.identity second.identity]\n",
    );
    assert!(outcome.is_success(), "{:?}", outcome.diagnostics);
    assert_eq!(outcome.value, Some(pima::Value::Boolean(true)));
}

#[test]
fn unaliased_imports_are_live_read_only_views() {
    let directory = module_test_directory("live-import");
    std::fs::write(
        directory.join("counter.pima"),
        "pub var count 0\npub function bump () { let count [+ count 1] }\n",
    )
    .unwrap();
    let mut interpreter = Interpreter::new(Config {
        working_directory: Some(directory),
    });

    let outcome = interpreter.run_source("<test>", "import \"counter.pima\"\n[bump ]\ncount\n");
    assert!(outcome.is_success(), "{:?}", outcome.diagnostics);
    assert_eq!(outcome.value, Some(pima::Value::Integer(1)));
}

#[test]
fn import_cycles_are_reported_as_pima_errors() {
    let directory = module_test_directory("module-cycle");
    std::fs::write(directory.join("a.pima"), "import \"b.pima\"\npub val a 1\n").unwrap();
    std::fs::write(directory.join("b.pima"), "import \"a.pima\"\npub val b 2\n").unwrap();
    let mut interpreter = Interpreter::new(Config {
        working_directory: Some(directory),
    });

    let outcome = interpreter.run_source("<test>", "import \"a.pima\"\n");
    assert!(!outcome.is_success());
    assert!(outcome.diagnostics[0].message.contains("import cycle"));
    let message = &outcome.diagnostics[0].message;
    let first_a = message.find("a.pima").expect("cycle should include a.pima");
    let b = message.find("b.pima").expect("cycle should include b.pima");
    let second_a = message
        .rfind("a.pima")
        .expect("cycle should close with a.pima");
    assert!(first_a < b && b < second_a, "{message}");
}

#[test]
fn runtime_diagnostics_include_origin_and_function_stack() {
    let source =
        "function inner () {\n    missing\n}\nfunction outer () {\n    [inner ]\n}\n[outer ]\n";
    let mut interpreter = Interpreter::default();
    let outcome = interpreter.run_source("<test>", source);
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
        "import \"/pima/io\" as io\n[io.write_text \"message.txt\" \"hello\"]\n[io.read_text \"message.txt\"]\n",
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
        "import \"/pima/library/standard\"\nimport \"/pima/io\" as io\nval error [attempt {\n    [io.read_text \"invalid.txt\"]\n}]\n[Types.is? error :invalid_encoding]\n",
    );

    assert!(outcome.is_success(), "{:?}", outcome.diagnostics);
    assert_eq!(outcome.value, Some(pima::Value::Boolean(true)));
}

#[test]
fn io_module_supports_a_complete_text_file_workflow() {
    let directory = module_test_directory("io-workflow");
    let mut interpreter = Interpreter::new(Config {
        working_directory: Some(directory.clone()),
    });
    let outcome = interpreter.run_source(
        "<test>",
        "import \"/pima/io\" as io\n\
         io.create_directory \"data\"\n\
         io.write_text \"data/notes.txt\" \"one\\n\"\n\
         io.append_text \"data/notes.txt\" \"two\\n\"\n\
         val lines [io.read_lines \"data/notes.txt\"]\n\
         io.copy_file \"data/notes.txt\" \"data/copy.txt\"\n\
         io.move \"data/copy.txt\" \"data/moved.txt\"\n\
         val entries [io.list_directory \"data\"]\n\
         val file [io.file? \"data/moved.txt\"]\n\
         val folder [io.directory? \"data\"]\n\
         val missing [io.exists? \"data/missing.txt\"]\n\
         io.remove_file \"data/moved.txt\"\n\
         (lines entries file folder missing)\n",
    );

    assert!(outcome.is_success(), "{:?}", outcome.diagnostics);
    let pima::Value::List(result) = outcome.value.expect("workflow returns a list") else {
        panic!("expected workflow result list");
    };
    let values = result.to_vec();
    assert_eq!(
        values[0],
        pima::Value::List(
            [
                pima::Value::String("one".into()),
                pima::Value::String("two".into()),
            ]
            .into_iter()
            .collect()
        )
    );
    assert_eq!(
        values[1],
        pima::Value::List(
            [
                pima::Value::String("moved.txt".into()),
                pima::Value::String("notes.txt".into()),
            ]
            .into_iter()
            .collect()
        )
    );
    assert_eq!(
        &values[2..],
        &[
            pima::Value::Boolean(true),
            pima::Value::Boolean(true),
            pima::Value::Boolean(false),
        ]
    );
    assert!(!directory.join("data/moved.txt").exists());
}

#[test]
fn io_path_helpers_are_platform_aware() {
    let directory = module_test_directory("io-paths");
    std::fs::create_dir_all(directory.join("folder")).unwrap();
    std::fs::write(directory.join("folder/report.txt"), "report").unwrap();
    let mut interpreter = Interpreter::new(Config {
        working_directory: Some(directory),
    });
    let outcome = interpreter.run_source(
        "<test>",
        "import \"/pima/io\" as io\n\
         val path [io.join \"folder\" \"report.txt\"]\n\
         (path [io.parent path] [io.file_name path] [io.extension path] [io.canonicalize path])\n",
    );

    assert!(outcome.is_success(), "{:?}", outcome.diagnostics);
    let pima::Value::List(values) = outcome.value.unwrap() else {
        panic!("expected path result list");
    };
    let values = values.to_vec();
    assert_eq!(values[2], pima::Value::String("report.txt".into()));
    assert_eq!(values[3], pima::Value::String("txt".into()));
    assert!(
        matches!(&values[4], pima::Value::String(path) if std::path::Path::new(path.as_ref()).is_absolute())
    );
}

#[test]
fn io_module_classifies_missing_files() {
    let directory = module_test_directory("io-missing");
    let mut interpreter = Interpreter::new(Config {
        working_directory: Some(directory),
    });
    let outcome = interpreter.run_source(
        "<test>",
        "import \"/pima/library/standard\"\n\
         import \"/pima/io\" as io\n\
         val failure [attempt { io.read_text \"missing.txt\" }]\n\
         [Types.is? failure :file_not_found]\n",
    );

    assert!(outcome.is_success(), "{:?}", outcome.diagnostics);
    assert_eq!(outcome.value, Some(pima::Value::Boolean(true)));
}

#[test]
fn io_module_round_trips_binary_data_and_validates_bytes() {
    let directory = module_test_directory("io-bytes");
    let mut interpreter = Interpreter::new(Config {
        working_directory: Some(directory.clone()),
    });
    let outcome = interpreter.run_source(
        "<test>",
        "import \"/pima/library/standard\"\n\
         import \"/pima/io\" as io\n\
         io.write_bytes \"data.bin\" (0 127 255)\n\
         io.append_bytes \"data.bin\" (42)\n\
         val bytes [io.read_bytes \"data.bin\"]\n\
         val failure [attempt { io.write_bytes \"bad.bin\" (256) }]\n\
         (bytes [Types.is? failure :value_error])\n",
    );

    assert!(outcome.is_success(), "{:?}", outcome.diagnostics);
    assert_eq!(
        std::fs::read(directory.join("data.bin")).unwrap(),
        [0, 127, 255, 42]
    );
    let pima::Value::List(result) = outcome.value.unwrap() else {
        panic!("expected binary result list");
    };
    let values = result.to_vec();
    assert_eq!(
        values[0],
        pima::Value::List(
            [
                pima::Value::Integer(0),
                pima::Value::Integer(127),
                pima::Value::Integer(255),
                pima::Value::Integer(42),
            ]
            .into_iter()
            .collect()
        )
    );
    assert_eq!(values[1], pima::Value::Boolean(true));
}

#[test]
fn imports_are_rejected_outside_module_scope() {
    let directory = module_test_directory("nested-import");
    std::fs::write(directory.join("dependency.pima"), "pub val answer 42\n").unwrap();
    let mut interpreter = Interpreter::new(Config {
        working_directory: Some(directory),
    });
    let outcome = interpreter.run_source(
        "<test>",
        "function load () {\n    import \"dependency.pima\"\n}\n[load ]\n",
    );

    assert!(!outcome.is_success());
    assert!(
        outcome.diagnostics[0]
            .message
            .contains("only at module scope")
    );
}

#[test]
fn throw_requires_a_public_immutable_string_message() {
    let outcome = run(
        "val InvalidError {\n    pub val types (:error :invalid)\n}\nval caught [attempt {\n    throw [new InvalidError]\n}]\n[is? caught :type_error]\n",
    );

    assert!(outcome.is_success(), "{:?}", outcome.diagnostics);
    assert_eq!(outcome.value, Some(pima::Value::Boolean(true)));
}

#[test]
fn continue_outside_a_loop_is_a_typed_control_flow_error() {
    let outcome =
        run("val caught [attempt {\n    continue\n}]\n[is? caught :control_flow_error]\n");

    assert!(outcome.is_success(), "{:?}", outcome.diagnostics);
    assert_eq!(outcome.value, Some(pima::Value::Boolean(true)));
}

#[test]
fn top_level_return_inside_new_is_rejected() {
    let outcome = run("new {\n    return 7\n}\n");

    assert!(!outcome.is_success());
    assert!(
        outcome.diagnostics[0]
            .message
            .contains("outside of a function"),
        "{:?}",
        outcome.diagnostics
    );
}

#[test]
fn return_inside_new_can_exit_an_enclosing_function() {
    let outcome =
        run("function construct () {\n    new {\n        return 7\n    }\n}\n[construct ]\n");

    assert!(outcome.is_success(), "{:?}", outcome.diagnostics);
    assert_eq!(outcome.value, Some(pima::Value::Integer(7)));
}

#[test]
fn failed_new_invalidates_closures_published_as_external_side_effects() {
    let value = run_ok(
        "val Failure {\n    pub val types (:error :failure)\n    pub val message \"failed\"\n}\nvar escaped ()\nval caught [attempt {\n    new {\n        function survivor () { 42 }\n        let escaped survivor\n        throw [new Failure]\n    }\n}]\n[escaped ]\n",
    );
    assert!(matches!(value, pima::Value::Namespace(_)));
    let checked = run_ok(
        "val Failure {\n    pub val types (:error :failure)\n    pub val message \"failed\"\n}\nvar escaped ()\nval caught [attempt {\n    new {\n        function survivor () { 42 }\n        let escaped survivor\n        throw [new Failure]\n    }\n}]\n([Types.is? caught :failure] [Types.is? [escaped] :invalid_object] [Types.is? [escaped].construction_error :failure])\n",
    );
    assert_eq!(
        checked,
        pima::Value::List(
            [
                pima::Value::Boolean(true),
                pima::Value::Boolean(true),
                pima::Value::Boolean(true),
            ]
            .into_iter()
            .collect()
        )
    );
}

#[test]
fn failed_new_invalidates_escaped_blocks() {
    let value = run_ok(
        "val Failure {\n    pub val types (:error :failure)\n    pub val message \"failed\"\n}\nvar escaped 0\nval caught [attempt {\n    new {\n        let escaped { 42 }\n        throw [new Failure]\n    }\n}]\nval result do escaped\nTypes.is? result :invalid_object\n",
    );
    assert_eq!(value, pima::Value::Boolean(true));
}

// ── Namespace types ──

#[test]
fn namespace_custom_types() {
    let value =
        run_ok("val Square {\n  pub val types (:square :shape)\n}\nval s [new Square]\n[types s]");
    // Should be a list containing :object, :square, :shape
    assert!(matches!(value, pima::Value::List(_)));
}

#[test]
fn is_type_on_namespace() {
    assert_eq!(
        run_ok("val T {\n  pub val types (:my_type)\n}\nval o [new T]\n[is? o :my_type]"),
        pima::Value::Boolean(true)
    );
}

// ── Member access on function returns captured namespace ──

#[test]
fn member_access_returns_bound_function() {
    // square.area should return the function with namespace env captured
    let value = run_ok(
        "val Square {\n  val w 10\n  pub function area () { w }\n}\nval s [new Square]\n[s.area ]",
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
