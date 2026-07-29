use pima::{
    source::SourceMap,
    syntax::{
        lexer::lex,
        parser::parse,
        token::{Keyword, TokenKind},
    },
};
use proptest::prelude::*;
use std::sync::Arc;

fn lex_kinds(source: &str) -> Vec<TokenKind> {
    let mut sources = SourceMap::default();
    let source_id = sources.add("<test>", source);
    lex(source_id, source)
        .expect("source should lex")
        .into_iter()
        .map(|token| token.kind)
        .collect()
}

#[test]
fn lexes_function_symbols_calls_and_eols() {
    let kinds = lex_kinds("function add (:x :y) {\r\n+ x y\n}\n");

    assert_eq!(
        kinds,
        vec![
            TokenKind::Keyword(Keyword::Function),
            TokenKind::Identifier(Arc::from("add")),
            TokenKind::LeftParen,
            TokenKind::Symbol(Arc::from("x")),
            TokenKind::Symbol(Arc::from("y")),
            TokenKind::RightParen,
            TokenKind::LeftBrace,
            TokenKind::Eol,
            TokenKind::Identifier(Arc::from("+")),
            TokenKind::Identifier(Arc::from("x")),
            TokenKind::Identifier(Arc::from("y")),
            TokenKind::Eol,
            TokenKind::RightBrace,
            TokenKind::Eol,
            TokenKind::Eof,
        ]
    );
}

#[test]
fn distinguishes_member_dot_range_and_numbers() {
    let kinds = lex_kinds("square.area\n[.. -2 2]\n3.5\n");

    assert_eq!(
        kinds,
        vec![
            TokenKind::Identifier(Arc::from("square")),
            TokenKind::Dot,
            TokenKind::Identifier(Arc::from("area")),
            TokenKind::Eol,
            TokenKind::LeftBracket,
            TokenKind::Identifier(Arc::from("..")),
            TokenKind::Integer(-2),
            TokenKind::Integer(2),
            TokenKind::RightBracket,
            TokenKind::Eol,
            TokenKind::Float(3.5),
            TokenKind::Eol,
            TokenKind::Eof,
        ]
    );
}

#[test]
fn preserves_line_endings_inside_block_comments() {
    let kinds = lex_kinds("set x 1 /* first\nsecond */\nset y 2");
    let eol_count = kinds
        .iter()
        .filter(|kind| matches!(kind, TokenKind::Eol))
        .count();

    assert_eq!(eol_count, 2);
}

#[test]
fn accepts_documented_string_escapes() {
    let kinds = lex_kinds("\"line\\nemoji: \\u{1F600}\"");
    assert_eq!(
        kinds,
        vec![
            TokenKind::String(Arc::from("line\nemoji: 😀")),
            TokenKind::Eof
        ]
    );
}

#[test]
fn reports_unterminated_strings_with_a_span() {
    let mut sources = SourceMap::default();
    let source_id = sources.add("<test>", "\"unterminated");
    let diagnostics = lex(source_id, "\"unterminated").expect_err("lexing should fail");

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].message, "unterminated string literal");
    assert!(diagnostics[0].primary_span.is_some());
}

#[test]
fn reports_unterminated_block_comments() {
    let mut sources = SourceMap::default();
    let source_id = sources.add("<test>", "/* never closed");
    let diagnostics = lex(source_id, "/* never closed").expect_err("lexing should fail");

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].message, "unterminated block comment");
}

#[test]
fn recognizes_do_as_a_keyword() {
    let kinds = lex_kinds("do code\n");
    assert_eq!(
        kinds,
        vec![
            TokenKind::Keyword(Keyword::Do),
            TokenKind::Identifier(Arc::from("code")),
            TokenKind::Eol,
            TokenKind::Eof,
        ]
    );
}

#[test]
fn recognizes_context_annotation_punctuation() {
    let kinds = lex_kinds("@(:name) {}\n");
    assert_eq!(
        kinds,
        vec![
            TokenKind::At,
            TokenKind::LeftParen,
            TokenKind::Symbol(Arc::from("name")),
            TokenKind::RightParen,
            TokenKind::LeftBrace,
            TokenKind::RightBrace,
            TokenKind::Eol,
            TokenKind::Eof,
        ]
    );
}

proptest! {
    #[test]
    fn arbitrary_unicode_source_never_panics(source in any::<String>()) {
        let mut sources = SourceMap::default();
        let source_id = sources.add("<generated>", source.as_str());
        if let Ok(tokens) = lex(source_id, &source) {
            let _ = parse(&tokens);
        }
    }
}

#[test]
fn block_comment_with_double_star() {
    let mut sources = SourceMap::default();
    let source_id = sources.add("<test>", "/** comment */");
    let tokens = lex(source_id, "/** comment */").expect("should lex");
    // Should produce just Eof (block comment is consumed)
    assert_eq!(tokens.len(), 1);
    assert!(matches!(tokens[0].kind, TokenKind::Eof));
}
