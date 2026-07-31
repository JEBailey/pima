use std::{collections::HashMap, sync::Arc};

use crate::{
    diagnostic::Diagnostic,
    runtime::Value,
    syntax::ast::{
        BindingKind, BlockId, LoopKind, Module, NamespaceImportSelection, NodeId, NodeKind,
        Pattern, Visibility,
    },
};

use super::analysis::ScopeAnalysis;
use super::ir::{Function, Instruction, NamespaceBinding, Primitive, Program, Register};
use super::passes::PassPipeline;

mod blocks;
mod patterns;

pub fn compile(module: &Module) -> Result<Program, Vec<Diagnostic>> {
    compile_module(module, 0)
}

pub fn compile_with_pipeline(
    module: &Module,
    pipeline: &PassPipeline,
) -> Result<Program, Vec<Diagnostic>> {
    compile_module_with_pipeline(module, 0, pipeline)
}

pub fn compile_module(module: &Module, module_index: usize) -> Result<Program, Vec<Diagnostic>> {
    compile_module_with_pipeline(module, module_index, &PassPipeline::standard())
}

pub fn compile_module_with_pipeline(
    module: &Module,
    module_index: usize,
    pipeline: &PassPipeline,
) -> Result<Program, Vec<Diagnostic>> {
    let mut program = Compiler::new(module, module_index).compile()?;
    pipeline.run(&mut program)?;
    Ok(program)
}

pub fn compile_module_with_globals(
    module: &Module,
    module_index: usize,
    globals: impl IntoIterator<Item = (Arc<str>, Value)>,
) -> Result<Program, Vec<Diagnostic>> {
    compile_module_with_globals_and_pipeline(
        module,
        module_index,
        globals,
        &PassPipeline::standard(),
    )
}

pub fn compile_module_with_globals_and_pipeline(
    module: &Module,
    module_index: usize,
    globals: impl IntoIterator<Item = (Arc<str>, Value)>,
    pipeline: &PassPipeline,
) -> Result<Program, Vec<Diagnostic>> {
    let mut compiler = Compiler::new(module, module_index);
    compiler.install_globals(globals);
    let mut program = compiler.compile()?;
    pipeline.run(&mut program)?;
    Ok(program)
}

fn next_program_id() -> u64 {
    static NEXT_PROGRAM_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    NEXT_PROGRAM_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

fn dense_spans(
    instruction_count: usize,
    spans: &HashMap<usize, crate::source::Span>,
) -> Vec<Option<crate::source::Span>> {
    (0..instruction_count)
        .map(|instruction| spans.get(&instruction).copied())
        .collect()
}

fn empty_function() -> Function {
    Function {
        name: Arc::from("<uncompiled>"),
        instructions: Vec::new(),
        instruction_spans: Vec::new(),
        register_count: 0,
        capture_count: 0,
        parameter_count: None,
        binding_registers: Vec::new(),
    }
}

struct Compiler<'a> {
    module: &'a Module,
    constants: Vec<Value>,
    instructions: Vec<Instruction>,
    instruction_spans: HashMap<usize, crate::source::Span>,
    locals: HashMap<Arc<str>, Local>,
    functions: HashMap<Arc<str>, u16>,
    compiled_functions: Vec<Function>,
    block_functions: HashMap<BlockId, (u16, Vec<Arc<str>>)>,
    loops: Vec<LoopContext>,
    attempt_depth: usize,
    in_function: bool,
    capture_count: u16,
    next_register: u16,
    diagnostics: Vec<Diagnostic>,
    binding_registers: Vec<Register>,
    initial_bindings: Vec<(Register, Value)>,
    imported_bindings: std::collections::HashSet<Arc<str>>,
    module_index: usize,
}

