use std::{collections::HashSet, sync::Arc};

use crate::syntax::ast::{BlockId, MatchArm, Name, NodeId, Pattern};

use super::{Compiler, Local};
use crate::vm::ir::{Instruction, Register};

impl Compiler<'_> {
    pub(super) fn compile_parameter_pattern(&mut self, pattern: &Pattern, source: Register) {
        match pattern {
            Pattern::Wildcard => {}
            Pattern::Capture(name) => {
                self.locals.insert(
                    name.text.clone(),
                    Local {
                        register: source,
                        block: None,
                        binding: false,
                    },
                );
            }
            Pattern::List(patterns) => {
                self.instructions.push(Instruction::CheckListLength {
                    source,
                    length: patterns.len() as u16,
                    message: Arc::from("function argument does not match its parameter pattern"),
                });
                for (index, pattern) in patterns.iter().enumerate() {
                    let element = self.allocate_register();
                    self.instructions.push(Instruction::ListGet {
                        destination: element,
                        source,
                        index: index as u16,
                    });
                    self.compile_parameter_pattern(pattern, element);
                }
            }
            Pattern::Literal(literal) => {
                let Some(expected) = self.compile_node(*literal) else {
                    return;
                };
                let mismatch = self.instructions.len();
                self.instructions.push(Instruction::JumpIfNotEqual {
                    left: source,
                    right: expected,
                    target: usize::MAX,
                });
                let matched = self.instructions.len();
                self.instructions
                    .push(Instruction::Jump { target: usize::MAX });
                let failure = self.instructions.len();
                self.instructions.push(Instruction::RaiseTyped {
                    types: vec![Arc::from("error"), Arc::from("match_error")],
                    message: Arc::from("function argument does not match its parameter pattern"),
                });
                let end = self.instructions.len();
                self.patch_jump(mismatch, failure);
                self.patch_jump(matched, end);
            }
        }
    }

    pub(super) fn compile_capture_pattern(
        &mut self,
        pattern: &Pattern,
        source: Register,
        message: Arc<str>,
        captures: &mut Vec<(Name, Register)>,
    ) {
        match pattern {
            Pattern::Wildcard => {}
            Pattern::Capture(name) => captures.push((name.clone(), source)),
            Pattern::List(patterns) => {
                self.instructions.push(Instruction::CheckListLength {
                    source,
                    length: patterns.len() as u16,
                    message: message.clone(),
                });
                for (index, pattern) in patterns.iter().enumerate() {
                    let element = self.allocate_register();
                    self.instructions.push(Instruction::ListGet {
                        destination: element,
                        source,
                        index: index as u16,
                    });
                    self.compile_capture_pattern(pattern, element, message.clone(), captures);
                }
            }
            Pattern::Literal(_) => unreachable!("bindings use capture patterns"),
        }
    }

    pub(super) fn commit_binding_captures(
        &mut self,
        captures: Vec<(Name, Register)>,
        mutable: bool,
        block: Option<BlockId>,
    ) {
        let mut seen = HashSet::new();
        if let Some((name, _)) = captures
            .iter()
            .find(|(name, _)| !seen.insert(name.text.clone()))
        {
            self.instructions.push(Instruction::RaiseTyped {
                types: vec![Arc::from("error"), Arc::from("match_error")],
                message: Arc::from(format!("pattern captures `{name}` more than once")),
            });
            return;
        }
        for (name, source) in captures {
            if self
                .locals
                .get(&name.text)
                .is_some_and(|local| !local.binding)
            {
                self.instructions.push(Instruction::RaiseTyped {
                    types: vec![Arc::from("error"), Arc::from("name_error")],
                    message: Arc::from(format!("duplicate binding `{name}` in current scope")),
                });
                continue;
            }
            let register = self
                .locals
                .get(&name.text)
                .map(|local| local.register)
                .unwrap_or_else(|| {
                    let register = self.allocate_register();
                    self.binding_registers.push(register);
                    register
                });
            self.instructions.push(Instruction::Bind {
                binding: register,
                source,
                mutable,
                name: name.text.clone(),
            });
            self.locals
                .entry(name.text.clone())
                .and_modify(|local| local.block = block.or(local.block))
                .or_insert(Local {
                    register,
                    block,
                    binding: true,
                });
        }
    }

    pub(super) fn commit_assignment_captures(&mut self, captures: Vec<(Name, Register)>) {
        let mut assignments = Vec::with_capacity(captures.len());
        for (name, source) in captures {
            let Some(local) = self.locals.get(&name.text).copied() else {
                self.instructions.push(Instruction::RaiseTyped {
                    types: vec![Arc::from("error"), Arc::from("name_error")],
                    message: Arc::from(format!("unbound identifier `{name}` for assignment")),
                });
                return;
            };
            if !local.binding {
                self.instructions.push(Instruction::RaiseTyped {
                    types: vec![Arc::from("error"), Arc::from("mutation_error")],
                    message: Arc::from(format!("cannot assign to immutable binding `{name}`")),
                });
                return;
            }
            assignments.push((name.text, local, source));
        }
        for (name, local, _) in &assignments {
            self.instructions.push(Instruction::CheckWritable {
                binding: local.register,
                name: name.clone(),
            });
        }
        for (name, local, source) in assignments {
            self.instructions.push(Instruction::StoreBinding {
                binding: local.register,
                source,
                name,
            });
        }
    }

    pub(super) fn compile_match(&mut self, value: NodeId, arms: &[MatchArm]) -> Option<Register> {
        let value = self.compile_node(value)?;
        let result = self.allocate_register();
        let mut end_jumps = Vec::new();
        for arm in arms {
            let outer_locals = self.locals.clone();
            let mut failures = Vec::new();
            let mut captures = HashSet::new();
            let mut duplicate = None;
            self.compile_match_pattern(
                &arm.pattern,
                value,
                &mut failures,
                &mut captures,
                &mut duplicate,
            );
            if let Some(name) = duplicate {
                self.instructions.push(Instruction::RaiseTyped {
                    types: vec![Arc::from("error"), Arc::from("match_error")],
                    message: Arc::from(format!("pattern captures `{name}` more than once")),
                });
            }
            let arm_result = self.compile_executable_node(arm.body)?;
            self.instructions.push(Instruction::Move {
                destination: result,
                source: arm_result,
            });
            let end_jump = self.instructions.len();
            self.instructions
                .push(Instruction::Jump { target: usize::MAX });
            end_jumps.push(end_jump);
            let next_arm = self.instructions.len();
            for failure in failures {
                self.patch_jump(failure, next_arm);
            }
            self.locals = outer_locals;
        }
        self.instructions.push(Instruction::RaiseTyped {
            types: vec![Arc::from("error"), Arc::from("match_error")],
            message: Arc::from("no match arm accepted the value"),
        });
        let end = self.instructions.len();
        for jump in end_jumps {
            self.patch_jump(jump, end);
        }
        Some(result)
    }

    fn compile_match_pattern(
        &mut self,
        pattern: &Pattern,
        source: Register,
        failures: &mut Vec<usize>,
        captures: &mut HashSet<Arc<str>>,
        duplicate: &mut Option<Arc<str>>,
    ) {
        match pattern {
            Pattern::Wildcard => {}
            Pattern::Capture(name) => {
                if !captures.insert(name.text.clone()) && duplicate.is_none() {
                    *duplicate = Some(name.text.clone());
                }
                self.locals.insert(
                    name.text.clone(),
                    Local {
                        register: source,
                        block: None,
                        binding: false,
                    },
                );
            }
            Pattern::Literal(literal) => {
                let Some(expected) = self.compile_node(*literal) else {
                    return;
                };
                failures.push(self.instructions.len());
                self.instructions.push(Instruction::JumpIfNotEqual {
                    left: source,
                    right: expected,
                    target: usize::MAX,
                });
            }
            Pattern::List(patterns) => {
                failures.push(self.instructions.len());
                self.instructions.push(Instruction::JumpIfNotListLength {
                    source,
                    length: patterns.len() as u16,
                    target: usize::MAX,
                });
                for (index, pattern) in patterns.iter().enumerate() {
                    let element = self.allocate_register();
                    self.instructions.push(Instruction::ListGet {
                        destination: element,
                        source,
                        index: index as u16,
                    });
                    self.compile_match_pattern(pattern, element, failures, captures, duplicate);
                }
            }
        }
    }
}
