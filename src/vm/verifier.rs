use crate::diagnostic::Diagnostic;

use super::{Instruction, Program, Register};

pub(crate) fn verify(program: &Program) -> Result<(), Vec<Diagnostic>> {
    let mut diagnostics = Vec::new();
    verify_body(
        "module",
        &program.instructions,
        &program.instruction_spans,
        program.register_count,
        program.constants.len(),
        program.functions.len(),
        &mut diagnostics,
    );
    for (index, function) in program.functions.iter().enumerate() {
        verify_body(
            &format!("function {index} ({})", function.name),
            &function.instructions,
            &function.instruction_spans,
            function.register_count,
            program.constants.len(),
            program.functions.len(),
            &mut diagnostics,
        );
        if function.capture_count + 1 > function.register_count {
            diagnostics.push(Diagnostic::error(format!(
                "function {index} capture layout exceeds its register frame"
            )));
        }
        verify_registers(
            "binding",
            &function.binding_registers,
            function.register_count,
            &mut diagnostics,
        );
    }
    verify_registers(
        "module binding",
        &program.binding_registers,
        program.register_count,
        &mut diagnostics,
    );
    verify_registers(
        "initial binding",
        &program
            .initial_bindings
            .iter()
            .map(|(register, _)| *register)
            .collect::<Vec<_>>(),
        program.register_count,
        &mut diagnostics,
    );
    if diagnostics.is_empty() {
        Ok(())
    } else {
        Err(diagnostics)
    }
}

fn verify_body(
    name: &str,
    instructions: &[Instruction],
    spans: &[Option<crate::source::Span>],
    register_count: u16,
    constant_count: usize,
    function_count: usize,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if instructions.len() != spans.len() {
        diagnostics.push(Diagnostic::error(format!(
            "{name} instruction/span counts differ"
        )));
    }
    for (index, instruction) in instructions.iter().enumerate() {
        instruction.visit_registers(|register| {
            if register.0 >= register_count {
                diagnostics.push(Diagnostic::error(format!("{name} instruction {index} references r{} outside its {register_count}-register frame", register.0)));
            }
        });
        if instruction
            .target()
            .is_some_and(|target| target >= instructions.len())
        {
            diagnostics.push(Diagnostic::error(format!(
                "{name} instruction {index} has an invalid control-flow target"
            )));
        }
        match instruction {
            Instruction::LoadConstant { constant, .. } if *constant as usize >= constant_count => {
                diagnostics.push(Diagnostic::error(format!(
                    "{name} instruction {index} references missing constant {constant}"
                )))
            }
            Instruction::MakeClosure { function, .. } | Instruction::MakeBlock { function, .. }
                if *function as usize >= function_count =>
            {
                diagnostics.push(Diagnostic::error(format!(
                    "{name} instruction {index} references missing function {function}"
                )))
            }
            _ => {}
        }
    }
}

fn verify_registers(
    kind: &str,
    registers: &[Register],
    count: u16,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for register in registers {
        if register.0 >= count {
            diagnostics.push(Diagnostic::error(format!(
                "{kind} r{} is outside its {count}-register frame",
                register.0
            )));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::Value;

    fn program(instructions: Vec<Instruction>, register_count: u16) -> Program {
        Program {
            id: 1,
            constants: vec![Value::Unit],
            instruction_spans: vec![None; instructions.len()],
            instructions,
            register_count,
            functions: Vec::new(),
            binding_registers: Vec::new(),
            initial_bindings: Vec::new(),
            module_index: 0,
        }
    }

    #[test]
    fn accepts_a_well_formed_body() {
        assert!(
            verify(&program(
                vec![Instruction::Return {
                    source: Register(0)
                }],
                1
            ))
            .is_ok()
        );
    }

    #[test]
    fn rejects_register_constant_and_jump_overruns() {
        let invalid = program(
            vec![
                Instruction::LoadConstant {
                    destination: Register(2),
                    constant: 4,
                },
                Instruction::Jump { target: 9 },
            ],
            1,
        );
        let diagnostics = verify(&invalid).unwrap_err();
        assert_eq!(diagnostics.len(), 3);
    }
}
