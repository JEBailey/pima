use std::sync::Arc;

use logos::{Lexer, Logos};

use crate::{
    diagnostic::Diagnostic,
    source::{SourceId, Span},
};

use super::token::{Keyword, Token, TokenKind};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum LexError {
    #[default]
    UnexpectedCharacter,
    UnterminatedBlockComment,
    UnterminatedString,
    NewlineInString,
    InvalidEscape,
    InvalidUnicodeEscape,
}

impl LexError {
    fn message(self) -> &'static str {
        match self {
            Self::UnexpectedCharacter => "unexpected character",
            Self::UnterminatedBlockComment => "unterminated block comment",
            Self::UnterminatedString => "unterminated string literal",
            Self::NewlineInString => "line ending in string literal",
            Self::InvalidEscape => "invalid string escape",
            Self::InvalidUnicodeEscape => "invalid Unicode escape",
        }
    }
}

#[derive(Logos, Clone, Copy, Debug, PartialEq)]
#[logos(error = LexError)]
#[logos(skip r"[ \t\f]+")]
#[logos(skip(r"//[^\r\n]*", allow_greedy = true))]
enum RawToken {
    #[regex(r"\r\n|\r|\n")]
    Eol,

    #[regex(r"/\*", lex_block_comment, priority = 10)]
    BlockComment,

    #[token("\"", lex_string)]
    String,

    #[regex(r"-?[0-9]+\.[0-9]+", priority = 3)]
    Float,

    #[regex(r"-?[0-9]+", priority = 2)]
    Integer,

    #[regex(r":[A-Za-z_][A-Za-z0-9_?]*|:[+\-<>=!?^%*/]+", priority = 5)]
    Symbol,

    #[regex(r"(/[A-Za-z0-9_.-]+)+", priority = 4)]
    ImportPath,

    #[token("_", priority = 3)]
    Underscore,

    #[token("@")]
    At,

    #[token("&")]
    Ampersand,

    #[token("..")]
    DotDot,

    #[token(".")]
    Dot,

    #[token("(")]
    LeftParen,

    #[token(")")]
    RightParen,

    #[token("[")]
    LeftBracket,

    #[token("]")]
    RightBracket,

    #[token("{")]
    LeftBrace,

    #[token("}")]
    RightBrace,

    #[regex(r"[A-Za-z_][A-Za-z0-9_?]*")]
    Word,

    #[regex(r"[+\-<>=!?^%]+")]
    Operator,

    #[token("*")]
    Star,

    #[token("/")]
    Slash,
}

pub fn lex(source: SourceId, text: &str) -> Result<Vec<Token>, Vec<Diagnostic>> {
    let mut tokens = Vec::new();
    let mut diagnostics = Vec::new();
    let mut lexer = RawToken::lexer(text);

    while let Some(result) = lexer.next() {
        let range = lexer.span();
        let span = Span::new(source, range.start, range.end);

        match result {
            Ok(RawToken::BlockComment) => {
                push_comment_line_endings(&mut tokens, source, range.start, lexer.slice());
            }
            Ok(raw) => match token_kind(raw, lexer.slice()) {
                Ok(kind) => tokens.push(Token { kind, span }),
                Err(message) => diagnostics.push(Diagnostic::at_error(message, span)),
            },
            Err(error) => diagnostics.push(Diagnostic::at_error(error.message(), span)),
        }
    }

    tokens.push(Token {
        kind: TokenKind::Eof,
        span: Span::new(source, text.len(), text.len()),
    });

    if diagnostics.is_empty() {
        Ok(tokens)
    } else {
        Err(diagnostics)
    }
}

fn token_kind(raw: RawToken, text: &str) -> Result<TokenKind, &'static str> {
    let kind = match raw {
        RawToken::Eol => TokenKind::Eol,
        RawToken::String => TokenKind::String(decode_string(text)),
        RawToken::Float => TokenKind::Float(
            text.parse()
                .map_err(|_| "floating-point literal is out of range")?,
        ),
        RawToken::Integer => TokenKind::Integer(
            text.parse()
                .map_err(|_| "integer literal is outside the signed 64-bit range")?,
        ),
        RawToken::Symbol => TokenKind::Symbol(Arc::from(&text[1..])),
        RawToken::ImportPath => TokenKind::ImportPath(Arc::from(text)),
        RawToken::Underscore => TokenKind::Underscore,
        RawToken::At => TokenKind::At,
        RawToken::Ampersand => TokenKind::Ampersand,
        RawToken::DotDot | RawToken::Operator | RawToken::Slash | RawToken::Star => {
            TokenKind::Identifier(Arc::from(text))
        }
        RawToken::Dot => TokenKind::Dot,
        RawToken::LeftParen => TokenKind::LeftParen,
        RawToken::RightParen => TokenKind::RightParen,
        RawToken::LeftBracket => TokenKind::LeftBracket,
        RawToken::RightBracket => TokenKind::RightBracket,
        RawToken::LeftBrace => TokenKind::LeftBrace,
        RawToken::RightBrace => TokenKind::RightBrace,
        RawToken::Word => word_kind(text),
        RawToken::BlockComment => unreachable!("block comments are filtered before token mapping"),
    };
    Ok(kind)
}

