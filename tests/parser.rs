use pima::{
    source::SourceMap,
    syntax::{
        ast::{BindingKind, LoopKind, NodeKind, Visibility},
        lexer::lex,
        parser::parse,
    },
};

fn parse_source(source: &str) -> pima::syntax::ast::Module {
    let mut sources = SourceMap::default();
    let source_id = sources.add("<test>", source);
    let tokens = lex(source_id, source).expect("source should lex");
    parse(&tokens).expect("source should parse")
}

#[test]
fn parses_symbol_parameters_and_function_body() {
    let module = parse_source(
        r#"function add (:x :y) {
    + x y
}"#,
    );

    assert_eq!(module.statements.len(), 1);
    let NodeKind::Function {
        visibility,
        name,
        parameters,
        body,
    } = &module.node(module.statements[0]).kind
    else {
        panic!("expected function declaration");
    };

    assert_eq!(*visibility, Visibility::Private);
    assert_eq!(name.as_ref(), "add");
    assert_eq!(
        parameters.iter().map(AsRef::as_ref).collect::<Vec<&str>>(),
        vec!["x", "y"]
    );

    let body = module.block(*body);
    assert_eq!(body.statements.len(), 1);
    let NodeKind::Call {
        arguments,
        immediate,
        ..
    } = &module.node(body.statements[0]).kind
    else {
        panic!("expected body call");
    };
    assert!(!immediate);
    assert_eq!(arguments.len(), 2);
}

#[test]
fn parses_immediate_zero_argument_member_call() {
    let module = parse_source("[square.area]\n");
    let NodeKind::Call {
        callee,
        arguments,
        immediate,
    } = &module.node(module.statements[0]).kind
    else {
        panic!("expected immediate call");
    };

    assert!(*immediate);
    assert!(arguments.is_empty());
    assert!(matches!(module.node(*callee).kind, NodeKind::Member { .. }));
}

#[test]
fn eol_terminates_calls_but_blocks_hold_multiple_statements() {
    let module = parse_source(
        r#"while [< x 2] {
    println x
    let x [+ x 1]
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
    assert_eq!(module.block(*body).statements.len(), 2);
}

#[test]
fn parses_public_types_binding_and_constructor_expression() {
    let module = parse_source(
        r#"set Account {
    pub set types (:account)
}
set account [new Account]
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
    let module = parse_source("import /po/library/standard as standard\n");
    let NodeKind::Import { path, alias } = &module.node(module.statements[0]).kind else {
        panic!("expected import");
    };

    assert_eq!(path.as_ref(), "/po/library/standard");
    assert_eq!(alias.as_deref(), Some("standard"));
}

#[test]
fn rejects_bare_function_parameters() {
    let mut sources = SourceMap::default();
    let source_id = sources.add("<test>", "function add (x y) {}\n");
    let tokens = lex(source_id, "function add (x y) {}\n").expect("source should lex");
    let diagnostics = parse(&tokens).expect_err("source should not parse");

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("must be symbols"))
    );
}

#[test]
fn rejects_duplicate_function_parameters() {
    let mut sources = SourceMap::default();
    let source_id = sources.add("<test>", "function add (:x :x) {}\n");
    let tokens = lex(source_id, "function add (:x :x) {}\n").expect("source should lex");
    let diagnostics = parse(&tokens).expect_err("source should not parse");

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("duplicate parameter"))
    );
}

#[test]
fn parses_eval_as_a_caller_scoped_special_form() {
    let module = parse_source(
        r#"set code {
    println "hello"
}
eval code
[eval code]
"#,
    );

    assert_eq!(module.statements.len(), 3);
    assert!(matches!(
        module.node(module.statements[1]).kind,
        NodeKind::Eval(_)
    ));
    assert!(matches!(
        module.node(module.statements[2]).kind,
        NodeKind::Eval(_)
    ));
}

#[test]
fn parses_all_non_java_examples() {
    let examples = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("examples");

    for entry in std::fs::read_dir(examples).expect("examples directory should exist") {
        let path = entry.expect("example entry should be readable").path();
        if path.extension().and_then(std::ffi::OsStr::to_str) != Some("po")
            || path.file_name().and_then(std::ffi::OsStr::to_str) == Some("java_support.po")
        {
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