#[derive(Clone, Copy)]
struct Local {
    register: Register,
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
            instruction_spans: HashMap::new(),
            locals: HashMap::new(),
            functions: HashMap::new(),
            compiled_functions: Vec::new(),
            block_functions: HashMap::new(),
            loops: Vec::new(),
            attempt_depth: 0,
            in_function: false,
            capture_count: 0,
            next_register: 0,
            diagnostics: Vec::new(),
            binding_registers: Vec::new(),
            initial_bindings: Vec::new(),
            imported_bindings: std::collections::HashSet::new(),
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
        let module_analysis = ScopeAnalysis::module(self.module);
        self.apply_scope_analysis(&module_analysis);
        for statement in &self.module.statements {
            if let NodeKind::NamespaceImport {
                selection: NamespaceImportSelection::Member(member),
                alias,
                ..
            } = &self.module.node(*statement).kind
            {
                let local = alias.as_ref().unwrap_or(member);
                if !self.locals.contains_key(&local.text) {
                    let register = self.allocate_register();
                    self.binding_registers.push(register);
                    self.locals.insert(
                        local.text.clone(),
                        Local {
                            register,
                            block: None,
                            binding: true,
                        },
                    );
                }
            }
        }
        let mut module_captures = self
            .locals
            .iter()
            .map(|(name, local)| (name.clone(), *local))
            .collect::<Vec<_>>();
        module_captures.sort_by(|left, right| left.0.cmp(&right.0));
        for (index, (name, parameter, body, _)) in declarations.iter().enumerate() {
            self.compile_function(index as u16, name, parameter, *body, &module_captures);
        }

        let mut result = self.load_constant(Value::Unit);
        for &statement in &self.module.statements {
            if let Some(register) = self.compile_node(statement) {
                result = register;
            }
        }
        let exports = self.module_bindings();
        self.instructions
            .push(Instruction::PublishExports { bindings: exports });
        self.instructions
            .push(Instruction::Return { source: result });

