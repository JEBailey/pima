use pima::{
    source::SourceMap,
    syntax::{
        ast::{BindingKind, ContextTransferMode, LoopKind, NodeKind, Pattern, Visibility},
        lexer::lex,
        parser::parse,
    },
};

#[test]
fn parses_destructuring_binding_and_match_patterns() {
    let module = parse_source(
        "val (x (y _)) (1 (2 3))\n\
         match (:ok 42) (\n\
             (:ok value) { value }\n\
             (_ _) { 0 }\n\
         )\n",
    );

    let NodeKind::Binding { pattern, .. } = &module.node(module.statements[0]).kind else {
        panic!("expected binding");
    };
    assert!(matches!(pattern, Pattern::List(elements) if elements.len() == 2));

    let NodeKind::Match { arms, .. } = &module.node(module.statements[1]).kind else {
        panic!("expected match");
    };
    assert_eq!(arms.len(), 2);
    assert!(matches!(
        &arms[0].pattern,
        Pattern::List(elements)
            if matches!(elements[0], Pattern::Literal(_))
                && matches!(elements[1], Pattern::Capture(_))
    ));
}

#[test]
fn parses_ordered_branch_pairs() {
    let module = parse_source("branch (false 1 [< (x 2)] { x })\n");
    let NodeKind::Branch(arms) = &module.node(module.statements[0]).kind else {
        panic!("expected branch");
    };
    assert_eq!(arms.len(), 2);
    assert!(matches!(
        module.node(arms[0].condition).kind,
        NodeKind::Boolean(false)
    ));
    assert!(matches!(
        module.node(arms[0].result).kind,
        NodeKind::Integer(1)
    ));
    assert!(matches!(
        module.node(arms[1].result).kind,
        NodeKind::Block(_)
    ));
}

#[test]
fn binding_patterns_use_bare_names_as_destinations() {
    let module = parse_source(
        "val (left right) (1 2)\n\
         let (left right) (3 4)\n",
    );
    for &statement in &module.statements {
        let pattern = match &module.node(statement).kind {
            NodeKind::Binding { pattern, .. } => pattern,
            NodeKind::Assignment {
                target: pima::syntax::ast::AssignmentTarget::Pattern(pattern),
                ..
            } => pattern,
            _ => panic!("expected binding operation"),
        };
        assert!(matches!(
            pattern,
            Pattern::List(elements)
                if matches!(elements[0], Pattern::Capture(_))
                    && matches!(elements[1], Pattern::Capture(_))
        ));
    }
}

fn parse_source(source: &str) -> pima::syntax::ast::Module {
    let mut sources = SourceMap::default();
    let source_id = sources.add("<test>", source);
    let tokens = lex(source_id, source).expect("source should lex");
    parse(&tokens).expect("source should parse")
}

#[test]
fn parses_function_name_parameters_and_body() {
    let module = parse_source(
        r#"function add (x y) {
    + (x y)
}"#,
    );

    assert_eq!(module.statements.len(), 1);
    let NodeKind::Function {
        visibility,
        name,
        parameter,
        body,
    } = &module.node(module.statements[0]).kind
    else {
        panic!("expected function declaration");
    };

    assert_eq!(*visibility, Visibility::Private);
    assert_eq!(name.as_ref(), "add");
    let pima::syntax::ast::Pattern::List(elements) = parameter else {
        panic!("expected list parameter pattern");
    };
    assert_eq!(
        elements
            .iter()
            .map(|pattern| match pattern {
                Pattern::Capture(name) => name.as_ref(),
                _ => panic!("expected capture"),
            })
            .collect::<Vec<&str>>(),
        vec!["x", "y"]
    );

    let NodeKind::Block(body) = module.node(*body).kind else {
        panic!("expected block body");
    };
    let body = module.block(body);
    assert_eq!(body.statements.len(), 1);
    let NodeKind::Call {
        argument,
        immediate,
        ..
    } = &module.node(body.statements[0]).kind
    else {
        panic!("expected body call");
    };
    assert!(!immediate);
    assert!(matches!(module.node(*argument).kind, NodeKind::List(_)));
}

