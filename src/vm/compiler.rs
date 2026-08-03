use std::{collections::HashMap, sync::Arc};

use crate::{
    diagnostic::Diagnostic,
    runtime::Value,
    syntax::ast::{
        AssignmentTarget, BindingKind, BlockId, Module, NamespaceImportSelection, NodeId, NodeKind,
        Pattern, Visibility,
    },
};

use super::analysis::ScopeAnalysis;
use super::ir::{Function, Instruction, NamespaceBinding, Program, Register};
use super::passes::PassPipeline;

mod api;
mod blocks;
mod calls;
mod context;
mod control;
mod functions;
mod loops;
mod patterns;
mod remote;

pub(crate) use api::compile_module_with_globals_and_source;
pub use api::{
    compile, compile_module, compile_module_with_globals, compile_module_with_globals_and_pipeline,
    compile_module_with_pipeline, compile_with_pipeline,
};

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
    source: Option<Arc<str>>,
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
    module_index: usize,
    construction: Option<Register>,
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
            construction: None,
            module_index,
            source: None,
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
                    mutability,
                    pattern,
                    ..
                } => collect_pattern_exports(
                    pattern,
                    &self.locals,
                    *visibility == Visibility::Public,
                    *mutability == BindingKind::Mutable,
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
                            mutable: false,
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

    fn is_this_expression(&self, id: NodeId) -> bool {
        matches!(
            &self.module.node(id).kind,
            NodeKind::Identifier(name) if name.as_ref() == "this"
        )
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
                    allow_private: self.is_this_expression(*object),
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
            NodeKind::Assignment { target, value } => match target {
                AssignmentTarget::Pattern(pattern) => {
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
                AssignmentTarget::Member(target) => {
                    let NodeKind::Member { object, member } = &self.module.node(*target).kind
                    else {
                        unreachable!("member assignment target must be a member expression");
                    };
                    let namespace = self.compile_node(*object)?;
                    self.instructions.push(Instruction::CheckMemberWritable {
                        namespace,
                        name: member.text.clone(),
                        allow_private: self.is_this_expression(*object),
                    });
                    let source = self.compile_node(*value)?;
                    self.instructions.push(Instruction::StoreMember {
                        namespace,
                        source,
                        name: member.text.clone(),
                        allow_private: self.is_this_expression(*object),
                    });
                    Some(source)
                }
            },
            NodeKind::Call {
                callee,
                argument,
                immediate,
            } => self.compile_call(*callee, *argument, *immediate, node.span),
            NodeKind::Block(block) => {
                let (function, context) = self.compile_block_function(*block);
                let destination = self.allocate_register();
                self.instructions.push(Instruction::MakeBlock {
                    destination,
                    block: block.0,
                    function,
                    context,
                    construction: self.construction,
                });
                Some(destination)
            }
            NodeKind::Conditional {
                condition,
                consequent,
                alternative,
            } => self.compile_conditional(*condition, *consequent, *alternative),
            NodeKind::Branch(arms) => self.compile_branch(arms),
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
            NodeKind::Attempt(body) => Some(self.compile_attempt(*body)),
            NodeKind::New(operand) => self.compile_new(*operand, node.span),
            NodeKind::Do(operand) => self.compile_do(*operand, node.span),
            NodeKind::Remote(expression) => self.compile_remote(*expression, node.span),
            NodeKind::Await(future) => self.compile_await(*future),
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
                        allow_private: false,
                    });
                    value = destination;
                }
                let imported = self.allocate_register();
                self.instructions.push(Instruction::LoadMember {
                    destination: imported,
                    namespace: value,
                    name: member.text.clone(),
                    allow_private: false,
                });
                let local_name = alias.as_ref().unwrap_or(member).text.clone();
                let binding = self.locals[&local_name].register;
                self.instructions.push(Instruction::BindImport {
                    binding,
                    source: imported,
                    name: local_name.clone(),
                });
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
    mutable: bool,
    exports: &mut Vec<NamespaceBinding>,
) {
    match pattern {
        Pattern::Capture(name) => {
            if let Some(local) = locals.get(&name.text) {
                exports.push(NamespaceBinding {
                    name: name.text.clone(),
                    source: local.register,
                    public,
                    mutable,
                });
            }
        }
        Pattern::List(patterns) => {
            for pattern in patterns {
                collect_pattern_exports(pattern, locals, public, mutable, exports);
            }
        }
        Pattern::Wildcard | Pattern::Literal(_) => {}
    }
}
