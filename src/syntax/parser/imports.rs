use crate::{
    source::Span,
    syntax::{
        ast::{Name, NamespaceImportSelection, NodeId, NodeKind},
        token::{Keyword, TokenKind},
    },
};

use super::{ParseResult, Parser};

impl Parser<'_> {
    pub(super) fn parse_import(&mut self) -> ParseResult<NodeId> {
        let start = self.advance().span;
        if matches!(self.peek_kind(), Some(TokenKind::Identifier(_)))
            && matches!(
                self.tokens.get(self.position + 1).map(|token| &token.kind),
                Some(TokenKind::Dot)
            )
        {
            return self.parse_namespace_import(start);
        }
        let token = self
            .peek()
            .cloned()
            .ok_or_else(|| self.report_eof("expected module path after `import`"))?;
        let path = match token.kind {
            TokenKind::String(path) | TokenKind::ImportPath(path) => path,
            _ => {
                self.report_here("module path must be a string or absolute virtual path");
                return Err(());
            }
        };
        self.advance();
        let alias = if self.at(|kind| matches!(kind, TokenKind::Keyword(Keyword::As))) {
            self.advance();
            Some(
                self.expect_identifier("expected alias name such as `standard` after `as`")?
                    .0,
            )
        } else {
            None
        };
        let end = self.previous().span;
        Ok(self.alloc(self.join(start, end), NodeKind::Import { path, alias }))
    }

    fn parse_namespace_import(&mut self, start: Span) -> ParseResult<NodeId> {
        let mut names = Vec::new();
        let (first, first_span) = self.expect_identifier("expected object name after `import`")?;
        names.push(Name {
            text: first,
            span: first_span,
        });
        loop {
            self.expect_simple(
                |kind| matches!(kind, TokenKind::Dot),
                "expected `.` in object import",
            )?;
            let (text, span) = self.expect_member_name("expected member name or `*` after `.`")?;
            if text.as_ref() == "*" {
                if self.at(|kind| matches!(kind, TokenKind::Dot)) {
                    self.report_here("`*` must be the final object import segment");
                    return Err(());
                }
                if self.at(|kind| matches!(kind, TokenKind::Keyword(Keyword::As))) {
                    self.report_here("wildcard object imports cannot use `as`");
                    return Err(());
                }
                return Ok(self.alloc(
                    self.join(start, span),
                    NodeKind::NamespaceImport {
                        path: names,
                        selection: NamespaceImportSelection::Wildcard(span),
                        alias: None,
                    },
                ));
            }
            let name = Name { text, span };
            if self.at(|kind| matches!(kind, TokenKind::Dot)) {
                names.push(name);
                continue;
            }
            let alias = if self.at(|kind| matches!(kind, TokenKind::Keyword(Keyword::As))) {
                self.advance();
                let (text, span) =
                    self.expect_identifier("expected alias name such as `negate` after `as`")?;
                Some(Name { text, span })
            } else {
                None
            };
            let end = alias.as_ref().map_or(name.span, |alias| alias.span);
            return Ok(self.alloc(
                self.join(start, end),
                NodeKind::NamespaceImport {
                    path: names,
                    selection: NamespaceImportSelection::Member(name),
                    alias,
                },
            ));
        }
    }
}