fn word_kind(word: &str) -> TokenKind {
    let keyword = match word {
        "as" => Keyword::As,
        "await" => Keyword::Await,
        "attempt" => Keyword::Attempt,
        "branch" => Keyword::Branch,
        "break" => Keyword::Break,
        "continue" => Keyword::Continue,
        "do" => Keyword::Do,
        "function" => Keyword::Function,
        "if" => Keyword::If,
        "import" => Keyword::Import,
        "let" => Keyword::Let,
        "match" => Keyword::Match,
        "new" => Keyword::New,
        "remote" => Keyword::Remote,
        "pub" => Keyword::Pub,
        "return" => Keyword::Return,
        "val" => Keyword::Val,
        "throw" => Keyword::Throw,
        "this" => Keyword::This,
        "until" => Keyword::Until,
        "var" => Keyword::Var,
        "while" => Keyword::While,
        "true" => return TokenKind::Boolean(true),
        "false" => return TokenKind::Boolean(false),
        _ => return TokenKind::Identifier(Arc::from(word)),
    };
    TokenKind::Keyword(keyword)
}

fn decode_string(token: &str) -> Arc<str> {
    let body = &token[1..token.len() - 1];
    let mut output = String::with_capacity(body.len());
    let mut characters = body.chars();

    while let Some(character) = characters.next() {
        if character != '\\' {
            output.push(character);
            continue;
        }

        match characters.next().expect("the lexer validated this escape") {
            '"' => output.push('"'),
            '\\' => output.push('\\'),
            'n' => output.push('\n'),
            'r' => output.push('\r'),
            't' => output.push('\t'),
            '0' => output.push('\0'),
            'u' => {
                assert_eq!(characters.next(), Some('{'));
                let digits: String = characters
                    .by_ref()
                    .take_while(|value| *value != '}')
                    .collect();
                let value = u32::from_str_radix(&digits, 16)
                    .expect("the lexer validated this Unicode escape");
                output
                    .push(char::from_u32(value).expect("the lexer validated this Unicode scalar"));
            }
            _ => unreachable!("the lexer validated this escape"),
        }
    }

    Arc::from(output)
}

fn lex_block_comment(lexer: &mut Lexer<'_, RawToken>) -> Result<(), LexError> {
    let remainder = lexer.remainder();
    if let Some(end) = remainder.find("*/") {
        lexer.bump(end + 2);
        Ok(())
    } else {
        lexer.bump(remainder.len());
        Err(LexError::UnterminatedBlockComment)
    }
}

fn push_comment_line_endings(
    tokens: &mut Vec<Token>,
    source: SourceId,
    comment_start: usize,
    comment: &str,
) {
    let bytes = comment.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        let width = match bytes[index] {
            b'\r' if bytes.get(index + 1) == Some(&b'\n') => 2,
            b'\r' | b'\n' => 1,
            _ => {
                index += 1;
                continue;
            }
        };
        tokens.push(Token {
            kind: TokenKind::Eol,
            span: Span::new(source, comment_start + index, comment_start + index + width),
        });
        index += width;
    }
}

fn lex_string(lexer: &mut Lexer<'_, RawToken>) -> Result<(), LexError> {
    let remainder = lexer.remainder();
    let mut characters = remainder.char_indices().peekable();

    while let Some((index, character)) = characters.next() {
        match character {
            '"' => {
                lexer.bump(index + character.len_utf8());
                return Ok(());
            }
            '\r' | '\n' => {
                lexer.bump(index);
                return Err(LexError::NewlineInString);
            }
            '\\' => {
                let Some((escape_index, escape)) = characters.next() else {
                    lexer.bump(remainder.len());
                    return Err(LexError::UnterminatedString);
                };
                match escape {
                    '"' | '\\' | 'n' | 'r' | 't' | '0' => {}
                    'u' => {
                        let consumed = validate_unicode_escape(&mut characters, escape_index)?;
                        if consumed == 0 {
                            return Err(LexError::InvalidUnicodeEscape);
                        }
                    }
                    _ => {
                        lexer.bump(escape_index + escape.len_utf8());
                        return Err(LexError::InvalidEscape);
                    }
                }
            }
            _ => {}
        }
    }

    lexer.bump(remainder.len());
    Err(LexError::UnterminatedString)
}

fn validate_unicode_escape(
    characters: &mut std::iter::Peekable<std::str::CharIndices<'_>>,
    _u_index: usize,
) -> Result<usize, LexError> {
    let Some((_, '{')) = characters.next() else {
        return Err(LexError::InvalidUnicodeEscape);
    };

    let mut digits = String::new();
    for (_, character) in characters.by_ref() {
        if character == '}' {
            if digits.is_empty() {
                return Err(LexError::InvalidUnicodeEscape);
            }
            let value =
                u32::from_str_radix(&digits, 16).map_err(|_| LexError::InvalidUnicodeEscape)?;
            char::from_u32(value).ok_or(LexError::InvalidUnicodeEscape)?;
            return Ok(digits.len());
        }
        if !character.is_ascii_hexdigit() || digits.len() == 6 {
            return Err(LexError::InvalidUnicodeEscape);
        }
        digits.push(character);
    }

    Err(LexError::InvalidUnicodeEscape)
}
