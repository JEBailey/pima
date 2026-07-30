use std::{collections::HashMap, sync::Arc};

use crate::{
    diagnostic::Diagnostic,
    runtime::Value,
    syntax::ast::{BindingKind, BlockId, LoopKind, Module, NodeId, NodeKind, Pattern, Visibility},
};

use super::ir::{Function, Instruction, Primitive, Program, Register};

pub fn compile(module: &Module) -> Result<Program, Vec<Diagnostic>> {
    Compiler::new(module).compile()
}

fn empty_function() -> Function {
    Function {
        instructions: Vec::new(),
        register_count: 0,
        capture_count: 0,
    }
}

struct Compiler<'a> {
    module: &'a Module,
    constants: Vec<Value>,
    instructions: Vec<Instruction>,
    locals: HashMap<Arc<str>, Local>,
    functions: HashMap<Arc<str>, u16>,
    compiled_functions: Vec<Function>,
    loops: Vec<LoopContext>,
    in_function: bool,
    next_register: u16,
    diagnostics: Vec<Diagnostic>,
}

#[derive(Clone, Copy)]
struct Local {
    register: Register,
    mutable: bool,
}

struct LoopContext {
    continue_target: usize,
    break_jumps: Vec<usize>,
    result: Register,
}

impl<'a> Compiler<'a> {
    fn new(module: &'a Module) -> Self {
        Self {
            module,
            constants: Vec::new(),
            instructions: Vec::new(),
            locals: HashMap::new(),
            functions: HashMap::new(),
            compiled_functions: Vec::new(),
            loops: Vec::new(),
            in_function: false,
            next_register: 0,
            diagnostics: Vec::new(),
        }
    }

    fn compile(mut self) -> Result<Program, Vec<Diagnostic>> {
        let declarations = self
            .module
            .statements
            .iter()
            .filter_map(|statement| match &self.module.node(*statement).kind {
                NodeKind::Function {
                    name,
                    parameter,
                    body,
                    ..
                } => Some((name.text.clone(), parameter.clone(), *body, name.span)),
                _ => None,
            })
            .collect::<Vec<_>>();
        for (index, (name, _, _, span)) in declarations.iter().enumerate() {
            if self.functions.insert(name.clone(), index as u16).is_some() {
                self.diagnostics.push(Diagnostic::at_error(
                    format!("duplicate VM function `{name}`"),
                    *span,
                ));
            }
            self.compiled_functions.push(empty_function());
        }
        for (index, (_, parameter, body, _)) in declarations.iter().enumerate() {
            self.compile_function(index as u16, parameter, *body, &[]);
        }

        let mut result = self.load_constant(Value::Unit);
        for &statement in &self.module.statements {
            if let Some(register) = self.compile_node(statement) {
                result = register;
            }
        }
        self.instructions
            .push(Instruction::Return { source: result });

        if self.diagnostics.is_empty() {
            Ok(Program {
                constants: self.constants,
                instructions: self.instructions,
                register_count: self.next_register,
                functions: self.compiled_functions,
            })
        } else {
            Err(self.diagnostics)
        }
    }