        if self.diagnostics.is_empty() {
            let instruction_spans = dense_spans(self.instructions.len(), &self.instruction_spans);
            Ok(Program {
                id: next_program_id(),
                constants: self.constants,
                instructions: self.instructions,
                instruction_spans,
                register_count: self.next_register,
                functions: self.compiled_functions,
                binding_registers: self.binding_registers,
                initial_bindings: self.initial_bindings,
                module_index: self.module_index,
            })
        } else {
            Err(self.diagnostics)
        }
    }

    fn install_globals(&mut self, globals: impl IntoIterator<Item = (Arc<str>, Value)>) {
        for (name, value) in globals {
            if self.locals.contains_key(&name) {
                continue;
            }
            let binding = self.allocate_register();
            self.binding_registers.push(binding);
            self.locals.insert(
                name.clone(),
                Local {
                    register: binding,
                    block: None,
                    binding: true,
                },
            );
            self.initial_bindings.push((binding, value));
        }
    }

    fn module_bindings(&self) -> Vec<NamespaceBinding> {
        let mut exports = Vec::new();
        for statement in &self.module.statements {
            match &self.module.node(*statement).kind {
                NodeKind::Binding {
                    visibility,
                    pattern,
                    ..
                } => collect_pattern_exports(
                    pattern,
                    &self.locals,
                    *visibility == Visibility::Public,
                    &mut exports,
                ),
                NodeKind::Function {
                    visibility, name, ..
                } => {
                    if let Some(local) = self.locals.get(&name.text) {
                        exports.push(NamespaceBinding {
                            name: name.text.clone(),
                            source: local.register,
                            public: *visibility == Visibility::Public,
                        });
                    }
                }
                _ => {}
            }
        }
        exports
    }

    fn compile_node(&mut self, id: NodeId) -> Option<Register> {
        let start = self.instructions.len();
        let result = self.compile_node_inner(id);
        let span = self.module.node(id).span;
        for instruction in start..self.instructions.len() {
            self.instruction_spans.entry(instruction).or_insert(span);
        }
        result
    }

    fn compile_node_inner(&mut self, id: NodeId) -> Option<Register> {
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
            NodeKind::Placeholder => Some(self.load_constant(Value::Placeholder)),
            NodeKind::Identifier(name) => {
                let Some(local) = self.locals.get(name).copied() else {
                    self.instructions.push(Instruction::RaiseTyped {
                        types: vec![Arc::from("error"), Arc::from("name_error")],
                        message: Arc::from(format!("unbound identifier `{name}`")),
                    });
                    return Some(self.load_constant(Value::Unit));
                };
                let destination = self.allocate_register();
                self.instructions.push(if local.binding {
                    Instruction::LoadBinding {
                        destination,
                        binding: local.register,
                        name: name.clone(),
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
                visibility: _,
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
                let (function, context) = self.compile_block_function(*block);
                let destination = self.allocate_register();
                self.instructions.push(Instruction::MakeBlock {
                    destination,
                    block: block.0,
                    function,
                    context,
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
            NodeKind::Function { name, .. }
                if !self.in_function && self.module.statements.contains(&id) =>
            {
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
            NodeKind::NamespaceImport {
                path,
                selection: NamespaceImportSelection::Member(member),
                alias,
            } if self.module.statements.contains(&id)
                && self.locals.contains_key(&path[0].text) =>
            {
                let root = self.locals[&path[0].text];
                let mut value = self.allocate_register();
                self.instructions.push(Instruction::LoadBinding {
                    destination: value,
                    binding: root.register,
                    name: path[0].text.clone(),
                });
                for name in &path[1..] {
                    let destination = self.allocate_register();
                    self.instructions.push(Instruction::LoadMember {
                        destination,
                        namespace: value,
                        name: name.text.clone(),
                    });
                    value = destination;
                }
                let imported = self.allocate_register();
                self.instructions.push(Instruction::LoadMember {
                    destination: imported,
                    namespace: value,
                    name: member.text.clone(),
                });
                let local_name = alias.as_ref().unwrap_or(member).text.clone();
                let binding = self.locals[&local_name].register;
                self.instructions.push(Instruction::Bind {
                    binding,
                    source: imported,
                    mutable: false,
                    name: local_name.clone(),
                });
                self.imported_bindings.insert(local_name);
                Some(self.load_constant(Value::Unit))
            }
            NodeKind::Import { .. } | NodeKind::NamespaceImport { .. }
                if self.module.statements.contains(&id) =>
            {
                Some(self.load_constant(Value::Unit))
            }
            NodeKind::Import { .. } | NodeKind::NamespaceImport { .. } => {
                self.instructions.push(Instruction::RaiseTyped {
                    types: vec![Arc::from("error"), Arc::from("import_error")],
                    message: Arc::from("imports are allowed only at module scope"),
                });
                Some(self.load_constant(Value::Unit))
            }
            NodeKind::Return(_) => {
                self.instructions.push(Instruction::RaiseTyped {
                    types: vec![Arc::from("error"), Arc::from("control_flow_error")],
                    message: Arc::from("return outside of a function"),
                });
                Some(self.load_constant(Value::Unit))
            }
        }
    }

    fn compile_function(
        &mut self,
        function: u16,
        name: &Arc<str>,
        parameter: &Pattern,
        body: NodeId,
        captures: &[(Arc<str>, Local)],
    ) {
        let outer_instructions = std::mem::take(&mut self.instructions);
        let outer_instruction_spans = std::mem::take(&mut self.instruction_spans);
        let outer_locals = std::mem::take(&mut self.locals);
        let outer_loops = std::mem::take(&mut self.loops);
        let outer_attempt_depth = self.attempt_depth;
        let outer_next_register = self.next_register;
        let outer_in_function = self.in_function;
        let outer_capture_count = self.capture_count;
        let outer_binding_registers = std::mem::take(&mut self.binding_registers);

        self.next_register = 1 + captures.len() as u16;
        self.attempt_depth = 0;
        self.in_function = true;
        self.capture_count = captures.len() as u16;
        for (index, (name, local)) in captures.iter().enumerate() {
            self.locals.insert(
                name.clone(),
                Local {
                    register: Register(1 + index as u16),
                    block: local.block,
                    binding: local.binding,
                },
            );
        }
        self.compile_parameter_pattern(parameter, Register(0));
        let inherited_blocks = self
            .locals
            .iter()
            .filter_map(|(name, local)| local.block.map(|block| (name.clone(), block)))
            .collect();
        let function_analysis = ScopeAnalysis::function(self.module, body, inherited_blocks);
        for name in function_analysis.declarations() {
            if self.locals.get(&name.text).is_some_and(|local| {
                local.binding && local.register.0 > 0 && local.register.0 <= self.capture_count
            }) {
                let register = self.allocate_register();
                self.binding_registers.push(register);
                self.locals.insert(
                    name.text.clone(),
                    Local {
                        register,
                        block: function_analysis.static_block(&name.text),
                        binding: true,
                    },
                );
            }
        }
        self.apply_scope_analysis(&function_analysis);
        let result = self
            .compile_executable_node(body)
            .unwrap_or_else(|| self.load_constant(Value::Unit));
        self.instructions
            .push(Instruction::Return { source: result });
        let instructions = std::mem::take(&mut self.instructions);
        let instruction_spans = dense_spans(instructions.len(), &self.instruction_spans);
        self.compiled_functions[function as usize] = Function {
            name: name.clone(),
            instructions,
            instruction_spans,
            register_count: self.next_register,
            capture_count: captures.len() as u16,
            parameter_count: match parameter {
                Pattern::List(patterns) => Some(patterns.len() as u16),
                _ => None,
            },
            binding_registers: std::mem::take(&mut self.binding_registers),
        };

        self.instructions = outer_instructions;
        self.instruction_spans = outer_instruction_spans;
        self.locals = outer_locals;
        self.loops = outer_loops;
        self.attempt_depth = outer_attempt_depth;
        self.next_register = outer_next_register;
        self.in_function = outer_in_function;
        self.capture_count = outer_capture_count;
        self.binding_registers = outer_binding_registers;
    }

    fn compile_block_function(&mut self, block: BlockId) -> (u16, Vec<Arc<str>>) {
        if let Some(compiled) = self.block_functions.get(&block) {
            return compiled.clone();
        }
        let mut names = std::collections::HashSet::new();
        let mut visited = std::collections::HashSet::new();
        collect_block_context(self.module, block, &mut names, &mut visited);
        let mut names = names.into_iter().collect::<Vec<_>>();
        names.sort();
        let function = self.compiled_functions.len() as u16;
        self.compiled_functions.push(empty_function());
        self.block_functions
            .insert(block, (function, names.clone()));

        let outer_instructions = std::mem::take(&mut self.instructions);
        let outer_instruction_spans = std::mem::take(&mut self.instruction_spans);
        let outer_locals = std::mem::take(&mut self.locals);
        let outer_loops = std::mem::take(&mut self.loops);
        let outer_attempt_depth = self.attempt_depth;
        let outer_next_register = self.next_register;
        let outer_in_function = self.in_function;
        let outer_capture_count = self.capture_count;
        let outer_binding_registers = std::mem::take(&mut self.binding_registers);

        self.next_register = 1 + names.len() as u16;
        self.attempt_depth = 0;
        self.in_function = true;
        self.capture_count = names.len() as u16;
        for (index, name) in names.iter().enumerate() {
            self.locals.insert(
                name.clone(),
                Local {
                    register: Register(1 + index as u16),
                    block: None,
                    binding: true,
                },
            );
        }
        let result = self.compile_block(block);
        self.instructions
            .push(Instruction::Return { source: result });
        let instructions = std::mem::take(&mut self.instructions);
        let instruction_spans = dense_spans(instructions.len(), &self.instruction_spans);
        self.compiled_functions[function as usize] = Function {
            name: Arc::from("<block>"),
            instructions,
            instruction_spans,
            register_count: self.next_register,
            capture_count: names.len() as u16,
            parameter_count: None,
            binding_registers: std::mem::take(&mut self.binding_registers),
        };

        self.instructions = outer_instructions;
        self.instruction_spans = outer_instruction_spans;
        self.locals = outer_locals;
        self.loops = outer_loops;
        self.attempt_depth = outer_attempt_depth;
        self.next_register = outer_next_register;
        self.in_function = outer_in_function;
        self.capture_count = outer_capture_count;
        self.binding_registers = outer_binding_registers;
        (function, names)
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
        self.compile_function(function, &name.text, parameter, body, &captures);
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
        Some(destination)
    }

    fn apply_scope_analysis(&mut self, analysis: &ScopeAnalysis) {
        for name in analysis.declarations() {
            if let Some(local) = self.locals.get_mut(&name.text) {
                local.block = local.block.or_else(|| analysis.static_block(&name.text));
                continue;
            }
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
        Some(closure)
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
            NodeKind::Block(block) => {
                self.check_block_requirements(block);
                Some(self.compile_block(block))
            }
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

    fn compile_continue(&mut self, _span: crate::source::Span) -> Option<Register> {
        let Some(context) = self.loops.last() else {
            self.instructions.push(Instruction::RaiseTyped {
                types: vec![Arc::from("error"), Arc::from("control_flow_error")],
                message: Arc::from("continue outside of a loop"),
            });
            return Some(self.load_constant(Value::Unit));
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
            }
            | Instruction::JumpIfNotBlock {
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

fn collect_pattern_exports(
    pattern: &Pattern,
    locals: &HashMap<Arc<str>, Local>,
    public: bool,
    exports: &mut Vec<NamespaceBinding>,
) {
    match pattern {
        Pattern::Capture(name) => {
            if let Some(local) = locals.get(&name.text) {
                exports.push(NamespaceBinding {
                    name: name.text.clone(),
                    source: local.register,
                    public,
                });
            }
        }
        Pattern::List(patterns) => {
            for pattern in patterns {
                collect_pattern_exports(pattern, locals, public, exports);
            }
        }
        Pattern::Wildcard | Pattern::Literal(_) => {}
    }
}

fn collect_block_context(
    module: &Module,
    block: BlockId,
    names: &mut std::collections::HashSet<Arc<str>>,
    visited: &mut std::collections::HashSet<BlockId>,
) {
    if !visited.insert(block) {
        return;
    }
    for requirement in &module.block(block).requirements {
        names.insert(requirement.text.clone());
    }
    for statement in &module.block(block).statements {
        collect_node_context(module, *statement, names, visited);
    }
}

fn collect_node_context(
    module: &Module,
    node: NodeId,
    names: &mut std::collections::HashSet<Arc<str>>,
    visited: &mut std::collections::HashSet<BlockId>,
) {
    match &module.node(node).kind {
        NodeKind::Identifier(name) => {
            names.insert(name.clone());
        }
        NodeKind::List(nodes) => {
            for node in nodes {
                collect_node_context(module, *node, names, visited);
            }
        }
        NodeKind::Block(block) | NodeKind::Attempt(block) => {
            collect_block_context(module, *block, names, visited)
        }
        NodeKind::Member { object, .. } | NodeKind::New(object) | NodeKind::Do(object) => {
            collect_node_context(module, *object, names, visited)
        }
        NodeKind::Call {
            callee, argument, ..
        } => {
            collect_node_context(module, *callee, names, visited);
            collect_node_context(module, *argument, names, visited);
        }
        NodeKind::Binding { pattern, value, .. } => {
            collect_pattern_names(pattern, names);
            collect_node_context(module, *value, names, visited);
        }
        NodeKind::Assignment { pattern, value } => {
            collect_pattern_names(pattern, names);
            collect_node_context(module, *value, names, visited);
        }
        NodeKind::Function {
            name,
            parameter,
            body,
            ..
        } => {
            names.insert(name.text.clone());
            collect_pattern_names(parameter, names);
            collect_node_context(module, *body, names, visited);
        }
        NodeKind::Conditional {
            condition,
            consequent,
            alternative,
        } => {
            collect_node_context(module, *condition, names, visited);
            collect_node_context(module, *consequent, names, visited);
            if let Some(alternative) = alternative {
                collect_node_context(module, *alternative, names, visited);
            }
        }
        NodeKind::Loop {
            condition, body, ..
        } => {
            collect_node_context(module, *condition, names, visited);
            collect_block_context(module, *body, names, visited);
        }
        NodeKind::Return(value) | NodeKind::Break(value) => {
            if let Some(value) = value {
                collect_node_context(module, *value, names, visited);
            }
        }
        NodeKind::Throw(value) => collect_node_context(module, *value, names, visited),
        NodeKind::Match { value, arms } => {
            collect_node_context(module, *value, names, visited);
            for arm in arms {
                collect_pattern_names(&arm.pattern, names);
                collect_block_context(module, arm.body, names, visited);
            }
        }
        NodeKind::Unit
        | NodeKind::Boolean(_)
        | NodeKind::Integer(_)
        | NodeKind::Float(_)
        | NodeKind::String(_)
        | NodeKind::Symbol(_)
        | NodeKind::Placeholder
        | NodeKind::Continue
        | NodeKind::Import { .. }
        | NodeKind::NamespaceImport { .. } => {}
    }
}

fn collect_pattern_names(pattern: &Pattern, names: &mut std::collections::HashSet<Arc<str>>) {
    match pattern {
        Pattern::Capture(name) => {
            names.insert(name.text.clone());
        }
        Pattern::Literal(node) => {
            let _ = node;
        }
        Pattern::List(patterns) => {
            for pattern in patterns {
                collect_pattern_names(pattern, names);
            }
        }
        Pattern::Wildcard => {}
    }
}
