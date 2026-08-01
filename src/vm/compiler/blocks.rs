use std::sync::Arc;

use crate::{
    diagnostic::Diagnostic,
    syntax::ast::{BindingKind, BlockId, NodeId, NodeKind, Pattern, Visibility},
};

use super::{Compiler, Local};
use crate::vm::ir::{Instruction, NamespaceBinding, Register};

impl Compiler<'_> {
    pub(super) fn compile_new(
        &mut self,
        operand: NodeId,
        span: crate::source::Span,
    ) -> Option<Register> {
        let Some(templates) = self.resolve_static_blocks(operand) else {
            self.diagnostics.push(Diagnostic::at_error(
                "register VM currently requires statically known templates after `new`",
                span,
            ));
            return None;
        };
        for (template, _) in &templates {
            if matches!(self.module.node(*template).kind, NodeKind::Identifier(_)) {
                self.compile_node(*template)?;
            }
        }
        let outer_locals = self.locals.clone();
        let inherited_blocks: std::collections::HashMap<Arc<str>, BlockId> = self
            .locals
            .iter()
            .filter_map(|(name, local)| local.block.map(|block| (name.clone(), block)))
            .collect();

        let mut winners = std::collections::HashMap::new();
        let mut winner_order = Vec::new();
        for (_, block) in &templates {
            for statement in &self.module.block(*block).statements {
                let mut names = Vec::new();
                match &self.module.node(*statement).kind {
                    NodeKind::Binding { pattern, .. } => pattern_names(pattern, &mut names),
                    NodeKind::Function { name, .. } => names.push(name.text.clone()),
                    _ => {}
                }
                for name in names {
                    if name.as_ref() == "types" || winners.contains_key(&name) {
                        continue;
                    }
                    winners.insert(name.clone(), (*block, *statement));
                    winner_order.push(name);
                }
            }
        }
        let has_types =
            templates.iter().any(|(_, block)| {
                self.module.block(*block).statements.iter().any(|statement| {
                matches!(
                    &self.module.node(*statement).kind,
                    NodeKind::Binding { pattern, .. } if pattern_contains_name(pattern, "types")
                )
            })
            });
        if has_types {
            winner_order.push(Arc::from("types"));
        }

        for name in winner_order {
            let known_block = winners.get(&name).and_then(|(_, statement)| {
                match &self.module.node(*statement).kind {
                    NodeKind::Binding {
                        mutability: BindingKind::Immutable,
                        value,
                        ..
                    } => self.resolve_static_block(*value),
                    _ => None,
                }
            });
            let register = self.allocate_register();
            self.binding_registers.push(register);
            self.locals.insert(
                name,
                Local {
                    register,
                    block: known_block,
                    binding: true,
                },
            );
        }
        for (_, block) in &templates {
            let analysis = crate::vm::analysis::ScopeAnalysis::block(
                self.module,
                *block,
                inherited_blocks.clone(),
            );
            self.apply_scope_analysis(&analysis);
            self.check_block_requirements(*block);
        }

        let mut type_sources = Vec::new();
        for (_, block) in templates.iter().rev() {
            for statement in self.module.block(*block).statements.clone() {
                match self.module.node(statement).kind.clone() {
                    NodeKind::Binding {
                        visibility,
                        mutability,
                        pattern,
                        value,
                    } => {
                        let mut names = Vec::new();
                        pattern_names(&pattern, &mut names);
                        let winning_names = names
                            .iter()
                            .filter(|name| {
                                name.as_ref() == "types"
                                    || winners.get(*name) == Some(&(*block, statement))
                            })
                            .cloned()
                            .collect::<std::collections::HashSet<_>>();
                        if winning_names.is_empty() {
                            continue;
                        }
                        if winning_names.contains("types")
                            && (visibility != Visibility::Public
                                || mutability != BindingKind::Immutable)
                        {
                            self.instructions.push(Instruction::RaiseTyped {
                                types: vec![Arc::from("error"), Arc::from("type_error")],
                                message: Arc::from(
                                    "object `types` must be declared with `pub val`",
                                ),
                            });
                        }
                        let targets = winning_names
                            .iter()
                            .filter_map(|name| {
                                self.locals
                                    .get(name)
                                    .copied()
                                    .map(|local| (name.clone(), local))
                            })
                            .collect::<Vec<_>>();
                        for name in &winning_names {
                            if let Some(outer) = outer_locals.get(name).copied() {
                                self.locals.insert(name.clone(), outer);
                            }
                        }
                        let known_block = (mutability == BindingKind::Immutable)
                            .then(|| self.resolve_static_block(value))
                            .flatten();
                        let source = self.compile_node(value)?;
                        for (name, target) in targets {
                            self.locals.insert(name, target);
                        }
                        let mut captures = Vec::new();
                        self.compile_capture_pattern(
                            &pattern,
                            source,
                            Arc::from("binding pattern does not match its value"),
                            &mut captures,
                        );
                        captures.retain(|(name, _)| winning_names.contains(&name.text));
                        if let Some((_, source)) = captures
                            .iter()
                            .find(|(name, _)| name.text.as_ref() == "types")
                        {
                            type_sources.push((*block, *source));
                        }
                        captures.retain(|(name, _)| name.text.as_ref() != "types");
                        self.commit_binding_captures(
                            captures,
                            mutability == BindingKind::Mutable,
                            known_block,
                        );
                    }
                    NodeKind::Function { name, .. }
                        if winners.get(&name.text) == Some(&(*block, statement)) =>
                    {
                        self.compile_node(statement)?;
                    }
                    NodeKind::Function { .. } => {}
                    _ => {
                        self.compile_node(statement)?;
                    }
                }
            }
        }
        if has_types {
            type_sources.sort_by_key(|(block, _)| {
                templates
                    .iter()
                    .position(|(_, candidate)| candidate == block)
                    .unwrap_or(usize::MAX)
            });
            let merged = self.allocate_register();
            self.instructions.push(Instruction::MergeNamespaceTypes {
                destination: merged,
                sources: type_sources.into_iter().map(|(_, source)| source).collect(),
            });
            let local = self.locals["types"];
            self.instructions.push(Instruction::Bind {
                binding: local.register,
                source: merged,
                mutable: false,
                name: Arc::from("types"),
            });
        }

        let mut bindings = Vec::new();
        for (name, (_, statement)) in &winners {
            match &self.module.node(*statement).kind {
                NodeKind::Binding { visibility, .. } | NodeKind::Function { visibility, .. } => {
                    if let Some(local) = self.locals.get(name).copied() {
                        let source = namespace_value_register(self, local, name);
                        bindings.push(NamespaceBinding {
                            name: name.clone(),
                            source,
                            public: *visibility == Visibility::Public,
                        });
                    }
                }
                _ => {}
            }
        }
        if has_types {
            let local = self.locals["types"];
            bindings.push(NamespaceBinding {
                name: Arc::from("types"),
                source: local.register,
                public: true,
            });
        }
        self.locals = outer_locals;
        let destination = self.allocate_register();
        self.instructions.push(Instruction::MakeNamespace {
            destination,
            bindings,
        });
        Some(destination)
    }

    pub(super) fn compile_do(
        &mut self,
        operand: NodeId,
        _span: crate::source::Span,
    ) -> Option<Register> {
        if let Some(block) = self.resolve_static_block(operand) {
            if matches!(self.module.node(operand).kind, NodeKind::Identifier(_)) {
                self.compile_node(operand)?;
            }
            self.check_block_requirements(block);
            return Some(self.compile_block(block));
        }

        let value = self.compile_node(operand)?;
        let result = self.allocate_register();
        let mut end_jumps = Vec::new();
        for block in self.dynamic_block_candidates() {
            let branch = self.instructions.len();
            self.instructions.push(Instruction::JumpIfNotBlock {
                source: value,
                module: self.module_index,
                block: block.0,
                target: usize::MAX,
            });
            let inherited_blocks = self
                .locals
                .iter()
                .filter_map(|(name, local)| local.block.map(|known| (name.clone(), known)))
                .collect();
            let analysis =
                crate::vm::analysis::ScopeAnalysis::block(self.module, block, inherited_blocks);
            for name in analysis.declarations() {
                if self.locals.get(&name.text).is_some_and(|local| {
                    local.binding && local.register.0 > 0 && local.register.0 <= self.capture_count
                }) {
                    let register = self.allocate_register();
                    self.binding_registers.push(register);
                    self.locals.insert(
                        name.text.clone(),
                        Local {
                            register,
                            block: analysis.static_block(&name.text),
                            binding: true,
                        },
                    );
                }
            }
            self.apply_scope_analysis(&analysis);
            self.check_block_requirements(block);
            let block_result = self.compile_block(block);
            self.instructions.push(Instruction::Move {
                destination: result,
                source: block_result,
            });
            end_jumps.push(self.instructions.len());
            self.instructions
                .push(Instruction::Jump { target: usize::MAX });
            let next = self.instructions.len();
            self.patch_jump(branch, next);
        }
        let mut context = self
            .locals
            .iter()
            .map(|(name, local)| (name.clone(), local.register))
            .collect::<Vec<_>>();
        context.sort_by(|left, right| left.0.cmp(&right.0));
        self.instructions.push(Instruction::DoDynamic {
            destination: result,
            block: value,
            context,
        });
        let end = self.instructions.len();
        for jump in end_jumps {
            self.patch_jump(jump, end);
        }
        Some(result)
    }

    fn dynamic_block_candidates(&self) -> Vec<BlockId> {
        let mut blocks = std::collections::HashSet::new();
        for node in &self.module.nodes {
            if let NodeKind::Binding { value, .. } = node.kind
                && let NodeKind::Block(block) = self.module.node(value).kind
                && self
                    .module
                    .block(block)
                    .requirements
                    .iter()
                    .all(|requirement| self.locals.contains_key(&requirement.text))
            {
                blocks.insert(block);
            }
        }
        let mut blocks = blocks.into_iter().collect::<Vec<_>>();
        blocks.sort_by_key(|block| block.0);
        blocks
    }

    pub(super) fn resolve_static_block(&self, operand: NodeId) -> Option<BlockId> {
        match &self.module.node(operand).kind {
            NodeKind::Block(block) => Some(*block),
            NodeKind::Identifier(name) => self.locals.get(name).and_then(|local| local.block),
            _ => None,
        }
    }

    pub(super) fn resolve_static_blocks(&self, operand: NodeId) -> Option<Vec<(NodeId, BlockId)>> {
        let operands = match &self.module.node(operand).kind {
            NodeKind::List(operands) => operands.clone(),
            _ => vec![operand],
        };
        if operands.is_empty() {
            return None;
        }
        operands
            .into_iter()
            .map(|operand| {
                self.resolve_static_block(operand)
                    .map(|block| (operand, block))
            })
            .collect()
    }

    pub(super) fn check_block_requirements(&mut self, block: BlockId) {
        for requirement in &self.module.block(block).requirements {
            if !self.locals.contains_key(&requirement.text) {
                self.instructions.push(Instruction::RaiseTyped {
                    types: vec![
                        Arc::from("error"),
                        Arc::from("name_error"),
                        Arc::from("missing_context"),
                    ],
                    message: Arc::from(format!(
                        "cannot execute block: required context binding `{}` is unavailable",
                        requirement.text
                    )),
                });
            }
        }
    }
}

fn pattern_names(pattern: &Pattern, names: &mut Vec<std::sync::Arc<str>>) {
    match pattern {
        Pattern::Capture(name) => names.push(name.text.clone()),
        Pattern::List(patterns) => {
            for pattern in patterns {
                pattern_names(pattern, names);
            }
        }
        Pattern::Wildcard | Pattern::Literal(_) => {}
    }
}

fn pattern_contains_name(pattern: &Pattern, expected: &str) -> bool {
    match pattern {
        Pattern::Capture(name) => name.text.as_ref() == expected,
        Pattern::List(patterns) => patterns
            .iter()
            .any(|pattern| pattern_contains_name(pattern, expected)),
        Pattern::Wildcard | Pattern::Literal(_) => false,
    }
}

fn namespace_value_register(
    _compiler: &mut Compiler<'_>,
    local: Local,
    _name: &std::sync::Arc<str>,
) -> Register {
    local.register
}
