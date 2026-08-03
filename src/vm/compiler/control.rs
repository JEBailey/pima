use std::sync::Arc;

use crate::{
    runtime::Value,
    syntax::ast::{BranchArm, NodeId},
};

use super::{Compiler, Instruction, Register};

impl Compiler<'_> {
    pub(super) fn compile_conditional(
        &mut self,
        condition: NodeId,
        consequent: NodeId,
        alternative: Option<NodeId>,
    ) -> Option<Register> {
        let condition = self.compile_node(condition)?;
        let result = self.allocate_register();
        let branch = self.instructions.len();
        self.instructions.push(Instruction::JumpIfFalse {
            condition,
            target: usize::MAX,
            message: Arc::from("if condition must be a boolean"),
        });
        let consequent = self.compile_executable_node(consequent)?;
        self.instructions.push(Instruction::Move {
            destination: result,
            source: consequent,
        });
        let end_jump = self.instructions.len();
        self.instructions
            .push(Instruction::Jump { target: usize::MAX });
        let alternative_target = self.instructions.len();
        self.patch_jump(branch, alternative_target);
        let alternative = match alternative {
            Some(node) => self.compile_executable_node(node)?,
            None => self.load_constant(Value::Unit),
        };
        self.instructions.push(Instruction::Move {
            destination: result,
            source: alternative,
        });
        let end = self.instructions.len();
        self.patch_jump(end_jump, end);
        Some(result)
    }

    pub(super) fn compile_branch(&mut self, arms: &[BranchArm]) -> Option<Register> {
        let result = self.allocate_register();
        let unit = self.load_constant(Value::Unit);
        self.instructions.push(Instruction::Move {
            destination: result,
            source: unit,
        });
        let mut end_jumps = Vec::with_capacity(arms.len());
        for arm in arms {
            let condition = self.compile_node(arm.condition)?;
            let next_arm = self.instructions.len();
            self.instructions.push(Instruction::JumpIfFalse {
                condition,
                target: usize::MAX,
                message: Arc::from("branch condition must be a boolean"),
            });
            let value = self.compile_executable_node(arm.result)?;
            self.instructions.push(Instruction::Move {
                destination: result,
                source: value,
            });
            let end_jump = self.instructions.len();
            self.instructions
                .push(Instruction::Jump { target: usize::MAX });
            end_jumps.push(end_jump);
            let next = self.instructions.len();
            self.patch_jump(next_arm, next);
        }
        let end = self.instructions.len();
        for jump in end_jumps {
            self.patch_jump(jump, end);
        }
        Some(result)
    }

    pub(super) fn compile_attempt(&mut self, body: NodeId) -> Register {
        let destination = self.allocate_register();
        let begin = self.instructions.len();
        self.instructions.push(Instruction::BeginAttempt {
            destination,
            catch_target: usize::MAX,
        });
        self.attempt_depth += 1;
        let result = self
            .compile_executable_node(body)
            .unwrap_or_else(|| self.load_constant(Value::Unit));
        self.attempt_depth -= 1;
        self.instructions.push(Instruction::Move {
            destination,
            source: result,
        });
        self.instructions.push(Instruction::EndAttempt);
        let end_jump = self.instructions.len();
        self.instructions
            .push(Instruction::Jump { target: usize::MAX });
        let catch_target = self.instructions.len();
        let Instruction::BeginAttempt {
            catch_target: target,
            ..
        } = &mut self.instructions[begin]
        else {
            unreachable!("attempt must begin with a handler instruction")
        };
        *target = catch_target;
        let end = self.instructions.len();
        self.patch_jump(end_jump, end);
        destination
    }

    pub(super) fn patch_jump(&mut self, instruction: usize, target: usize) {
        match &mut self.instructions[instruction] {
            Instruction::Jump {
                target: jump_target,
            }
            | Instruction::JumpIfFalse {
                target: jump_target,
                ..
            }
            | Instruction::JumpIfTrue {
                target: jump_target,
                ..
            }
            | Instruction::JumpIfNotListLength {
                target: jump_target,
                ..
            }
            | Instruction::JumpIfNotEqual {
                target: jump_target,
                ..
            }
            | Instruction::JumpIfNotBlock {
                target: jump_target,
                ..
            } => *jump_target = target,
            _ => unreachable!("compiler attempted to patch a non-jump instruction"),
        }
    }
}