#[test]
fn parses_immediate_zero_argument_member_call() {
    let module = parse_source("[square.area]\n");
    let NodeKind::Call {
        callee,
        argument,
        immediate,
    } = &module.node(module.statements[0]).kind
    else {
        panic!("expected immediate call");
    };

    assert!(*immediate);
    assert!(
        matches!(&module.node(*argument).kind, NodeKind::List(elements) if elements.is_empty())
    );
    assert!(matches!(module.node(*callee).kind, NodeKind::Member { .. }));
}

#[test]
fn parses_a_single_expression_line_as_a_zero_operand_command() {
    for source in ["serve\n", "server.serve\n", "42\n"] {
        let module = parse_source(source);
        let NodeKind::Call {
            argument,
            immediate,
            ..
        } = module.node(module.statements[0]).kind
        else {
            panic!("expected line command for {source:?}");
        };
        assert!(!immediate);
        assert!(matches!(
            &module.node(argument).kind,
            NodeKind::List(elements) if elements.is_empty()
        ));
    }
}

#[test]
fn packs_implicit_call_arguments_into_lists() {
    for source in ["[add 1 2]\n", "add 1 2\n"] {
        let module = parse_source(source);
        let NodeKind::Call { argument, .. } = module.node(module.statements[0]).kind else {
            panic!("expected call");
        };
        assert!(matches!(
            &module.node(argument).kind,
            NodeKind::List(elements) if elements.len() == 2
        ));
    }

    let module = parse_source("[add]\n");
    let NodeKind::Call { argument, .. } = module.node(module.statements[0]).kind else {
        panic!("expected immediate call");
    };
    assert!(matches!(
        &module.node(argument).kind,
        NodeKind::List(elements) if elements.is_empty()
    ));
}

#[test]
fn explicit_list_adds_one_call_argument_layer() {
    let module = parse_source("add (1 2)\n");
    let NodeKind::Call { argument, .. } = module.node(module.statements[0]).kind else {
        panic!("expected call");
    };
    assert!(matches!(
        &module.node(argument).kind,
        NodeKind::List(elements) if elements.len() == 1
            && matches!(module.node(elements[0]).kind, NodeKind::List(ref inner) if inner.len() == 2)
    ));
}

#[test]
fn packs_one_scalar_operand_into_a_singleton_list() {
    let module = parse_source("identity 42\n");
    let NodeKind::Call { argument, .. } = module.node(module.statements[0]).kind else {
        panic!("expected call");
    };
    assert!(matches!(
        &module.node(argument).kind,
        NodeKind::List(elements) if elements.len() == 1
    ));
}

#[test]
fn packs_multiple_new_operands_into_one_list() {
    let module = parse_source("[new Specialized Base]\n");
    let NodeKind::New(operand) = module.node(module.statements[0]).kind else {
        panic!("expected new expression");
    };
    assert!(matches!(
        &module.node(operand).kind,
        NodeKind::List(elements) if elements.len() == 2
    ));
}

#[test]
fn do_accepts_exactly_one_code_block_operand() {
    parse_source("do { 42 }\n");
    parse_source("[do { 42 }]\n");

    for source in ["do { 1 } { 2 }\n", "[do { 1 } { 2 }]\n"] {
        let mut sources = SourceMap::default();
        let source_id = sources.add("<test>", source);
        let tokens = lex(source_id, source).expect("source should lex");
        let diagnostics = parse(&tokens).expect_err("extra `do` operand should not parse");
        assert!(diagnostics[0].message.contains("unexpected operand"));
    }
}