    fn compile_node(&mut self, id: NodeId) -> Option<Register> {
        let node = self.module.node(id);
        match &node.kind {
            NodeKind::Unit => Some(self.load_constant(Value::Unit)),
            NodeKind::Boolean(value) => Some(self.load_constant(Value::Boolean(*value))),
            NodeKind::Integer(value) => Some(self.load_constant(Value::Integer(*value))),
            NodeKind::Float(value) => Some(self.load_constant(Value::Float(*value))),
            NodeKind::String(value) => Some(self.load_constant(Value::String(value.clone()))),
            NodeKind::Identifier(name) => {
                let Some(local) = self.locals.get(name).copied() else {
                    self.diagnostics.push(Diagnostic::at_error(
                        format!("VM compiler cannot resolve local `{name}`"),
                        node.span,
                    ));
                    return None;
                };
                let destination = self.allocate_register();
                self.instructions.push(if local.mutable {
                    Instruction::LoadCell {
                        destination,
                        cell: local.register,
                    }
                } else {
                    Instruction::Move {
                        destination,
                        source: local.register,
                    }
                });
                Some(destination)
            }
            NodeKind::List(elements) => {
                let registers = elements
                    .iter()
                    .filter_map(|element| self.compile_node(*element))
                    .collect();
                let destination = self.allocate_register();
                self.instructions.push(Instruction::MakeList {
                    destination,
                    elements: registers,
                });
                Some(destination)
            }
            NodeKind::Binding {
                visibility: Visibility::Private,
                mutability,
                pattern: Pattern::Capture(name),
                value,
            } => {
                let value = self.compile_node(*value)?;
                if self.locals.contains_key(&name.text) {
                    self.diagnostics.push(Diagnostic::at_error(
                        format!("duplicate local `{name}`"),
                        name.span,
                    ));
                    return None;
                }
                let register = if *mutability == BindingKind::Mutable {
                    let register = self.allocate_register();
                    self.instructions.push(Instruction::MakeCell {
                        destination: register,
                        source: value,
                    });
                    register
                } else {
                    value
                };
                self.locals.insert(
                    name.text.clone(),
                    Local {
                        register,
                        mutable: *mutability == BindingKind::Mutable,
                    },
                );
                Some(value)
            }
            NodeKind::Assignment {
                pattern: Pattern::Capture(name),
                value,
            } => self.compile_assignment(name, *value),
            NodeKind::Call {
                callee, argument, ..
            } => self.compile_call(*callee, *argument, node.span),
            NodeKind::Block(block) => Some(self.compile_block(*block)),
            NodeKind::Conditional {
                condition,
                consequent,
                alternative,
            } => self.compile_conditional(*condition, *consequent, *alternative),
            NodeKind::Loop {
                kind,
                condition,
                body,
            } => self.compile_loop(*kind, *condition, *body),
            NodeKind::Break(value) => self.compile_break(*value, node.span),
            NodeKind::Continue => self.compile_continue(node.span),
            NodeKind::Function { .. } if !self.in_function => Some(self.load_constant(Value::Unit)),
            NodeKind::Function {
                name,
                parameter,
                body,
                ..
            } => self.compile_nested_function(name, parameter, *body),
            NodeKind::Return(value) if self.in_function => {
                let value = match value {
                    Some(value) => self.compile_node(*value)?,
                    None => self.load_constant(Value::Unit),
                };
                self.instructions
                    .push(Instruction::Return { source: value });
                Some(value)
            }
            _ => {
                self.diagnostics.push(Diagnostic::at_error(
                    "construct is not supported by the register VM yet",
                    node.span,
                ));
                None
            }
        }
    }

    fn compile_function(
        &mut self,
        function: u16,
        parameter: &Pattern,
        body: NodeId,
        captures: &[(Arc<str>, Local)],
    ) {
        let outer_instructions = std::mem::take(&mut self.instructions);
        let outer_locals = std::mem::take(&mut self.locals);
        let outer_loops = std::mem::take(&mut self.loops);
        let outer_next_register = self.next_register;
        let outer_in_function = self.in_function;

        self.next_register = 1 + captures.len() as u16;
        self.in_function = true;
        for (index, (name, local)) in captures.iter().enumerate() {
            self.locals.insert(
                name.clone(),
                Local {
                    register: Register(1 + index as u16),
                    mutable: local.mutable,
                },
            );
        }
        self.compile_parameter_pattern(parameter, Register(0));
        let result = self
            .compile_node(body)
            .unwrap_or_else(|| self.load_constant(Value::Unit));
        self.instructions
            .push(Instruction::Return { source: result });
        self.compiled_functions[function as usize] = Function {
            instructions: std::mem::take(&mut self.instructions),
            register_count: self.next_register,
            capture_count: captures.len() as u16,
        };

        self.instructions = outer_instructions;
        self.locals = outer_locals;
        self.loops = outer_loops;
        self.next_register = outer_next_register;
        self.in_function = outer_in_function;
    }

    fn compile_nested_function(
        &mut self,
        name: &crate::syntax::ast::Name,
        parameter: &Pattern,
        body: NodeId,
    ) -> Option<Register> {
        let mut captures = self
            .locals
            .iter()
            .map(|(name, local)| (name.clone(), *local))
            .collect::<Vec<_>>();
        captures.sort_by(|left, right| left.0.cmp(&right.0));
        let function = self.compiled_functions.len() as u16;
        self.compiled_functions.push(empty_function());
        self.compile_function(function, parameter, body, &captures);
        let destination = self.allocate_register();
        self.instructions.push(Instruction::MakeClosure {
            destination,
            function,
            captures: captures.iter().map(|(_, local)| local.register).collect(),
        });
        self.locals.insert(
            name.text.clone(),
            Local {
                register: destination,
                mutable: false,
            },
        );
        Some(destination)
    }

