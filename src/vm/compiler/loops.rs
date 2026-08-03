use std::sync::Arc;

use crate::{
    runtime::Value,
    syntax::ast::{LoopKind, NodeId},
};

use super::{Compiler, Instruction, LoopContext, Register};

impl Compiler<'_> {
    pub(super) fn compile_loop(
        &mut self,
        kind: LoopKind,
        condition: NodeId,
        body: NodeId,
    ) -> Option<Register> {
        let result = self.allocate_register();
        let unit = self.load_constant(Value::Unit);
        self.instructions.push(Instruction::Move {
            destination: result,
            source: unit,
        });
        let condition_target = self.instructions.len();
        let condition = self.compile_node(condition)?;
        let exit_jump = self.instructions.len();
        self.instructions.push(match kind {
            LoopKind::While => Instruction::JumpIfFalse {
                condition,
                target: usize::MAX,
                message: Arc::from("loop condition must be a boolean"),
            },
            LoopKind::Until => Instruction::JumpIfTrue {
                condition,
                target: usize::MAX,
                message: Arc::from("loop condition must be a boolean"),
            },
        });
        self.loops.push(LoopContext {
            continue_target: condition_target,
            break_jumps: Vec::new(),
            result,
            attempt_depth: self.attempt_depth,
        });
        let body = self.compile_executable_node(body)?;
        self.instructions.push(Instruction::Move {
            destination: result,
            source: body,
        });
        self.instructions.push(Instruction::Jump {
            target: condition_target,
        });
        let exit = self.instructions.len();
        self.patch_jump(exit_jump, exit);
        let loop_context = self.loops.pop().expect("loop context should exist");
        for jump in loop_context.break_jumps {
            self.patch_jump(jump, exit);
        }
        Some(result)
    }

    pub(super) fn compile_break(
        &mut self,
        value: Option<NodeId>,
        _span: crate::source::Span,
    ) -> Option<Register> {
        let Some((result, loop_attempt_depth)) = self
            .loops
            .last()
            .map(|context| (context.result, context.attempt_depth))
        else {
            self.instructions.push(Instruction::RaiseTyped {
                types: vec![Arc::from("error"), Arc::from("control_flow_error")],
                message: Arc::from("break outside of a loop"),
            });
            return Some(self.load_constant(Value::Unit));
        };
        let value = match value {
            Some(value) => self.compile_node(value)?,
            None => self.load_constant(Value::Unit),
        };
        self.instructions.push(Instruction::Move {
            destination: result,
            source: value,
        });
        self.end_attempts(self.attempt_depth - loop_attempt_depth);
        let jump = self.instructions.len();
        self.instructions
            .push(Instruction::Jump { target: usize::MAX });
        self.loops
            .last_mut()
            .expect("loop context should exist")
            .break_jumps
            .push(jump);
        Some(result)
    }

    pub(super) fn compile_continue(&mut self, _span: crate::source::Span) -> Option<Register> {
        let Some(context) = self.loops.last() else {
            self.instructions.push(Instruction::RaiseTyped {
                types: vec![Arc::from("error"), Arc::from("control_flow_error")],
                message: Arc::from("continue outside of a loop"),
            });
            return Some(self.load_constant(Value::Unit));
        };
        let (result, target, loop_attempt_depth) = (
            context.result,
            context.continue_target,
            context.attempt_depth,
        );
        self.end_attempts(self.attempt_depth - loop_attempt_depth);
        self.instructions.push(Instruction::Jump { target });
        Some(result)
    }

    pub(super) fn end_attempts(&mut self, count: usize) {
        self.instructions
            .extend(std::iter::repeat_n(Instruction::EndAttempt, count));
    }
}
