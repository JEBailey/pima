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

/// Reuses temporary registers whose conservative instruction intervals do not
/// overlap. Argument/capture ABI registers remain fixed and binding cells are
/// assigned dedicated frame slots for the complete body lifetime.
pub struct RegisterAllocation;

impl CompilerPass for RegisterAllocation {
    fn name(&self) -> &'static str {
        "register-allocation"
    }

    fn run(&self, program: &mut Program) -> Result<(), Vec<Diagnostic>> {
        let (mapping, count) = allocate_registers(
            &mut program.instructions,
            program.register_count,
            0,
            &program.binding_registers,
        );
        remap_metadata(&mapping, &mut program.binding_registers);
        for (register, _) in &mut program.initial_bindings {
            *register = mapping[register.0 as usize];
        }
        program.register_count = count;
        for function in &mut program.functions {
            let (mapping, count) = allocate_registers(
                &mut function.instructions,
                function.register_count,
                function.capture_count + 1,
                &function.binding_registers,
            );
            remap_metadata(&mapping, &mut function.binding_registers);
            function.register_count = count;
        }
        Ok(())
    }
}

fn allocate_registers(
    instructions: &mut [Instruction],
    register_count: u16,
    fixed_prefix: u16,
    bindings: &[super::Register],
) -> (Vec<super::Register>, u16) {
    let mut intervals = vec![None::<(usize, usize)>; register_count as usize];
    for (index, instruction) in instructions.iter().enumerate() {
        instruction.visit_registers(|register| {
            let interval = &mut intervals[register.0 as usize];
            *interval = Some(match *interval {
                Some((first, _)) => (first, index),
                None => (index, index),
            });
        });
    }
    let mut mapping = vec![super::Register(0); register_count as usize];
    let mut reserved = Vec::<u16>::new();
    for register in 0..fixed_prefix.min(register_count) {
        mapping[register as usize] = super::Register(register);
        reserved.push(register);
    }
    for binding in bindings {
        if binding.0 < fixed_prefix {
            continue;
        }
        let physical = first_available(&reserved, &[]);
        mapping[binding.0 as usize] = super::Register(physical);
        reserved.push(physical);
    }
    let mut candidates = intervals
        .iter()
        .enumerate()
        .filter_map(|(register, interval)| {
            let interval = (*interval)?;
            (register >= fixed_prefix as usize
                && !bindings
                    .iter()
                    .any(|binding| binding.0 as usize == register))
            .then_some((register, interval))
        })
        .collect::<Vec<_>>();
    candidates.sort_by_key(|(_, (first, _))| *first);
    let mut active = Vec::<(usize, u16)>::new();
    for (register, (first, last)) in candidates {
        active.retain(|(active_last, _)| *active_last >= first);
        let occupied = active
            .iter()
            .map(|(_, physical)| *physical)
            .collect::<Vec<_>>();
        let physical = first_available(&reserved, &occupied);
        mapping[register] = super::Register(physical);
        active.push((last, physical));
    }
    instructions
        .iter_mut()
        .for_each(|instruction| instruction.remap_registers(&mapping));
    let count = mapping
        .iter()
        .map(|register| register.0)
        .max()
        .map_or(0, |maximum| maximum + 1);
    (mapping, count)
}

fn first_available(reserved: &[u16], occupied: &[u16]) -> u16 {
    (0..u16::MAX)
        .find(|candidate| !reserved.contains(candidate) && !occupied.contains(candidate))
        .expect("VM register limit exceeded")
}

fn remap_metadata(mapping: &[super::Register], registers: &mut [super::Register]) {
    registers
        .iter_mut()
        .for_each(|register| *register = mapping[register.0 as usize]);
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

    #[test]
    fn register_allocation_reuses_non_overlapping_temporaries() {
        let mut sources = SourceMap::default();
        let source = "+ 1 2\n+ 3 4\n+ 5 6\n+ 7 8\n+ 9 10";
        let source_id = sources.add("<register-allocation-test>", source);
        let tokens = lex(source_id, source).unwrap();
        let module = parse(&tokens).unwrap();
        let mut unallocated_pipeline = PassPipeline::new();
        unallocated_pipeline.push(ControlFlowNormalization);
        let unallocated = compile_with_pipeline(&module, &unallocated_pipeline).unwrap();
        let mut allocated_pipeline = PassPipeline::standard();
        allocated_pipeline.push(RegisterAllocation);
        let allocated = compile_with_pipeline(&module, &allocated_pipeline).unwrap();

        assert!(allocated.register_count() < unallocated.register_count());
        assert!(allocated.register_count() <= 4);
    }
}