#[test]
fn eol_terminates_calls_but_blocks_hold_multiple_statements() {
    let module = parse_source(
        r#"while [< (x 2)] {
    println x
    let x [+ (x 1)]
}
println "done"
"#,
    );

    assert_eq!(module.statements.len(), 2);
    let NodeKind::Loop {
        kind,
        condition,
        body,
    } = &module.node(module.statements[0]).kind
    else {
        panic!("expected while loop");
    };
    assert_eq!(*kind, LoopKind::While);
    assert!(matches!(
        module.node(*condition).kind,
        NodeKind::Call {
            immediate: true,
            ..
        }
    ));
    let NodeKind::Block(body) = module.node(*body).kind else {
        panic!("expected loop block expression");
    };
    assert_eq!(module.block(body).statements.len(), 2);
}

#[test]
fn control_bodies_accept_non_block_expressions() {
    parse_source(
        r#"while false 1
until true 2
attempt 3
match :ok (
    :ok 4
)
"#,
    );
}

#[test]
fn parses_remote_namespace_construction() {
    let module = parse_source("remote Foo");
    let NodeKind::Remote(operand) = module.node(module.statements[0]).kind else {
        panic!("expected remote expression");
    };
    assert!(matches!(module.node(operand).kind, NodeKind::Identifier(_)));

    let module = parse_source("remote (Foo Barr)");
    let NodeKind::Remote(operand) = module.node(module.statements[0]).kind else {
        panic!("expected remote expression");
    };
    assert!(matches!(module.node(operand).kind, NodeKind::List(_)));
}

#[test]
fn parses_await_expression() {
    let module = parse_source("await pending");
    let NodeKind::Await(operation) = module.node(module.statements[0]).kind else {
        panic!("expected await expression");
    };
    assert!(matches!(
        module.node(operation).kind,
        NodeKind::Identifier(_)
    ));
}

#[test]
fn keyword_names_are_valid_member_selectors() {
    let module = parse_source("Remote.new");
    let NodeKind::Call { callee, .. } = module.node(module.statements[0]).kind else {
        panic!("expected member command");
    };
    assert!(matches!(
        &module.node(callee).kind,
        NodeKind::Member { member, .. } if member.text.as_ref() == "new"
    ));
}

#[test]
fn parses_member_assignment_targets() {
    let module = parse_source("let this.state.count 1\n");
    let pima::syntax::ast::NodeKind::Assignment { target, .. } =
        &module.node(module.statements[0]).kind
    else {
        panic!("expected assignment");
    };
    assert!(matches!(
        target,
        pima::syntax::ast::AssignmentTarget::Member(_)
    ));
}

#[test]
fn control_transfers_consume_at_most_one_expression() {
    for source in [
        "return value extra",
        "break value extra",
        "throw value extra",
    ] {
        let mut sources = SourceMap::default();
        let source_id = sources.add("<test>", source);
        let tokens = lex(source_id, source).expect("source should lex");
        let diagnostics = parse(&tokens).expect_err("extra operand should not parse");
        assert!(diagnostics[0].message.contains("unexpected operand"));
    }

    parse_source("function f () { return [calculate value] }");
    parse_source("function f () { return (1 2) }");
}

#[test]
fn parses_public_types_binding_and_constructor_expression() {
    let module = parse_source(
        r#"val Account {
    pub val types (:account)
}
val account [new Account]
"#,
    );

    let NodeKind::Binding {
        visibility,
        mutability,
        value,
        ..
    } = &module.node(module.statements[0]).kind
    else {
        panic!("expected template binding");
    };
    assert_eq!(*visibility, Visibility::Private);
    assert_eq!(*mutability, BindingKind::Immutable);

    let NodeKind::Block(block) = module.node(*value).kind else {
        panic!("expected template block");
    };
    let NodeKind::Binding { visibility, .. } = &module.node(module.block(block).statements[0]).kind
    else {
        panic!("expected public types binding");
    };
    assert_eq!(*visibility, Visibility::Public);

    let NodeKind::Binding { value, .. } = module.node(module.statements[1]).kind else {
        panic!("expected account binding");
    };
    assert!(matches!(module.node(value).kind, NodeKind::New(_)));
}

