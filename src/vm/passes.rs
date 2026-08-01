use crate::diagnostic::Diagnostic;

use super::{Instruction, Program};

/// A transformation or analysis stage applied after register IR is emitted.
///
/// Passes run in insertion order. A pass may inspect or rewrite every code body
/// through [`Program::visit_instruction_sequences_mut`].
pub trait CompilerPass {
    fn name(&self) -> &'static str;

    fn run(&self, program: &mut Program) -> Result<(), Vec<Diagnostic>>;
}

/// Ordered compiler stages run between lowering and VM execution.
#[derive(Default)]
pub struct PassPipeline {
    passes: Vec<Box<dyn CompilerPass>>,
}

impl PassPipeline {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn standard() -> Self {
        let mut pipeline = Self::new();
        pipeline.push(ControlFlowNormalization);
        pipeline
    }

    pub fn push(&mut self, pass: impl CompilerPass + 'static) -> &mut Self {
        self.passes.push(Box::new(pass));
        self
    }

    pub fn run(&self, program: &mut Program) -> Result<(), Vec<Diagnostic>> {
        for pass in &self.passes {
            pass.run(program)?;
        }
        Ok(())
    }

    pub fn pass_names(&self) -> impl Iterator<Item = &'static str> + '_ {
        self.passes.iter().map(|pass| pass.name())
    }
}

/// Removes instructions that cannot affect execution and shortens jump chains.
pub struct ControlFlowNormalization;

impl CompilerPass for ControlFlowNormalization {
    fn name(&self) -> &'static str {
        "control-flow-normalization"
    }

    fn run(&self, program: &mut Program) -> Result<(), Vec<Diagnostic>> {
        program.visit_instruction_sequences_mut(|instructions, spans| {
            thread_jumps(instructions);
            remove_no_ops(instructions, spans);
            thread_jumps(instructions);
        });
        Ok(())
    }
}

fn thread_jumps(instructions: &mut [Instruction]) {
    let targets = instructions
        .iter()
        .map(|instruction| resolve_jump_target(instructions, instruction.target()))
        .collect::<Vec<_>>();
    for (instruction, target) in instructions.iter_mut().zip(targets) {
        if let Some(target) = target {
            instruction.set_target(target);
        }
    }
}

fn resolve_jump_target(instructions: &[Instruction], target: Option<usize>) -> Option<usize> {
    let mut target = target?;
    for _ in 0..instructions.len() {
        match instructions.get(target) {
            Some(Instruction::Jump { target: next }) if *next != target => target = *next,
            _ => return Some(target),
        }
    }
    Some(target)
}

fn remove_no_ops(
    instructions: &mut Vec<Instruction>,
    spans: &mut Vec<Option<crate::source::Span>>,
) {
    let old_len = instructions.len();
    let removed = instructions
        .iter()
        .enumerate()
        .map(|(index, instruction)| {
            matches!(instruction, Instruction::Move { destination, source } if destination == source)
                || matches!(instruction, Instruction::Jump { target } if *target == index + 1)
        })
        .collect::<Vec<_>>();
    if !removed.iter().any(|removed| *removed) {
        return;
    }

    let mut remap = vec![0; old_len + 1];
    let mut retained = 0;
    for index in 0..old_len {
        remap[index] = retained;
        if !removed[index] {
            retained += 1;
        }
    }
    remap[old_len] = retained;

    let mut index = 0;
    instructions.retain(|_| {
        let keep = !removed[index];
        index += 1;
        keep
    });
    let mut index = 0;
    spans.retain(|_| {
        let keep = !removed[index];
        index += 1;
        keep
    });
    for instruction in instructions {
        if let Some(target) = instruction.target() {
            instruction.set_target(remap[target.min(old_len)]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        source::SourceMap,
        syntax::{lexer::lex, parser::parse},
        vm::{Register, compile_with_pipeline},
    };
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    #[test]
    fn normalizes_jump_chains_and_no_ops() {
        let mut instructions = vec![
            Instruction::Move {
                destination: Register(0),
                source: Register(0),
            },
            Instruction::Jump { target: 2 },
            Instruction::Jump { target: 4 },
            Instruction::Jump { target: 4 },
            Instruction::Return {
                source: Register(0),
            },
        ];
        let mut spans = vec![None; instructions.len()];
        thread_jumps(&mut instructions);
        remove_no_ops(&mut instructions, &mut spans);
        thread_jumps(&mut instructions);

        assert_eq!(instructions.len(), 3);
        assert_eq!(spans.len(), instructions.len());
        assert!(matches!(instructions[0], Instruction::Jump { target: 2 }));
    }

    struct CountBodies(Arc<AtomicUsize>);

    impl CompilerPass for CountBodies {
        fn name(&self) -> &'static str {
            "count-bodies"
        }

        fn run(&self, program: &mut Program) -> Result<(), Vec<Diagnostic>> {
            program.visit_instruction_sequences_mut(|_, _| {
                self.0.fetch_add(1, Ordering::Relaxed);
            });
            Ok(())
        }
    }

    #[test]
    fn custom_pipeline_visits_module_and_function_bodies() {
        let mut sources = SourceMap::default();
        let source = "function identity :value { value }\n[identity 42]";
        let source_id = sources.add("<pass-test>", source);
        let tokens = lex(source_id, source).unwrap();
        let module = parse(&tokens).unwrap();
        let count = Arc::new(AtomicUsize::new(0));
        let mut pipeline = PassPipeline::new();
        pipeline.push(CountBodies(count.clone()));

        compile_with_pipeline(&module, &pipeline).unwrap();

        assert_eq!(count.load(Ordering::Relaxed), 2);
    }
}
