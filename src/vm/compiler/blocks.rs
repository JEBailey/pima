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
        let mut bindings = Vec::new();
        let mut names = std::collections::HashSet::new();
        for statement in self.module.block(block).statements.clone() {
            let node = self.module.node(statement);
            match &node.kind {
                NodeKind::Binding {
                    visibility,
                    mutability: BindingKind::Immutable,
                    pattern: Pattern::Capture(name),
                    value,
                } => {
                    if !names.insert(name.text.clone()) {
                        self.diagnostics.push(Diagnostic::at_error(
                            format!("duplicate namespace binding `{name}`"),
                            name.span,
                        ));
                        continue;
                    }
                    let block_value = self.resolve_static_block(*value);
                    let source = self.compile_node(*value)?;
                    self.locals.insert(
                        name.text.clone(),
                        Local {
                            register: source,
                            block: block_value,
                            binding: false,
                        },
                    );
                    bindings.push(NamespaceBinding {
                        name: name.text.clone(),
                        source,
                        public: *visibility == Visibility::Public,
                    });
                }
                _ => self.diagnostics.push(Diagnostic::at_error(
                    "register VM namespaces currently support immutable value bindings only",
                    node.span,
                )),
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
        let Some(block) = self.resolve_static_block(operand) else {
            self.instructions.push(Instruction::RaiseTyped {
                types: vec![Arc::from("error"), Arc::from("type_error")],
                message: Arc::from(
                    "register VM cannot execute a block without static block identity",
                ),
            });
            return Some(self.load_constant(crate::runtime::Value::Unit));
        };
        if matches!(self.module.node(operand).kind, NodeKind::Identifier(_)) {
            self.compile_node(operand)?;
        }
        self.check_block_requirements(block);
        Some(self.compile_block(block))
    }

    pub(super) fn resolve_static_block(&self, operand: NodeId) -> Option<BlockId> {
        match &self.module.node(operand).kind {
            NodeKind::Block(block) => Some(*block),
            NodeKind::Identifier(name) => self.locals.get(name).and_then(|local| local.block),
            _ => None,
        }
    }

    fn check_block_requirements(&mut self, block: BlockId) {
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
