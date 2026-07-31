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
        let Some(block) = self.resolve_static_block(operand) else {
            self.diagnostics.push(Diagnostic::at_error(
                "register VM currently requires a statically known block after `new`",
                span,
            ));
            return None;
        };
        if matches!(self.module.node(operand).kind, NodeKind::Identifier(_)) {
            self.compile_node(operand)?;
        }
        self.check_block_requirements(block);
        let outer_locals = self.locals.clone();
        let inherited_blocks = self
            .locals
            .iter()
            .filter_map(|(name, local)| local.block.map(|block| (name.clone(), block)))
            .collect();
        let analysis =
            crate::vm::analysis::ScopeAnalysis::block(self.module, block, inherited_blocks);
        for name in analysis.declarations() {
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
        self.apply_scope_analysis(&analysis);

        for statement in self.module.block(block).statements.clone() {
            if let NodeKind::Binding {
                mutability,
                pattern,
                value,
                ..
            } = self.module.node(statement).kind.clone()
            {
                let mut names = Vec::new();
                pattern_names(&pattern, &mut names);
                let targets = names
                    .iter()
                    .filter_map(|name| {
                        self.locals
                            .get(name)
                            .copied()
                            .map(|local| (name.clone(), local))
                    })
                    .collect::<Vec<_>>();
                for name in &names {
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
                self.commit_binding_captures(
                    captures,
                    mutability == BindingKind::Mutable,
                    known_block,
                );
            } else {
                self.compile_node(statement)?;
            }
        }
        let mut bindings = Vec::new();
        for statement in self.module.block(block).statements.clone() {
            match &self.module.node(statement).kind {
                NodeKind::Binding {
                    visibility,
                    pattern,
                    ..
                } => collect_namespace_pattern(
                    pattern,
                    self,
                    *visibility == Visibility::Public,
                    &mut bindings,
                ),
                NodeKind::Function {
                    visibility, name, ..
                } => {
                    if let Some(local) = self.locals.get(&name.text).copied() {
                        let source = namespace_value_register(self, local, &name.text);
                        bindings.push(NamespaceBinding {
                            name: name.text.clone(),
                            source,
                            public: *visibility == Visibility::Public,
                        });
                    }
                }
                _ => {}
            }
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

fn collect_namespace_pattern(
    pattern: &Pattern,
    compiler: &mut Compiler<'_>,
    public: bool,
    bindings: &mut Vec<NamespaceBinding>,
) {
    match pattern {
        Pattern::Capture(name) => {
            if let Some(local) = compiler.locals.get(&name.text).copied() {
                let source = namespace_value_register(compiler, local, &name.text);
                bindings.push(NamespaceBinding {
                    name: name.text.clone(),
                    source,
                    public,
                });
            }
        }
        Pattern::List(patterns) => {
            for pattern in patterns {
                collect_namespace_pattern(pattern, compiler, public, bindings);
            }
        }
        Pattern::Wildcard | Pattern::Literal(_) => {}
    }
}

fn namespace_value_register(
    _compiler: &mut Compiler<'_>,
    local: Local,
    _name: &std::sync::Arc<str>,
) -> Register {
    local.register
}