    fn compile_parameter_pattern(&mut self, pattern: &Pattern, source: Register) {
        match pattern {
            Pattern::Wildcard => {}
            Pattern::Capture(name) => {
                self.locals.insert(
                    name.text.clone(),
                    Local {
                        register: source,
                        mutable: false,
                    },
                );
            }
            Pattern::List(patterns) => {
                self.instructions.push(Instruction::CheckListLength {
                    source,
                    length: patterns.len() as u16,
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
            Pattern::Literal(_) => self.diagnostics.push(Diagnostic::error(
                "literal function parameters are not supported by the register VM",
            )),
        }
    }

    fn compile_call(
        &mut self,
        callee: NodeId,
        argument: NodeId,
        span: crate::source::Span,
    ) -> Option<Register> {
        if let NodeKind::Identifier(name) = &self.module.node(callee).kind
            && let Some(function) = self.functions.get(name).copied()
        {
            let argument = self.compile_node(argument)?;
            let destination = self.allocate_register();
            self.instructions.push(Instruction::Call {
                destination,
                function,
                argument,
            });
            return Some(destination);
        }
        if let NodeKind::Identifier(name) = &self.module.node(callee).kind
            && self.locals.contains_key(name)
        {
            let callee = self.compile_node(callee)?;
            let argument = self.compile_node(argument)?;
            let destination = self.allocate_register();
            self.instructions.push(Instruction::CallDynamic {
                destination,
                callee,
                argument,
            });
            return Some(destination);
        }
        self.compile_primitive_call(callee, argument, span)
    }

    fn compile_assignment(
        &mut self,
        name: &crate::syntax::ast::Name,
        value: NodeId,
    ) -> Option<Register> {
        let Some(local) = self.locals.get(&name.text).copied() else {
            self.diagnostics.push(Diagnostic::at_error(
                format!("VM compiler cannot resolve local `{name}`"),
                name.span,
            ));
            return None;
        };
        if !local.mutable {
            self.diagnostics.push(Diagnostic::at_error(
                format!("cannot assign to immutable local `{name}`"),
                name.span,
            ));
            return None;
        }
        let source = self.compile_node(value)?;
        self.instructions.push(Instruction::StoreCell {
            cell: local.register,
            source,
        });
        Some(source)
    }

    fn compile_block(&mut self, block: BlockId) -> Register {
        let statements = self.module.block(block).statements.clone();
        let mut result = self.load_constant(Value::Unit);
        for statement in statements {
            if let Some(register) = self.compile_node(statement) {
                result = register;
            }
        }
        result
    }

    fn compile_conditional(
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
        });

        let consequent = self.compile_node(consequent)?;
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
            Some(alternative) => self.compile_node(alternative)?,
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

    fn compile_loop(
        &mut self,
        kind: LoopKind,
        condition: NodeId,
        body: BlockId,
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
            },
            LoopKind::Until => Instruction::JumpIfTrue {
                condition,
                target: usize::MAX,
            },
        });
        self.loops.push(LoopContext {
            continue_target: condition_target,
            break_jumps: Vec::new(),
            result,
        });
        let body = self.compile_block(body);
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

    fn compile_break(
        &mut self,
        value: Option<NodeId>,
        span: crate::source::Span,
    ) -> Option<Register> {
        let Some(result) = self.loops.last().map(|context| context.result) else {
            self.diagnostics
                .push(Diagnostic::at_error("`break` outside loop", span));
            return None;
        };
        let value = match value {
            Some(value) => self.compile_node(value)?,
            None => self.load_constant(Value::Unit),
        };
        self.instructions.push(Instruction::Move {
            destination: result,
            source: value,
        });
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

    fn compile_continue(&mut self, span: crate::source::Span) -> Option<Register> {
        let Some(context) = self.loops.last() else {
            self.diagnostics
                .push(Diagnostic::at_error("`continue` outside loop", span));
            return None;
        };
        let result = context.result;
        let target = context.continue_target;
        self.instructions.push(Instruction::Jump { target });
        Some(result)
    }

    fn patch_jump(&mut self, instruction: usize, target: usize) {
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
            } => *jump_target = target,
            _ => unreachable!("compiler attempted to patch a non-jump instruction"),
        }
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

    fn load_constant(&mut self, value: Value) -> Register {
        let constant = self.constants.len() as u16;
        self.constants.push(value);
        let destination = self.allocate_register();
        self.instructions.push(Instruction::LoadConstant {
            destination,
            constant,
        });
        destination
    }

    fn allocate_register(&mut self) -> Register {
        let register = Register(self.next_register);
        self.next_register = self
            .next_register
            .checked_add(1)
            .expect("VM register limit exceeded");
        register
    }
}
