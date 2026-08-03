use std::{collections::HashSet, sync::Arc};

use crate::syntax::{
    ast::{Name, NodeKind, Pattern},
    token::TokenKind,
};

use super::{ParseResult, Parser};

impl Parser<'_> {
    pub(super) fn parse_binding_pattern(&mut self) -> ParseResult<Pattern> {
        let token = self.peek().cloned().ok_or_else(|| {
            self.report_eof("expected binding target");
        })?;
        match token.kind {
            TokenKind::Identifier(name) => {
                self.advance();
                Ok(Pattern::Capture(Name {
                    text: name,
                    span: token.span,
                }))
            }
            TokenKind::Underscore => {
                self.advance();
                Ok(Pattern::Wildcard)
            }
            TokenKind::LeftParen => {
                self.advance();
                let mut elements = Vec::new();
                self.skip_eols();
                while !self.at(|kind| matches!(kind, TokenKind::RightParen)) {
                    if self.at_eof() {
                        self.report_eof("unterminated binding pattern; expected `)`");
                        return Err(());
                    }
                    elements.push(self.parse_binding_pattern()?);
                    self.skip_eols();
                }
                self.advance();
                Ok(Pattern::List(elements))
            }
            _ => {
                self.report_here("expected a binding name, `_`, or nested binding pattern");
                Err(())
            }
        }
    }

    pub(super) fn parse_pattern(&mut self) -> ParseResult<Pattern> {
        let token = self
            .peek()
            .cloned()
            .ok_or_else(|| self.report_eof("expected pattern"))?;
        match token.kind {
            TokenKind::Identifier(name) => {
                self.advance();
                Ok(Pattern::Capture(Name {
                    text: name,
                    span: token.span,
                }))
            }
            TokenKind::Symbol(name) => {
                self.advance();
                let literal = self.alloc(token.span, NodeKind::Symbol(name));
                Ok(Pattern::Literal(literal))
            }
            TokenKind::Boolean(_)
            | TokenKind::Integer(_)
            | TokenKind::Float(_)
            | TokenKind::String(_) => {
                let literal = self.parse_primary()?;
                Ok(Pattern::Literal(literal))
            }
            TokenKind::Underscore => {
                self.advance();
                Ok(Pattern::Wildcard)
            }
            TokenKind::LeftParen => {
                self.advance();
                let mut elements = Vec::new();
                self.skip_eols();
                while !self.at(|kind| matches!(kind, TokenKind::RightParen)) {
                    if self.at_eof() {
                        self.report_eof("unterminated list pattern; expected `)`");
                        return Err(());
                    }
                    elements.push(self.parse_pattern()?);
                    self.skip_eols();
                }
                self.advance();
                Ok(Pattern::List(elements))
            }
            _ => {
                self.report_here("expected capture name, literal, `_`, or list pattern");
                Err(())
            }
        }
    }
}

pub(super) fn duplicate_capture<'a>(
    pattern: &'a Pattern,
    captures: &mut HashSet<Arc<str>>,
) -> Option<&'a Name> {
    match pattern {
        Pattern::Capture(name) => (!captures.insert(name.text.clone())).then_some(name),
        Pattern::List(elements) => elements
            .iter()
            .find_map(|element| duplicate_capture(element, captures)),
        Pattern::Wildcard | Pattern::Literal(_) => None,
    }
}
