use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use crate::{
    diagnostic::Diagnostic,
    runtime::{ContextTransferMode, RemoteBlueprint},
    syntax::ast::{BindingKind, Module, NodeId, NodeKind, Visibility},
};

use super::{Compiler, Instruction, Register, context};
use crate::vm::ir::RemoteContextBinding;

impl Compiler<'_> {
    pub(super) fn compile_remote(
        &mut self,
        operand: NodeId,
        span: crate::source::Span,
    ) -> Option<Register> {
        let Some(templates) = self.resolve_static_blocks(operand) else {
            self.diagnostics.push(Diagnostic::at_error(
                "`remote` requires statically known object templates",
                span,
            ));
            return None;
        };
        let Some(source) = &self.source else {
            self.diagnostics.push(Diagnostic::at_error(
                "remote object source is unavailable",
                span,
            ));
            return None;
        };
        let mut template_sources = Vec::new();
        let mut public_functions = HashMap::new();
        let mut declared = HashSet::new();
        for (_, block) in &templates {
            let block_span = self.module.block(*block).span;
            let Some(block_source) = source.get(block_span.start..block_span.end) else {
                self.diagnostics.push(Diagnostic::at_error(
                    "remote object source span is invalid",
                    block_span,
                ));
                return None;
            };
            template_sources.push(block_source.to_owned());
            for statement in &self.module.block(*block).statements {
                match &self.module.node(*statement).kind {
                    NodeKind::Function {
                        visibility, name, ..
                    } if declared.insert(name.text.clone()) => {
                        public_functions
                            .insert(name.text.clone(), *visibility == Visibility::Public);
                    }
                    NodeKind::Binding { pattern, .. } => {
                        context::collect_pattern_names(pattern, &mut declared)
                    }
                    _ => {}
                }
            }
        }
        let mut context_bindings = Vec::new();
        let mut captured = HashSet::new();
        for (_, block) in &templates {
            for requirement in &self.module.block(*block).requirements {
                if declared.contains(&requirement.name.text)
                    || !captured.insert(requirement.name.text.clone())
                {
                    continue;
                }
                let Some(local) = self.locals.get(&requirement.name.text).copied() else {
                    self.instructions.push(Instruction::RaiseTyped {
                        types: vec![
                            Arc::from("error"),
                            Arc::from("name_error"),
                            Arc::from("missing_context"),
                        ],
                        message: Arc::from(format!(
                            "cannot execute block: required context binding `{}` is unavailable",
                            requirement.name.text
                        )),
                    });
                    continue;
                };
                let value = if local.binding {
                    let value = self.allocate_register();
                    self.instructions.push(Instruction::LoadBinding {
                        destination: value,
                        binding: local.register,
                        name: requirement.name.text.clone(),
                    });
                    value
                } else {
                    local.register
                };
                let mode = match requirement.mode {
                    crate::syntax::ast::ContextTransferMode::Copy => ContextTransferMode::Copy,
                    crate::syntax::ast::ContextTransferMode::Move => ContextTransferMode::Move,
                    crate::syntax::ast::ContextTransferMode::Share => ContextTransferMode::Share,
                };
                context_bindings.push(RemoteContextBinding {
                    name: requirement.name.text.clone(),
                    source: value,
                    mode,
                    move_target: (mode == ContextTransferMode::Move).then_some(local.register),
                });
            }
        }
        let blueprint_source: Arc<str> = if template_sources.len() == 1 {
            Arc::from(template_sources.pop().unwrap())
        } else {
            Arc::from(format!("({})", template_sources.join(" ")))
        };
        let preamble = self.source.as_ref().map_or_else(
            || Arc::from(""),
            |source| {
                Arc::from(
                    self.module
                        .statements
                        .iter()
                        .filter_map(|statement| {
                            let node = self.module.node(*statement);
                            is_remote_preamble_node(self.module, node)
                                .then(|| source.get(node.span.start..node.span.end))
                                .flatten()
                        })
                        .collect::<Vec<_>>()
                        .join("\n"),
                )
            },
        );
        let destination = self.allocate_register();
        self.instructions.push(Instruction::MakeRemoteNamespace {
            destination,
            blueprint: RemoteBlueprint {
                preamble,
                source: blueprint_source,
                public_functions: public_functions
                    .into_iter()
                    .filter_map(|(name, public)| public.then_some(name))
                    .collect(),
            },
            context: context_bindings,
        });
        Some(destination)
    }

    pub(super) fn compile_await(&mut self, future: NodeId) -> Option<Register> {
        let task = self.compile_node(future)?;
        let destination = self.allocate_register();
        self.instructions
            .push(Instruction::AwaitTask { destination, task });
        Some(destination)
    }
}

fn is_remote_preamble_node(module: &Module, node: &crate::syntax::ast::Node) -> bool {
    match &node.kind {
        NodeKind::Import { .. } | NodeKind::NamespaceImport { .. } | NodeKind::Function { .. } => {
            true
        }
        NodeKind::Binding {
            mutability: BindingKind::Immutable,
            value,
            ..
        } => is_remote_constant(module, *value),
        _ => false,
    }
}

fn is_remote_constant(module: &Module, node: NodeId) -> bool {
    match &module.node(node).kind {
        NodeKind::Unit
        | NodeKind::Boolean(_)
        | NodeKind::Integer(_)
        | NodeKind::Float(_)
        | NodeKind::String(_)
        | NodeKind::Symbol(_)
        | NodeKind::Block(_) => true,
        NodeKind::List(elements) => elements
            .iter()
            .all(|element| is_remote_constant(module, *element)),
        _ => false,
    }
}
