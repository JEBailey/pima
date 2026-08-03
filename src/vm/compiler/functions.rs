use std::{collections::HashMap, sync::Arc};

use crate::{
    runtime::Value,
    syntax::ast::{BlockId, Name, NodeId, Pattern},
};

use super::{
    Compiler, Function, Instruction, Local, LoopContext, Register, ScopeAnalysis, context,
    dense_spans, empty_function,
};

/// Suspended lowering state while a nested instruction sequence is emitted.
/// Keeping this inventory in one type prevents new compiler state from being
/// silently omitted when function lowering saves and restores its parent.
struct LoweringFrame {
    instructions: Vec<Instruction>,
    instruction_spans: HashMap<usize, crate::source::Span>,
    locals: HashMap<Arc<str>, Local>,
    loops: Vec<LoopContext>,
    attempt_depth: usize,
    next_register: u16,
    in_function: bool,
    capture_count: u16,
    binding_registers: Vec<Register>,
    construction: Option<Register>,
}

impl LoweringFrame {
    fn suspend(compiler: &mut Compiler<'_>) -> Self {
        Self {
            instructions: std::mem::take(&mut compiler.instructions),
            instruction_spans: std::mem::take(&mut compiler.instruction_spans),
            locals: std::mem::take(&mut compiler.locals),
            loops: std::mem::take(&mut compiler.loops),
            attempt_depth: std::mem::take(&mut compiler.attempt_depth),
            next_register: std::mem::take(&mut compiler.next_register),
            in_function: std::mem::take(&mut compiler.in_function),
            capture_count: std::mem::take(&mut compiler.capture_count),
            binding_registers: std::mem::take(&mut compiler.binding_registers),
            construction: compiler.construction.take(),
        }
    }

    fn restore(self, compiler: &mut Compiler<'_>) {
        compiler.instructions = self.instructions;
        compiler.instruction_spans = self.instruction_spans;
        compiler.locals = self.locals;
        compiler.loops = self.loops;
        compiler.attempt_depth = self.attempt_depth;
        compiler.next_register = self.next_register;
        compiler.in_function = self.in_function;
        compiler.capture_count = self.capture_count;
        compiler.binding_registers = self.binding_registers;
        compiler.construction = self.construction;
    }
}

impl Compiler<'_> {
    pub(super) fn compile_function(
        &mut self,
        function: u16,
        name: &Arc<str>,
        parameter: &Pattern,
        body: NodeId,
        captures: &[(Arc<str>, Local)],
    ) {
        let outer = LoweringFrame::suspend(self);

        self.next_register = 1 + captures.len() as u16;
        self.attempt_depth = 0;
        self.in_function = true;
        self.capture_count = captures.len() as u16;
        self.construction = outer.construction.and_then(|owner| {
            captures
                .iter()
                .position(|(_, local)| local.register == owner)
                .map(|index| Register(1 + index as u16))
        });
        for (index, (name, local)) in captures.iter().enumerate() {
            self.locals.insert(
                name.clone(),
                Local {
                    register: Register(1 + index as u16),
                    block: local.block,
                    binding: local.binding,
                    namespace: local.namespace,
                },
            );
        }
        self.compile_parameter_pattern(parameter, Register(0));
        let inherited_blocks = self
            .locals
            .iter()
            .filter_map(|(name, local)| local.block.map(|block| (name.clone(), block)))
            .collect();
        let analysis = ScopeAnalysis::function(self.module, body, inherited_blocks);
        for name in analysis.declarations() {
            if self.locals.get(&name.text).is_some_and(|local| {
                local.binding && local.register.0 > 0 && local.register.0 <= self.capture_count
            }) {
                let register = self.allocate_register();
                self.binding_registers.push(register);
                self.locals.insert(
                    name.text.clone(),
                    Local {
                        register,
                        block: analysis.static_block(&name.text),
                        binding: true,
                        namespace: false,
                    },
                );
            }
        }
        self.apply_scope_analysis(&analysis);
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

        outer.restore(self);
    }

    pub(super) fn compile_block_function(&mut self, block: BlockId) -> (u16, Vec<Arc<str>>) {
        if let Some(compiled) = self.block_functions.get(&block) {
            return compiled.clone();
        }
        let mut names = std::collections::HashSet::new();
        context::collect_block_context(
            self.module,
            block,
            &mut names,
            &mut std::collections::HashSet::new(),
        );
        let mut names = names.into_iter().collect::<Vec<_>>();
        names.sort();
        let function = self.compiled_functions.len() as u16;
        self.compiled_functions.push(empty_function());
        self.block_functions
            .insert(block, (function, names.clone()));

        let outer = LoweringFrame::suspend(self);
        self.next_register = 1 + names.len() as u16;
        self.attempt_depth = 0;
        self.in_function = true;
        self.capture_count = names.len() as u16;
        self.construction = outer.construction.and_then(|_| {
            names
                .iter()
                .position(|name| name.as_ref() == "this")
                .map(|index| Register(1 + index as u16))
        });
        for (index, name) in names.iter().enumerate() {
            self.locals.insert(
                name.clone(),
                Local {
                    register: Register(1 + index as u16),
                    block: None,
                    binding: true,
                    namespace: false,
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
        outer.restore(self);
        (function, names)
    }

    pub(super) fn compile_nested_function(
        &mut self,
        name: &Name,
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
            construction: self.construction,
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

    pub(super) fn apply_scope_analysis(&mut self, analysis: &ScopeAnalysis) {
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
                    namespace: false,
                },
            );
        }
    }

    pub(super) fn compile_top_level_function(&mut self, name: &Name) -> Option<Register> {
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
            construction: self.construction,
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
}
