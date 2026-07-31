use crate::source::Span;
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub enum TokenKind {
    Eol,
    Identifier(Arc<str>),
    Symbol(Arc<str>),
    Boolean(bool),
    Integer(i64),
    Float(f64),
    String(Arc<str>),
    ImportPath(Arc<str>),
    Underscore,
    At,
    Dot,
    LeftParen,
    RightParen,
    LeftBracket,
    RightBracket,
    LeftBrace,
    RightBrace,
    Keyword(Keyword),
    Eof,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Keyword {
    As,
    Attempt,
    Branch,
    Break,
    Continue,
    Do,
    Function,
    If,
    Import,
    Let,
    Match,
    New,
    Pub,
    Return,
    Val,
    Throw,
    Until,
    Var,
    While,
}
