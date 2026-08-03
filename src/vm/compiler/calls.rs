use crate::{
    diagnostic::Diagnostic,
    syntax::ast::{NodeId, NodeKind},
};

use super::{Compiler, Instruction, Register};
use crate::vm::ir::Primitive;

impl Compiler<'_> {
    pub(super) fn compile_call(
        &mut self,
        callee: NodeId,
        argument: NodeId,
        immediate: bool,
        span: crate::source::Span,
    ) -> Option<Register> {
        let command = !immediate
            && matches!(&self.module.node(argument).kind, NodeKind::List(values) if values.is_empty());
        if let NodeKind::Identifier(name) = &self.module.node(callee).kind
            && matches!(
                name.as_ref(),
                "+" | "-" | "*" | "/" | "div" | "mod" | "<" | ">" | "="
            )
        {
            return self.compile_primitive_call(callee, argument, span);
        }
        let callee = self.compile_node(callee)?;
        let argument = self.compile_call_argument(argument)?;
        let destination = self.allocate_register();
        self.instructions.push(Instruction::CallDynamic {
            destination,
            callee,
            argument,
            command,
        });
        Some(destination)
    }

    fn compile_call_argument(&mut self, argument: NodeId) -> Option<Register> {
        let NodeKind::List(arguments) = &self.module.node(argument).kind else {
            return self.compile_node(argument);
        };
        let arguments = arguments.clone();
        let elements = arguments
            .into_iter()
            .filter_map(|argument| match &self.module.node(argument).kind {
                NodeKind::Identifier(name) => self.locals.get(name).map(|local| local.register),
                _ => self.compile_node(argument),
            })
            .collect();
        let destination = self.allocate_register();
        self.instructions.push(Instruction::MakeArguments {
            destination,
            elements,
        });
        Some(destination)
    }

    fn compile_primitive_call(
        &mut self,
        callee: NodeId,
        argument: NodeId,
        span: crate::source::Span,
    ) -> Option<Register> {
        let NodeKind::Identifier(name) = &self.module.node(callee).kind else {
            self.diagnostics.push(Diagnostic::at_error(
                "register VM currently supports direct primitive calls only",
                span,
            ));
            return None;
        };
        let primitive = match name.as_ref() {
            "+" => Primitive::Add,
            "-" => Primitive::Subtract,
            "*" => Primitive::Multiply,
            "/" => Primitive::Divide,
            "div" => Primitive::IntegerDivide,
            "mod" => Primitive::Modulo,
            "<" => Primitive::LessThan,
            ">" => Primitive::GreaterThan,
            "=" => Primitive::Equal,
            _ => {
                self.diagnostics.push(Diagnostic::at_error(
                    format!("primitive `{name}` is not supported by the register VM yet"),
                    span,
                ));
                return None;
            }
        };
        let NodeKind::List(arguments) = &self.module.node(argument).kind else {
            self.diagnostics.push(Diagnostic::at_error(
                "register VM primitive arguments must currently be a list",
                self.module.node(argument).span,
            ));
            return None;
        };
        let arguments = arguments
            .iter()
            .filter_map(|argument| self.compile_node(*argument))
            .collect();
        let destination = self.allocate_register();
        self.instructions.push(Instruction::CallPrimitive {
            destination,
            primitive,
            arguments,
        });
        Some(destination)
    }
}
