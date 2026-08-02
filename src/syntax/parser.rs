use std::{collections::HashSet, sync::Arc};

use crate::{
    diagnostic::Diagnostic,
    source::{SourceId, Span},
};

use super::{
    ast::{
        AssignmentTarget, BindingKind, Block, BlockId, BranchArm, ContextRequirement,
        ContextTransferMode, LoopKind, MatchArm, Module, Name, NamespaceImportSelection, Node,
        NodeId, NodeKind, Pattern, Visibility,
    },
    token::{Keyword, Token, TokenKind},
};

pub fn parse(tokens: &[Token]) -> Result<Module, Vec<Diagnostic>> {
    let output = parse_recovering(tokens);
    if output.diagnostics.is_empty() {
        Ok(output.module)
    } else {
        Err(output.diagnostics)
    }
}

/// Parses as much of a module as possible and preserves syntax diagnostics.
///
/// The interpreter uses [`parse`] because it must not execute malformed input.
/// Editor tooling can use this entry point to retain symbols and structure from
/// valid regions while a document is being edited.
pub fn parse_recovering(tokens: &[Token]) -> ParseOutput {
    let source = tokens
        .first()
        .map(|token| token.span.source)
        .unwrap_or(SourceId(0));
    let mut parser = Parser {
        tokens,
        position: 0,
        source,
        nodes: Vec::new(),
        blocks: Vec::new(),
        diagnostics: Vec::new(),
    };
    let statements = parser.parse_program();

    ParseOutput {
        module: Module {
            source,
            statements,
            nodes: parser.nodes,
            blocks: parser.blocks,
        },
        diagnostics: parser.diagnostics,
    }
}

#[derive(Clone, Debug)]
pub struct ParseOutput {
    pub module: Module,
    pub diagnostics: Vec<Diagnostic>,
}

struct Parser<'tokens> {
    tokens: &'tokens [Token],
    position: usize,
    source: SourceId,
    nodes: Vec<Node>,
    blocks: Vec<Block>,
    diagnostics: Vec<Diagnostic>,
}

type ParseResult<T> = Result<T, ()>;

