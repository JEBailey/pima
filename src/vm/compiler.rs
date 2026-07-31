use std::{collections::HashMap, sync::Arc};

use crate::{
    diagnostic::Diagnostic,
    runtime::Value,
    syntax::ast::{BindingKind, BlockId, LoopKind, Module, NodeId, NodeKind, Pattern, Visibility},
};

use super::ir::{Function, Instruction, NamespaceBinding, Primitive, Program, Register};

pub fn compile(module: &Module) -> Result<Program, Vec<Diagnostic>> {
    compile_module(module, 0)
}

pub fn compile_module(module: &Module, module_index: usize) -> Result<Program, Vec<Diagnostic>> {
    Compiler::new(module, module_index).compile()
}

fn empty_function() -> Function {
    Function {
        instructions: Vec::new(),
        register_count: 0,
        capture_count: 0,
        binding_registers: Vec::new(),
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
    attempt_depth: usize,
    in_function: bool,
    next_register: u16,
    diagnostics: Vec<Diagnostic>,
    binding_registers: Vec<Register>,
    module_index: usize,
}

#[derive(Clone, Copy)]
struct Local {
    register: Register,
    mutable: bool,
    block: Option<BlockId>,
    binding: bool,
}

struct LoopContext {
    continue_target: usize,
    break_jumps: Vec<usize>,
    result: Register,
    attempt_depth: usize,
}

impl<'a> Compiler<'a> {
    fn new(module: &'a Module, module_index: usize) -> Self {
        Self {
            module,
            constants: Vec::new(),
            instructions: Vec::new(),
            locals: HashMap::new(),
            functions: HashMap::new(),
            compiled_functions: Vec::new(),
            loops: Vec::new(),
            attempt_depth: 0,
            in_function: false,
            next_register: 0,
            diagnostics: Vec::new(),
            binding_registers: Vec::new(),
            module_index,
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
        self.predeclare_module_bindings();
        let mut module_captures = self
            .locals
            .iter()
            .map(|(name, local)| (name.clone(), *local))
            .collect::<Vec<_>>();
        module_captures.sort_by(|left, right| left.0.cmp(&right.0));
        for (index, (_, parameter, body, _)) in declarations.iter().enumerate() {
            self.compile_function(index as u16, parameter, *body, &module_captures);
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
                binding_registers: self.binding_registers,
                module_index: self.module_index,
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
            NodeKind::Symbol(name) => {
                let destination = self.allocate_register();
                self.instructions.push(Instruction::LoadSymbol {
                    destination,
                    name: name.clone(),
                });
                Some(destination)
            }
            NodeKind::Identifier(name) => {
                let Some(local) = self.locals.get(name).copied() else {
                    self.diagnostics.push(Diagnostic::at_error(
                        format!("VM compiler cannot resolve local `{name}`"),
                        node.span,
                    ));
                    return None;
                };
                let destination = self.allocate_register();
                self.instructions.push(if local.binding {
                    Instruction::LoadBinding {
                        destination,
                        binding: local.register,
                        name: name.clone(),
                    }
                } else if local.mutable {
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
            NodeKind::Member { object, member } => {
                let namespace = self.compile_node(*object)?;
                let destination = self.allocate_register();
                self.instructions.push(Instruction::LoadMember {
                    destination,
                    namespace,
                    name: member.text.clone(),
                });
                Some(destination)
            }
            NodeKind::Binding {
                visibility: Visibility::Private,
                mutability,
                pattern,
                value,
            } => {
                let block = (*mutability == BindingKind::Immutable)
                    .then(|| self.resolve_static_block(*value))
                    .flatten();
                let value = self.compile_node(*value)?;
                let mut captures = Vec::new();
                self.compile_capture_pattern(
                    pattern,
                    value,
                    Arc::from("binding pattern does not match its value"),
                    &mut captures,
                );
                self.commit_binding_captures(captures, *mutability == BindingKind::Mutable, block);
                Some(self.load_constant(Value::Unit))
            }
            NodeKind::Assignment { pattern, value } => {
                let source = self.compile_node(*value)?;
                let mut captures = Vec::new();
                self.compile_capture_pattern(
                    pattern,
                    source,
                    Arc::from("assignment pattern does not match its value"),
                    &mut captures,
                );
                self.commit_assignment_captures(captures);
                Some(source)
            }
            NodeKind::Call {
                callee, argument, ..
            } => self.compile_call(*callee, *argument, node.span),
            NodeKind::Block(block) => {
                let destination = self.allocate_register();
                self.instructions.push(Instruction::MakeBlock {
                    destination,
                    block: block.0,
                });
                Some(destination)
            }
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
            NodeKind::Throw(value) => {
                let source = self.compile_node(*value)?;
                self.instructions.push(Instruction::Throw { source });
                Some(source)
            }
            NodeKind::Attempt(block) => Some(self.compile_attempt(*block)),
            NodeKind::New(operand) => self.compile_new(*operand, node.span),
            NodeKind::Do(operand) => self.compile_do(*operand, node.span),
            NodeKind::Match { value, arms } => self.compile_match(*value, arms),
            NodeKind::Function { name, .. } if !self.in_function => {
                self.compile_top_level_function(name)
            }
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
                self.end_attempts(self.attempt_depth);
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
        let outer_attempt_depth = self.attempt_depth;
        let outer_next_register = self.next_register;
        let outer_in_function = self.in_function;
        let outer_binding_registers = std::mem::take(&mut self.binding_registers);

        self.next_register = 1 + captures.len() as u16;
        self.attempt_depth = 0;
        self.in_function = true;
        for (index, (name, local)) in captures.iter().enumerate() {
            self.locals.insert(
                name.clone(),
                Local {
                    register: Register(1 + index as u16),
                    mutable: local.mutable,
                    block: local.block,
                    binding: local.binding,
                },
            );
        }
        self.compile_parameter_pattern(parameter, Register(0));
        if let NodeKind::Block(block) = self.module.node(body).kind {
            self.predeclare_block_bindings(block);
        }
        let result = self
            .compile_executable_node(body)
            .unwrap_or_else(|| self.load_constant(Value::Unit));
        self.instructions
            .push(Instruction::Return { source: result });
        self.compiled_functions[function as usize] = Function {
            instructions: std::mem::take(&mut self.instructions),
            register_count: self.next_register,
            capture_count: captures.len() as u16,
            binding_registers: std::mem::take(&mut self.binding_registers),
        };

        self.instructions = outer_instructions;
        self.locals = outer_locals;
        self.loops = outer_loops;
        self.attempt_depth = outer_attempt_depth;
        self.next_register = outer_next_register;
        self.in_function = outer_in_function;
        self.binding_registers = outer_binding_registers;
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
        let binding = self
            .locals
            .get(&name.text)
            .copied()
            .expect("nested function binding must be allocated before statements are compiled");
        self.instructions.push(Instruction::Bind {
            binding: binding.register,
            source: destination,
            mutable: false,
            name: name.text.clone(),
        });
        Some(self.load_constant(Value::Unit))
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
            Pattern::Literal(_) => unreachable!("function parameters use binding patterns"),
        }
    }

    fn compile_capture_pattern(
        &mut self,
        pattern: &Pattern,
        source: Register,
        message: Arc<str>,
        captures: &mut Vec<(crate::syntax::ast::Name, Register)>,
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

    fn predeclare_block_bindings(&mut self, block: BlockId) {
        for statement in &self.module.block(block).statements {
            let mut names = Vec::new();
            match &self.module.node(*statement).kind {
                NodeKind::Binding { pattern, .. } => collect_capture_names(pattern, &mut names),
                NodeKind::Function { name, .. } => names.push(name.clone()),
                _ => continue,
            };
            for name in names {
                if self.locals.contains_key(&name.text) {
                    continue;
                }
                let register = self.allocate_register();
                self.binding_registers.push(register);
                self.locals.insert(
                    name.text,
                    Local {
                        register,
                        mutable: false,
                        block: None,
                        binding: true,
                    },
                );
            }
        }
    }

    fn predeclare_module_bindings(&mut self) {
        for statement in &self.module.statements {
            let mut names = Vec::new();
            match &self.module.node(*statement).kind {
                NodeKind::Binding { pattern, .. } => collect_capture_names(pattern, &mut names),
                NodeKind::Function { name, .. } => names.push(name.clone()),
                _ => continue,
            }
            for name in names {
                if self.locals.contains_key(&name.text) {
                    continue;
                }
                let register = self.allocate_register();
                self.binding_registers.push(register);
                self.locals.insert(
                    name.text,
                    Local {
                        register,
                        mutable: false,
                        block: None,
                        binding: true,
                    },
                );
            }
        }
    }

    fn compile_top_level_function(&mut self, name: &crate::syntax::ast::Name) -> Option<Register> {
        let function = self.functions.get(&name.text).copied()?;
        let mut captures = self
            .locals
            .iter()
            .map(|(name, local)| (name.clone(), *local))
            .collect::<Vec<_>>();
        captures.sort_by(|left, right| left.0.cmp(&right.0));
        let closure = self.allocate_register();
        self.instructions.push(Instruction::MakeClosure {
            destination: closure,
            function,
            captures: captures.iter().map(|(_, local)| local.register).collect(),
        });
        let binding =
            self.locals.get(&name.text).copied().expect(
                "top-level function binding must be allocated before statements are compiled",
            );
        self.instructions.push(Instruction::Bind {
            binding: binding.register,
            source: closure,
            mutable: false,
            name: name.text.clone(),
        });
        Some(self.load_constant(Value::Unit))
    }

    fn commit_binding_captures(
        &mut self,
        captures: Vec<(crate::syntax::ast::Name, Register)>,
        mutable: bool,
        block: Option<BlockId>,
    ) {
        let mut seen = std::collections::HashSet::new();
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
            let register = if let Some(local) = self.locals.get(&name.text) {
                local.register
            } else {
                let register = self.allocate_register();
                self.binding_registers.push(register);
                register
            };
            self.instructions.push(Instruction::Bind {
                binding: register,
                source,
                mutable,
                name: name.text.clone(),
            });
            self.locals
                .entry(name.text.clone())
                .and_modify(|local| {
                    local.mutable = mutable;
                    local.block = block.or(local.block);
                })
                .or_insert(Local {
                    register,
                    mutable,
                    block,
                    binding: true,
                });
        }
    }

    fn compile_call(
        &mut self,
        callee: NodeId,
        argument: NodeId,
        span: crate::source::Span,
    ) -> Option<Register> {
        if let NodeKind::Identifier(name) = &self.module.node(callee).kind
            && matches!(
                name.as_ref(),
                "+" | "-" | "*" | "/" | "div" | "mod" | "<" | ">" | "="
            )
        {
            return self.compile_primitive_call(callee, argument, span);
        }
        let callee = self.compile_node(callee)?;
        let argument = self.compile_node(argument)?;
        let destination = self.allocate_register();
        self.instructions.push(Instruction::CallDynamic {
            destination,
            callee,
            argument,
        });
        Some(destination)
    }

    fn commit_assignment_captures(&mut self, captures: Vec<(crate::syntax::ast::Name, Register)>) {
        let mut assignments = Vec::with_capacity(captures.len());
        for (name, source) in captures {
            let Some(local) = self.locals.get(&name.text).copied() else {
                self.diagnostics.push(Diagnostic::at_error(
                    format!("VM compiler cannot resolve local `{name}`"),
                    name.span,
                ));
                continue;
            };
            if !local.binding && !local.mutable {
                self.diagnostics.push(Diagnostic::at_error(
                    format!("cannot assign to immutable local `{name}`"),
                    name.span,
                ));
                continue;
            }
            assignments.push((name.text, local, source));
        }
        if !self.diagnostics.is_empty() {
            return;
        }
        for (name, local, source) in assignments {
            if local.binding {
                self.instructions.push(Instruction::StoreBinding {
                    binding: local.register,
                    source,
                    name,
                });
            } else {
                self.instructions.push(Instruction::StoreCell {
                    cell: local.register,
                    source,
                });
            }
        }
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

    fn compile_executable_node(&mut self, node: NodeId) -> Option<Register> {
        match self.module.node(node).kind {
            NodeKind::Block(block) => Some(self.compile_block(block)),
            _ => self.compile_node(node),
        }
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
            Some(alternative) => self.compile_executable_node(alternative)?,
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

    fn compile_attempt(&mut self, block: BlockId) -> Register {
        let destination = self.allocate_register();
        let begin = self.instructions.len();
        self.instructions.push(Instruction::BeginAttempt {
            destination,
            catch_target: usize::MAX,
        });
        self.attempt_depth += 1;
        let result = self.compile_block(block);
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
            unreachable!("attempt must begin with a handler instruction");
        };
        *target = catch_target;
        let end = self.instructions.len();
        self.patch_jump(end_jump, end);
        destination
    }

    fn compile_new(&mut self, operand: NodeId, span: crate::source::Span) -> Option<Register> {
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
                            mutable: false,
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
                _ => {
                    self.diagnostics.push(Diagnostic::at_error(
                        "register VM namespaces currently support immutable value bindings only",
                        node.span,
                    ));
                }
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

    fn compile_do(&mut self, operand: NodeId, span: crate::source::Span) -> Option<Register> {
        let Some(block) = self.resolve_static_block(operand) else {
            self.diagnostics.push(Diagnostic::at_error(
                "register VM currently requires a statically known block after `do`",
                span,
            ));
            return None;
        };
        if matches!(self.module.node(operand).kind, NodeKind::Identifier(_)) {
            self.compile_node(operand)?;
        }
        self.check_block_requirements(block);
        Some(self.compile_block(block))
    }

    fn resolve_static_block(&self, operand: NodeId) -> Option<BlockId> {
        match &self.module.node(operand).kind {
            NodeKind::Block(block) => Some(*block),
            NodeKind::Identifier(name) => self.locals.get(name).and_then(|local| local.block),
            _ => None,
        }
    }

    fn compile_match(
        &mut self,
        value: NodeId,
        arms: &[crate::syntax::ast::MatchArm],
    ) -> Option<Register> {
        let value = self.compile_node(value)?;
        let result = self.allocate_register();
        let mut end_jumps = Vec::new();
        for arm in arms {
            let outer_locals = self.locals.clone();
            let mut failures = Vec::new();
            let mut captures = std::collections::HashSet::new();
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
            let arm_result = self.compile_block(arm.body);
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
        captures: &mut std::collections::HashSet<Arc<str>>,
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
                        mutable: false,
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
        let Some((result, loop_attempt_depth)) = self
            .loops
            .last()
            .map(|context| (context.result, context.attempt_depth))
        else {
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

    fn compile_continue(&mut self, span: crate::source::Span) -> Option<Register> {
        let Some(context) = self.loops.last() else {
            self.diagnostics
                .push(Diagnostic::at_error("`continue` outside loop", span));
            return None;
        };
        let result = context.result;
        let target = context.continue_target;
        let loop_attempt_depth = context.attempt_depth;
        self.end_attempts(self.attempt_depth - loop_attempt_depth);
        self.instructions.push(Instruction::Jump { target });
        Some(result)
    }

    fn end_attempts(&mut self, count: usize) {
        self.instructions
            .extend(std::iter::repeat_n(Instruction::EndAttempt, count));
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
            }
            | Instruction::JumpIfNotListLength {
                target: jump_target,
                ..
            }
            | Instruction::JumpIfNotEqual {
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

fn collect_capture_names(pattern: &Pattern, names: &mut Vec<crate::syntax::ast::Name>) {
    match pattern {
        Pattern::Capture(name) => names.push(name.clone()),
        Pattern::List(patterns) => {
            for pattern in patterns {
                collect_capture_names(pattern, names);
            }
        }
        Pattern::Wildcard | Pattern::Literal(_) => {}
    }
}
