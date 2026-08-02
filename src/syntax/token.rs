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
    Ampersand,
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
    Await,
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
    Remote,
    Pub,
    Return,
    Val,
    Throw,
    This,
    Until,
    Var,
    While,
}

impl Keyword {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::As => "as",
            Self::Await => "await",
            Self::Attempt => "attempt",
            Self::Branch => "branch",
            Self::Break => "break",
            Self::Continue => "continue",
            Self::Do => "do",
            Self::Function => "function",
            Self::If => "if",
            Self::Import => "import",
            Self::Let => "let",
            Self::Match => "match",
            Self::New => "new",
            Self::Remote => "remote",
            Self::Pub => "pub",
            Self::Return => "return",
            Self::Val => "val",
            Self::Throw => "throw",
            Self::This => "this",
            Self::Until => "until",
            Self::Var => "var",
            Self::While => "while",
        }
    }
}