impl Parser<'_> {
    fn parse_program(&mut self) -> Vec<NodeId> {
        let mut statements = Vec::new();
        self.skip_eols();

        while !self.at_eof() {
            if self.at(|kind| matches!(kind, TokenKind::RightBrace)) {
                self.report_here("unexpected `}`");
                self.advance();
                self.skip_eols();
                continue;
            }

            match self.parse_statement() {
                Ok(statement) => statements.push(statement),
                Err(()) => self.synchronize_statement(),
            }
            self.skip_eols();
        }
        statements
    }

    fn parse_statement(&mut self) -> ParseResult<NodeId> {
        let callee = self.parse_expression()?;
        if self.is_special_form(callee) && !self.at_statement_end() {
            self.report_here("unexpected operand after completed special form");
            return Err(());
        }
        if self.at_statement_end() {
            if self.is_special_form(callee)
                || matches!(self.node(callee).kind, NodeKind::Call { .. })
            {
                return Ok(callee);
            }
            let span = self.node(callee).span;
            let argument = self.alloc(span, NodeKind::List(Vec::new()));
            return Ok(self.alloc_call(callee, argument, false, span));
        }

        let first_argument = self.parse_expression()?;
        let mut call_span = self.join(self.node(callee).span, self.node(first_argument).span);
        let argument = self.alloc(call_span, NodeKind::List(vec![first_argument]));
        while !self.at_statement_end() {
            let expression = self.parse_expression()?;
            call_span = self.join(self.node(callee).span, self.node(expression).span);
            self.push_list_element(argument, expression);
        }

        self.nodes[argument.0 as usize].span = call_span;
        Ok(self.alloc_call(callee, argument, false, call_span))
    }

    fn parse_expression(&mut self) -> ParseResult<NodeId> {
        match self.peek_kind() {
            Some(TokenKind::Keyword(Keyword::Pub)) => self.parse_public_declaration(),
            Some(TokenKind::Keyword(Keyword::Val)) => {
                self.parse_binding(Visibility::Private, BindingKind::Immutable)
            }
            Some(TokenKind::Keyword(Keyword::Var)) => {
                self.parse_binding(Visibility::Private, BindingKind::Mutable)
            }
            Some(TokenKind::Keyword(Keyword::Let)) => self.parse_assignment(),
            Some(TokenKind::Keyword(Keyword::Match)) => self.parse_match(),
            Some(TokenKind::Keyword(Keyword::Branch)) => self.parse_branch(),
            Some(TokenKind::Keyword(Keyword::Function)) => self.parse_function(Visibility::Private),
            Some(TokenKind::Keyword(Keyword::If)) => self.parse_conditional(),
            Some(TokenKind::Keyword(Keyword::While)) => self.parse_loop(LoopKind::While),
            Some(TokenKind::Keyword(Keyword::Until)) => self.parse_loop(LoopKind::Until),
            Some(TokenKind::Keyword(Keyword::Return)) => self.parse_optional_transfer(true),
            Some(TokenKind::Keyword(Keyword::Break)) => self.parse_optional_transfer(false),
            Some(TokenKind::Keyword(Keyword::Continue)) => self.parse_continue(),
            Some(TokenKind::Keyword(Keyword::Throw)) => self.parse_throw(),
            Some(TokenKind::Keyword(Keyword::Import)) => self.parse_import(),
            Some(TokenKind::Keyword(Keyword::New)) => self.parse_new(),
            Some(TokenKind::Keyword(Keyword::Do)) => self.parse_do(),
            Some(TokenKind::Keyword(Keyword::Remote)) => self.parse_remote(),
            Some(TokenKind::Keyword(Keyword::Await)) => self.parse_await(),
            Some(TokenKind::Keyword(Keyword::Attempt)) => self.parse_attempt(),
            Some(TokenKind::At) => self.parse_annotated_block(),
            _ => self.parse_postfix(),
        }
    }

    fn parse_postfix(&mut self) -> ParseResult<NodeId> {
        let mut expression = self.parse_primary()?;

        while self.at(|kind| matches!(kind, TokenKind::Dot)) {
            self.advance();
            let (member, member_span) =
                self.expect_member_name("expected member name after `.`")?;
            let span = self.join(self.node(expression).span, member_span);
            expression = self.alloc(
                span,
                NodeKind::Member {
                    object: expression,
                    member: Name {
                        text: member,
                        span: member_span,
                    },
                },
            );
        }

        Ok(expression)
    }

    fn parse_primary(&mut self) -> ParseResult<NodeId> {
        let token = self.peek().cloned().ok_or_else(|| {
            self.report_eof("expected expression");
        })?;

        match token.kind {
            TokenKind::Boolean(value) => {
                self.advance();
                Ok(self.alloc(token.span, NodeKind::Boolean(value)))
            }
            TokenKind::Integer(value) => {
                self.advance();
                Ok(self.alloc(token.span, NodeKind::Integer(value)))
            }
            TokenKind::Float(value) => {
                self.advance();
                Ok(self.alloc(token.span, NodeKind::Float(value)))
            }
            TokenKind::String(value) => {
                self.advance();
                Ok(self.alloc(token.span, NodeKind::String(value)))
            }
            TokenKind::Symbol(value) => {
                self.advance();
                Ok(self.alloc(token.span, NodeKind::Symbol(value)))
            }
            TokenKind::Identifier(value) => {
                self.advance();
                Ok(self.alloc(token.span, NodeKind::Identifier(value)))
            }
            TokenKind::Keyword(Keyword::This) => {
                self.advance();
                Ok(self.alloc(token.span, NodeKind::Identifier(Arc::from("this"))))
            }
            TokenKind::Underscore => {
                self.advance();
                Ok(self.alloc(token.span, NodeKind::Placeholder))
            }
            TokenKind::LeftParen => self.parse_list(),
            TokenKind::LeftBracket => self.parse_immediate_call(),
            TokenKind::LeftBrace => self.parse_block_expression(),
            _ => {
                self.report_here("expected expression");
                Err(())
            }
        }
    }

    fn parse_list(&mut self) -> ParseResult<NodeId> {
        let start = self.advance().span;
        let mut elements = Vec::new();
        self.skip_eols();

        while !self.at(|kind| matches!(kind, TokenKind::RightParen)) {
            if self.at_eof() {
                self.report_eof("unterminated list; expected `)`");
                return Err(());
            }
            elements.push(self.parse_expression()?);
            self.skip_eols();
        }

        let end = self.advance().span;
        Ok(self.alloc(self.join(start, end), NodeKind::List(elements)))
    }

    fn parse_immediate_call(&mut self) -> ParseResult<NodeId> {
        let start = self.advance().span;
        self.skip_eols();

        if self.at(|kind| matches!(kind, TokenKind::RightBracket)) {
            let end = self.advance().span;
            self.diagnostics.push(Diagnostic::at_error(
                "an immediate call requires a callee",
                self.join(start, end),
            ));
            return Err(());
        }
        if self.at_eof() {
            self.report_eof("unterminated immediate call; expected `]`");
            return Err(());
        }

        let callee = self.parse_expression()?;
        self.skip_eols();
        if self.is_special_form(callee) {
            if !self.at(|kind| matches!(kind, TokenKind::RightBracket)) {
                self.report_here("unexpected operand after completed special form");
                return Err(());
            }
            self.advance();
            return Ok(callee);
        }

        let argument = self.alloc(start, NodeKind::List(Vec::new()));
        while !self.at(|kind| matches!(kind, TokenKind::RightBracket)) {
            if self.at_eof() {
                self.report_eof("unterminated immediate call; expected `]`");
                return Err(());
            }
            let expression = self.parse_expression()?;
            self.push_list_element(argument, expression);
            self.skip_eols();
            if self.is_special_form(expression)
                && !self.at(|kind| matches!(kind, TokenKind::RightBracket))
            {
                self.report_here("unexpected operand after completed special form");
                return Err(());
            }
        }

        let end = self.advance().span;
        let span = self.join(start, end);
        self.nodes[argument.0 as usize].span = span;
        Ok(self.alloc_call(callee, argument, true, span))
    }

    fn parse_block_expression(&mut self) -> ParseResult<NodeId> {
        let (block, span) = self.parse_block()?;
        Ok(self.alloc(span, NodeKind::Block(block)))
    }

    fn parse_block(&mut self) -> ParseResult<(BlockId, Span)> {
        let start =
            self.expect_simple(|kind| matches!(kind, TokenKind::LeftBrace), "expected `{`")?;
        let mut statements = Vec::new();
        self.skip_eols();

        while !self.at(|kind| matches!(kind, TokenKind::RightBrace)) {
            if self.at_eof() {
                self.report_eof("unterminated block; expected `}`");
                return Err(());
            }

            match self.parse_statement() {
                Ok(statement) => statements.push(statement),
                Err(()) => self.synchronize_statement(),
            }
            self.skip_eols();
        }

        let end = self.advance().span;
        let span = self.join(start, end);
        let id = BlockId(self.blocks.len() as u32);
        self.blocks.push(Block {
            span,
            requirements: Vec::new(),
            statements,
        });
        Ok((id, span))
    }

    fn parse_annotated_block(&mut self) -> ParseResult<NodeId> {
        let start = self.advance().span;
        self.expect_simple(
            |kind| matches!(kind, TokenKind::LeftParen),
            "expected `(` after `@`",
        )?;

        let mut requirements = Vec::new();
        let mut names = HashSet::new();
        self.skip_eols();
        while !self.at(|kind| matches!(kind, TokenKind::RightParen)) {
            if self.at_eof() {
                self.report_eof("unterminated context requirement list; expected `)`");
                return Err(());
            }
            let mode = match self.peek_kind() {
                Some(TokenKind::Identifier(marker)) if marker.as_ref() == "*" => {
                    self.advance();
                    ContextTransferMode::Move
                }
                Some(TokenKind::Ampersand) => {
                    self.advance();
                    ContextTransferMode::Share
                }
                _ => ContextTransferMode::Copy,
            };
            let token = self.peek().cloned().ok_or_else(|| {
                self.report_eof("unterminated context requirement list; expected `)`");
            })?;
            let TokenKind::Identifier(requirement) = token.kind else {
                self.report_here("context requirements must be names such as `name`");
                return Err(());
            };
            self.advance();
            if is_reserved(&requirement) {
                self.diagnostics.push(Diagnostic::at_error(
                    "reserved words cannot be context requirements",
                    token.span,
                ));
                return Err(());
            }
            if !names.insert(requirement.clone()) {
                self.diagnostics.push(Diagnostic::at_error(
                    format!("duplicate context requirement `{requirement}`"),
                    token.span,
                ));
                return Err(());
            }
            requirements.push(ContextRequirement {
                mode,
                name: Name {
                    text: requirement,
                    span: token.span,
                },
            });
            self.skip_eols();
        }
        self.advance();

        let (block, block_span) = self.parse_block()?;
        self.blocks[block.0 as usize].requirements = requirements;
        Ok(self.alloc(self.join(start, block_span), NodeKind::Block(block)))
    }

    fn parse_public_declaration(&mut self) -> ParseResult<NodeId> {
        let public = self.advance().span;
        match self.peek_kind() {
            Some(TokenKind::Keyword(Keyword::Val)) => {
                self.parse_binding_from(public, Visibility::Public, BindingKind::Immutable)
            }
            Some(TokenKind::Keyword(Keyword::Var)) => {
                self.parse_binding_from(public, Visibility::Public, BindingKind::Mutable)
            }
            Some(TokenKind::Keyword(Keyword::Function)) => {
                self.parse_function_from(public, Visibility::Public)
            }
            _ => {
                self.report_here("`pub` must prefix `val`, `var`, or `function`");
                Err(())
            }
        }
    }

    fn parse_binding(
        &mut self,
        visibility: Visibility,
        mutability: BindingKind,
    ) -> ParseResult<NodeId> {
        let start = self.peek().expect("binding token exists").span;
        self.parse_binding_from(start, visibility, mutability)
    }

    fn parse_binding_from(
        &mut self,
        start: Span,
        visibility: Visibility,
        mutability: BindingKind,
    ) -> ParseResult<NodeId> {
        self.advance();
        let pattern = self.parse_binding_pattern()?;
        if self.at_statement_end() {
            self.report_here("expected binding value");
            return Err(());
        }
        let value = self.parse_expression()?;
        let span = self.join(start, self.node(value).span);
        Ok(self.alloc(
            span,
            NodeKind::Binding {
                visibility,
                mutability,
                pattern,
                value,
            },
        ))
    }

    fn parse_assignment(&mut self) -> ParseResult<NodeId> {
        let start = self.advance().span;
        let target = if matches!(
            self.peek_kind(),
            Some(TokenKind::Identifier(_) | TokenKind::Keyword(Keyword::This))
        ) && matches!(
            self.tokens.get(self.position + 1).map(|token| &token.kind),
            Some(TokenKind::Dot)
        ) {
            AssignmentTarget::Member(self.parse_member_assignment_target()?)
        } else {
            AssignmentTarget::Pattern(self.parse_binding_pattern()?)
        };
        if self.at_statement_end() {
            self.report_here("expected assigned value");
            return Err(());
        }
        let value = self.parse_expression()?;
        Ok(self.alloc(
            self.join(start, self.node(value).span),
            NodeKind::Assignment { target, value },
        ))
    }

    fn parse_member_assignment_target(&mut self) -> ParseResult<NodeId> {
        let token = self.advance().clone();
        let name = match token.kind {
            TokenKind::Identifier(name) => name,
            TokenKind::Keyword(Keyword::This) => Arc::from("this"),
            _ => unreachable!("member assignment target starts with an object name"),
        };
        let mut object = self.alloc(token.span, NodeKind::Identifier(name));
        while self.at(|kind| matches!(kind, TokenKind::Dot)) {
            self.advance();
            let (member, member_span) =
                self.expect_member_name("expected member name after `.`")?;
            let span = self.join(self.node(object).span, member_span);
            object = self.alloc(
                span,
                NodeKind::Member {
                    object,
                    member: Name {
                        text: member,
                        span: member_span,
                    },
                },
            );
        }
        Ok(object)
    }

    fn parse_binding_pattern(&mut self) -> ParseResult<Pattern> {
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

    fn parse_pattern(&mut self) -> ParseResult<Pattern> {
        let token = self.peek().cloned().ok_or_else(|| {
            self.report_eof("expected pattern");
        })?;
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

    fn parse_match(&mut self) -> ParseResult<NodeId> {
        let start = self.advance().span;
        if self.at_statement_end() {
            self.report_here("expected value after `match`");
            return Err(());
        }
        let value = self.parse_expression()?;
        self.expect_simple(
            |kind| matches!(kind, TokenKind::LeftParen),
            "expected `(` before match arms",
        )?;
        self.skip_eols();

        let mut arms = Vec::new();
        while !self.at(|kind| matches!(kind, TokenKind::RightParen)) {
            let pattern = self.parse_pattern()?;
            self.skip_eols();
            let body = self.require_expression("expected match arm result expression")?;
            self.skip_eols();
            arms.push(MatchArm { pattern, body });
        }
        let end = self.advance().span;
        if arms.is_empty() {
            self.diagnostics.push(Diagnostic::at_error(
                "match requires at least one arm",
                self.join(start, end),
            ));
            return Err(());
        }
        Ok(self.alloc(self.join(start, end), NodeKind::Match { value, arms }))
    }

    fn parse_branch(&mut self) -> ParseResult<NodeId> {
        let start = self.advance().span;
        self.expect_simple(
            |kind| matches!(kind, TokenKind::LeftParen),
            "expected `(` before branch arms",
        )?;
        self.skip_eols();

        let mut arms = Vec::new();
        while !self.at(|kind| matches!(kind, TokenKind::RightParen)) {
            if self.at_eof() {
                self.report_eof("unterminated branch; expected `)`");
                return Err(());
            }
            let condition = self.parse_expression()?;
            self.skip_eols();
            if self.at(|kind| matches!(kind, TokenKind::RightParen)) {
                self.report_here("expected result expression after branch condition");
                return Err(());
            }
            let result = self.parse_expression()?;
            self.skip_eols();
            arms.push(BranchArm { condition, result });
        }
        let end = self.advance().span;
        Ok(self.alloc(self.join(start, end), NodeKind::Branch(arms)))
    }

    fn parse_function(&mut self, visibility: Visibility) -> ParseResult<NodeId> {
        let start = self.peek().expect("function token exists").span;
        self.parse_function_from(start, visibility)
    }

    fn parse_function_from(&mut self, start: Span, visibility: Visibility) -> ParseResult<NodeId> {
        self.expect_simple(
            |kind| matches!(kind, TokenKind::Keyword(Keyword::Function)),
            "expected `function`",
        )?;
        let (name, name_span) = self.expect_identifier("expected function name such as `add`")?;
        let parameter = self.parse_pattern()?;
        let mut captures = HashSet::new();
        if let Some(duplicate) = duplicate_capture(&parameter, &mut captures) {
            self.diagnostics.push(Diagnostic::at_error(
                format!("duplicate function parameter `{}`", duplicate.text),
                duplicate.span,
            ));
            return Err(());
        }
        if self.at_statement_end() {
            self.report_here("expected function body expression");
            return Err(());
        }
        let body = self.parse_expression()?;
        let body_span = self.node(body).span;
        Ok(self.alloc(
            self.join(start, body_span),
            NodeKind::Function {
                visibility,
                name: Name {
                    text: name,
                    span: name_span,
                },
                parameter,
                body,
            },
        ))
    }

    fn parse_conditional(&mut self) -> ParseResult<NodeId> {
        let start = self.advance().span;
        let condition = self.require_expression("expected condition after `if`")?;
        let consequent = self.require_expression("expected consequent branch")?;
        let alternative = if self.at_statement_end() {
            None
        } else {
            Some(self.parse_expression()?)
        };
        let end = alternative
            .map(|id| self.node(id).span)
            .unwrap_or_else(|| self.node(consequent).span);
        Ok(self.alloc(
            self.join(start, end),
            NodeKind::Conditional {
                condition,
                consequent,
                alternative,
            },
        ))
    }

    fn parse_loop(&mut self, kind: LoopKind) -> ParseResult<NodeId> {
        let start = self.advance().span;
        let condition = self.require_expression("expected loop condition")?;
        let body = self.require_expression("expected loop body expression")?;
        let body_span = self.node(body).span;
        Ok(self.alloc(
            self.join(start, body_span),
            NodeKind::Loop {
                kind,
                condition,
                body,
            },
        ))
    }

    fn parse_optional_transfer(&mut self, is_return: bool) -> ParseResult<NodeId> {
        let start = self.advance().span;
        let value = if self.at_statement_end() {
            None
        } else {
            Some(self.parse_expression()?)
        };
        let end = value.map(|id| self.node(id).span).unwrap_or(start);
        let kind = if is_return {
            NodeKind::Return(value)
        } else {
            NodeKind::Break(value)
        };
        Ok(self.alloc(self.join(start, end), kind))
    }

    fn parse_continue(&mut self) -> ParseResult<NodeId> {
        let span = self.advance().span;
        Ok(self.alloc(span, NodeKind::Continue))
    }

    fn parse_throw(&mut self) -> ParseResult<NodeId> {
        let start = self.advance().span;
        if self.at_statement_end() {
            self.report_here("expected error value after `throw`");
            return Err(());
        }
        let value = self.parse_expression()?;
        Ok(self.alloc(
            self.join(start, self.node(value).span),
            NodeKind::Throw(value),
        ))
    }

    fn parse_import(&mut self) -> ParseResult<NodeId> {
        let start = self.advance().span;
        if matches!(self.peek_kind(), Some(TokenKind::Identifier(_)))
            && matches!(
                self.tokens.get(self.position + 1).map(|token| &token.kind),
                Some(TokenKind::Dot)
            )
        {
            return self.parse_namespace_import(start);
        }
        let token = self.peek().cloned().ok_or_else(|| {
            self.report_eof("expected module path after `import`");
        })?;
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

    fn parse_new(&mut self) -> ParseResult<NodeId> {
        let start = self.advance().span;
        if self.at_statement_end() {
            self.report_here("expected at least one template after `new`");
            return Err(());
        }
        let mut operands = Vec::new();
        while !self.at_statement_end() {
            operands.push(self.parse_expression()?);
        }
        let end = self
            .node(*operands.last().expect("an operand was parsed"))
            .span;
        let operand = self.pack_special_operands(operands, self.join(start, end));
        Ok(self.alloc(
            self.join(start, self.node(operand).span),
            NodeKind::New(operand),
        ))
    }

    fn parse_do(&mut self) -> ParseResult<NodeId> {
        let start = self.advance().span;
        let block = self.require_expression("expected code block after `do`")?;
        Ok(self.alloc(self.join(start, self.node(block).span), NodeKind::Do(block)))
    }

    fn parse_attempt(&mut self) -> ParseResult<NodeId> {
        let start = self.advance().span;
        let body = self.require_expression("expected expression after `attempt`")?;
        Ok(self.alloc(
            self.join(start, self.node(body).span),
            NodeKind::Attempt(body),
        ))
    }

    fn parse_remote(&mut self) -> ParseResult<NodeId> {
        let start = self.advance().span;
        let expression = self.require_expression("expected expression after `remote`")?;
        Ok(self.alloc(
            self.join(start, self.node(expression).span),
            NodeKind::Remote(expression),
        ))
    }

    fn parse_await(&mut self) -> ParseResult<NodeId> {
        let start = self.advance().span;
        let operation = self.require_expression("expected future after `await`")?;
        Ok(self.alloc(
            self.join(start, self.node(operation).span),
            NodeKind::Await(operation),
        ))
    }

    fn require_expression(&mut self, message: &str) -> ParseResult<NodeId> {
        if self.at_statement_end() {
            self.report_here(message);
            Err(())
        } else {
            self.parse_expression()
        }
    }

    fn alloc_call(
        &mut self,
        callee: NodeId,
        argument: NodeId,
        immediate: bool,
        span: Span,
    ) -> NodeId {
        self.alloc(
            span,
            NodeKind::Call {
                callee,
                argument,
                immediate,
            },
        )
    }

    fn push_list_element(&mut self, list: NodeId, element: NodeId) {
        let NodeKind::List(elements) = &mut self.nodes[list.0 as usize].kind else {
            unreachable!("call argument node is always a list");
        };
        elements.push(element);
    }

    /// Packs operands for syntax forms which still distinguish one operand from
    /// an explicit list of operands.
    fn pack_special_operands(&mut self, expressions: Vec<NodeId>, span: Span) -> NodeId {
        if expressions.len() == 1 {
            expressions[0]
        } else {
            self.alloc(span, NodeKind::List(expressions))
        }
    }

    fn is_special_form(&self, id: NodeId) -> bool {
        matches!(
            self.node(id).kind,
            NodeKind::Binding { .. }
                | NodeKind::Assignment { .. }
                | NodeKind::Function { .. }
                | NodeKind::Conditional { .. }
                | NodeKind::Branch(_)
                | NodeKind::Loop { .. }
                | NodeKind::Return(_)
                | NodeKind::Break(_)
                | NodeKind::Continue
                | NodeKind::Throw(_)
                | NodeKind::Import { .. }
                | NodeKind::NamespaceImport { .. }
                | NodeKind::New(_)
                | NodeKind::Do(_)
                | NodeKind::Remote(_)
                | NodeKind::Await(_)
                | NodeKind::Attempt(_)
                | NodeKind::Match { .. }
        )
    }

    fn expect_identifier(&mut self, message: &str) -> ParseResult<(Arc<str>, Span)> {
        let token = self.peek().cloned().ok_or_else(|| {
            self.report_eof(message);
        })?;
        if let TokenKind::Identifier(name) = token.kind {
            self.advance();
            Ok((name, token.span))
        } else {
            self.report_here(message);
            Err(())
        }
    }

    fn expect_member_name(&mut self, message: &str) -> ParseResult<(Arc<str>, Span)> {
        let token = self.peek().cloned().ok_or_else(|| {
            self.report_eof(message);
        })?;
        match token.kind {
            TokenKind::Identifier(name) => {
                self.advance();
                Ok((name, token.span))
            }
            TokenKind::Keyword(keyword) => {
                self.advance();
                Ok((Arc::from(keyword.as_str()), token.span))
            }
            _ => {
                self.report_here(message);
                Err(())
            }
        }
    }

    fn expect_simple(
        &mut self,
        predicate: impl FnOnce(&TokenKind) -> bool,
        message: &str,
    ) -> ParseResult<Span> {
        if self.at(predicate) {
            Ok(self.advance().span)
        } else {
            self.report_here(message);
            Err(())
        }
    }

    fn alloc(&mut self, span: Span, kind: NodeKind) -> NodeId {
        let id = NodeId(self.nodes.len() as u32);
        self.nodes.push(Node { span, kind });
        id
    }

    fn node(&self, id: NodeId) -> &Node {
        &self.nodes[id.0 as usize]
    }

    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.position)
    }

    fn peek_kind(&self) -> Option<&TokenKind> {
        self.peek().map(|token| &token.kind)
    }

    fn previous(&self) -> &Token {
        &self.tokens[self.position - 1]
    }

    fn advance(&mut self) -> &Token {
        let position = self.position;
        if !self.at_eof() {
            self.position += 1;
        }
        &self.tokens[position]
    }

    fn at(&self, predicate: impl FnOnce(&TokenKind) -> bool) -> bool {
        self.peek_kind().is_some_and(predicate)
    }

    fn at_eof(&self) -> bool {
        matches!(self.peek_kind(), None | Some(TokenKind::Eof))
    }

    fn at_statement_end(&self) -> bool {
        matches!(
            self.peek_kind(),
            None | Some(
                TokenKind::Eof | TokenKind::Eol | TokenKind::RightBrace | TokenKind::RightBracket
            )
        )
    }

    fn skip_eols(&mut self) {
        while self.at(|kind| matches!(kind, TokenKind::Eol)) {
            self.position += 1;
        }
    }

    fn synchronize_statement(&mut self) {
        while !self.at_statement_end() {
            self.position += 1;
        }
        // A failed nested expression may return control to statement recovery
        // while positioned on its closing delimiter. Those delimiters cannot
        // terminate a top-level or block statement, so consume them to ensure
        // recovery makes progress. `}` is retained for the enclosing block.
        if self.at(|kind| matches!(kind, TokenKind::RightParen | TokenKind::RightBracket)) {
            self.position += 1;
        }
        self.skip_eols();
    }

    fn report_here(&mut self, message: impl Into<String>) {
        if let Some(token) = self.peek() {
            self.diagnostics
                .push(Diagnostic::at_error(message, token.span));
        } else {
            self.report_eof(message);
        }
    }

    fn report_eof(&mut self, message: impl Into<String>) {
        let end = self.tokens.last().map(|token| token.span.end).unwrap_or(0);
        self.diagnostics.push(Diagnostic::at_error(
            message,
            Span::new(self.source, end, end),
        ));
    }

    fn join(&self, start: Span, end: Span) -> Span {
        Span::new(self.source, start.start, end.end)
    }
}

fn is_reserved(name: &str) -> bool {
    matches!(
        name,
        "as" | "attempt"
            | "break"
            | "continue"
            | "do"
            | "function"
            | "if"
            | "import"
            | "let"
            | "match"
            | "new"
            | "pub"
            | "return"
            | "val"
            | "throw"
            | "this"
            | "until"
            | "var"
            | "while"
    )
}

fn duplicate_capture<'a>(
    pattern: &'a Pattern,
    captures: &mut HashSet<std::sync::Arc<str>>,
) -> Option<&'a Name> {
    match pattern {
        Pattern::Capture(name) => (!captures.insert(name.text.clone())).then_some(name),
        Pattern::List(elements) => elements
            .iter()
            .find_map(|element| duplicate_capture(element, captures)),
        Pattern::Wildcard | Pattern::Literal(_) => None,
    }
}