#[test]
fn parses_bare_import_path_and_alias() {
    let module = parse_source("import /pima/library/standard as standard\n");
    let NodeKind::Import { path, alias } = &module.node(module.statements[0]).kind else {
        panic!("expected import");
    };

    assert_eq!(path.as_ref(), "/pima/library/standard");
    assert_eq!(alias.as_deref(), Some("standard"));
}

#[test]
fn rejects_symbol_import_aliases() {
    for source in [
        "import /pima/library/standard as :standard\n",
        "import Logic.not as :negate\n",
    ] {
        let mut sources = SourceMap::default();
        let source_id = sources.add("<test>", source);
        let tokens = lex(source_id, source).expect("source should lex");
        let diagnostics = parse(&tokens).expect_err("symbol alias should not parse");
        assert!(diagnostics[0].message.contains("alias name"));
    }
}

#[test]
fn rejects_symbols_in_unambiguous_name_positions() {
    for source in [
        "val :answer 42\n",
        "var :answer 42\n",
        "let :answer 42\n",
        "function :answer () 42\n",
        "val report @(:answer) {}\n",
    ] {
        let mut sources = SourceMap::default();
        let source_id = sources.add("<test>", source);
        let tokens = lex(source_id, source).expect("source should lex");
        parse(&tokens).expect_err("quoted symbol should not be accepted as a name");
    }
}

#[test]
fn parses_static_namespace_import() {
    let module = parse_source("import Math.*\n");
    let NodeKind::NamespaceImport {
        path,
        selection,
        alias,
    } = &module.node(module.statements[0]).kind
    else {
        panic!("expected static namespace import");
    };
    assert_eq!(path[0].text.as_ref(), "Math");
    assert!(matches!(
        selection,
        pima::syntax::ast::NamespaceImportSelection::Wildcard(_)
    ));
    assert!(alias.is_none());
}

#[test]
fn parses_selected_nested_namespace_import_with_alias() {
    let module = parse_source("import standard.Logic.not as negate\n");
    let NodeKind::NamespaceImport {
        path,
        selection,
        alias,
    } = &module.node(module.statements[0]).kind
    else {
        panic!("expected namespace import");
    };
    assert_eq!(
        path.iter()
            .map(|name| name.text.as_ref())
            .collect::<Vec<_>>(),
        ["standard", "Logic"]
    );
    let pima::syntax::ast::NamespaceImportSelection::Member(member) = selection else {
        panic!("expected selected member");
    };
    assert_eq!(member.text.as_ref(), "not");
    assert_eq!(
        alias.as_ref().map(|name| name.text.as_ref()),
        Some("negate")
    );
}

#[test]
fn rejects_alias_on_wildcard_namespace_import() {
    let mut sources = SourceMap::default();
    let source_id = sources.add("<test>", "import Logic.* as logical\n");
    let tokens = lex(source_id, "import Logic.* as logical\n").expect("source should lex");
    let diagnostics = parse(&tokens).expect_err("wildcard alias should not parse");
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("cannot use `as`"))
    );
}

#[test]
fn accepts_bare_function_parameters() {
    let mut sources = SourceMap::default();
    let source_id = sources.add("<test>", "function add (x y) {}\n");
    let tokens = lex(source_id, "function add (x y) {}\n").expect("source should lex");
    parse(&tokens).expect("bare names are valid function parameter captures");
}

#[test]
fn rejects_duplicate_function_parameters() {
    let mut sources = SourceMap::default();
    let source_id = sources.add("<test>", "function add (x x) {}\n");
    let tokens = lex(source_id, "function add (x x) {}\n").expect("source should lex");
    let diagnostics = parse(&tokens).expect_err("source should not parse");

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("duplicate function parameter"))
    );
}

#[test]
fn parses_do_as_a_caller_scoped_special_form() {
    let module = parse_source(
        r#"val code {
    println "hello"
}
do code
[do code]
"#,
    );

    assert_eq!(module.statements.len(), 3);
    assert!(matches!(
        module.node(module.statements[1]).kind,
        NodeKind::Do(_)
    ));
    assert!(matches!(
        module.node(module.statements[2]).kind,
        NodeKind::Do(_)
    ));
}

#[test]
fn parses_annotated_block_context_requirements() {
    let module = parse_source(
        "val report @(name *score &service) {\n\
             Console.println (name score)\n\
         }\n",
    );
    let NodeKind::Binding { value, .. } = module.node(module.statements[0]).kind else {
        panic!("expected block binding");
    };
    let NodeKind::Block(block_id) = module.node(value).kind else {
        panic!("expected annotated block");
    };
    let requirements = &module.block(block_id).requirements;
    assert_eq!(
        requirements
            .iter()
            .map(|requirement| requirement.name.text.as_ref())
            .collect::<Vec<&str>>(),
        ["name", "score", "service"]
    );
    assert_eq!(requirements[0].mode, ContextTransferMode::Copy);
    assert_eq!(requirements[1].mode, ContextTransferMode::Move);
    assert_eq!(requirements[2].mode, ContextTransferMode::Share);
}

#[test]
fn preserves_editor_spans_for_declared_names() {
    let source = "function add (left right) {\n    val total [+ (left right)]\n    total\n}\n";
    let module = parse_source(source);
    let NodeKind::Function {
        name,
        parameter,
        body,
        ..
    } = &module.node(module.statements[0]).kind
    else {
        panic!("expected function");
    };

    assert_eq!(&source[name.span.start..name.span.end], "add");
    let pima::syntax::ast::Pattern::List(elements) = parameter else {
        panic!("expected list parameter pattern");
    };
    assert_eq!(
        elements
            .iter()
            .map(|parameter| match parameter {
                Pattern::Capture(name) => &source[name.span.start..name.span.end],
                _ => panic!("expected capture"),
            })
            .collect::<Vec<_>>(),
        ["left", "right"]
    );

    let NodeKind::Block(body) = module.node(*body).kind else {
        panic!("expected block body");
    };
    let NodeKind::Binding { pattern, .. } = &module.node(module.block(body).statements[0]).kind
    else {
        panic!("expected binding");
    };
    let Pattern::Capture(total) = pattern else {
        panic!("expected capture");
    };
    assert_eq!(&source[total.span.start..total.span.end], "total");
}

#[test]
fn rejects_duplicate_annotated_block_requirements() {
    let mut sources = SourceMap::default();
    let text = "val report @(name name) {}\n";
    let source_id = sources.add("<test>", text);
    let tokens = lex(source_id, text).expect("source should lex");
    let diagnostics = parse(&tokens).expect_err("source should not parse");
    assert!(
        diagnostics[0]
            .message
            .contains("duplicate context requirement")
    );
}

#[test]
fn parses_all_examples_and_demos() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    for directory in [root.join("examples"), root.join("demos")] {
        for entry in std::fs::read_dir(directory).expect("source directory should exist") {
            let path = entry.expect("example entry should be readable").path();
            if path.extension().and_then(std::ffi::OsStr::to_str) != Some("pima") {
                continue;
            }

            let source = std::fs::read_to_string(&path).expect("example should be readable");
            let mut sources = SourceMap::default();
            let source_id = sources.add(path.display().to_string(), source.as_str());
            let tokens = lex(source_id, &source)
                .unwrap_or_else(|errors| panic!("{} failed to lex: {errors:#?}", path.display()));
            parse(&tokens)
                .unwrap_or_else(|errors| panic!("{} failed to parse: {errors:#?}", path.display()));
        }
    }
}
