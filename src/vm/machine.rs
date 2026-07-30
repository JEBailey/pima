use crate::{diagnostic::Diagnostic, runtime::Value};

use super::ir::{Instruction, Primitive, Program};

#[derive(Default)]
pub struct Machine;

impl Machine {
    pub fn execute(&mut self, program: &Program) -> Result<Value, Diagnostic> {
        let mut frames = vec![Frame {
            function: None,
            instruction_pointer: 0,
            registers: vec![Slot::Value(Value::Unit); program.register_count as usize],
            return_destination: None,
        }];
        loop {
            let frame_index = frames
                .len()
                .checked_sub(1)
                .expect("VM must retain an entry frame until return");
            let instructions = match frames[frame_index].function {
                Some(function) => &program.functions[function as usize].instructions,
                None => &program.instructions,
            };
            let Some(instruction) = instructions.get(frames[frame_index].instruction_pointer)
            else {
                return Err(Diagnostic::error("register VM program did not return"));
            };
            frames[frame_index].instruction_pointer += 1;
            let frame = &mut frames[frame_index];
            match instruction {
                Instruction::LoadConstant {
                    destination,
                    constant,
                } => {
                    frame.registers[destination.0 as usize] =
                        Slot::Value(program.constants[*constant as usize].clone());
                }
                Instruction::Move {
                    destination,
                    source,
                } => {
                    frame.registers[destination.0 as usize] =
                        frame.registers[source.0 as usize].clone();
                }
                Instruction::MakeCell {
                    destination,
                    source,
                } => {
                    frame.registers[destination.0 as usize] = Slot::Cell(std::rc::Rc::new(
                        std::cell::RefCell::new(frame.registers[source.0 as usize].clone()),
                    ));
                }
                Instruction::LoadCell { destination, cell } => {
                    let Slot::Cell(cell) = frame.registers[cell.0 as usize].clone() else {
                        return Err(Diagnostic::error("LOAD_CELL requires a cell"));
                    };
                    let value = cell.borrow().clone();
                    frame.registers[destination.0 as usize] = value;
                }
                Instruction::StoreCell { cell, source } => {
                    let value = frame.registers[source.0 as usize].clone();
                    if matches!(value, Slot::Closure { .. } | Slot::Cell(_)) {
                        return Err(Diagnostic::error(
                            "storing VM closures or cells in captured cells is not supported yet",
                        ));
                    }
                    let Slot::Cell(cell) = frame.registers[cell.0 as usize].clone() else {
                        return Err(Diagnostic::error("STORE_CELL requires a cell"));
                    };
                    *cell.borrow_mut() = value;
                }
                Instruction::MakeList {
                    destination,
                    elements,
                } => {
                    let values = elements
                        .iter()
                        .map(|register| language_value(&frame.registers[register.0 as usize]))
                        .collect::<Result<Vec<_>, _>>()?;
                    frame.registers[destination.0 as usize] =
                        Slot::Value(Value::List(values.into_iter().collect()));
                }
                Instruction::CheckListLength { source, length } => {
                    let Slot::Value(Value::List(list)) = &frame.registers[source.0 as usize] else {
                        return Err(pattern_error());
                    };
                    if list.len() != *length as usize {
                        return Err(pattern_error());
                    }
                }
                Instruction::ListGet {
                    destination,
                    source,
                    index,
                } => {
                    let Slot::Value(Value::List(list)) = &frame.registers[source.0 as usize] else {
                        return Err(Diagnostic::error("LIST_GET requires a list"));
                    };
                    let Some(value) = list.iter().nth(*index as usize).cloned() else {
                        return Err(Diagnostic::error("LIST_GET index is out of range"));
                    };
                    frame.registers[destination.0 as usize] = Slot::Value(value);
                }
                Instruction::CallPrimitive {
                    destination,
                    primitive,
                    arguments,
                } => {
                    let arguments = arguments
                        .iter()
                        .map(|register| language_value_ref(&frame.registers[register.0 as usize]))
                        .collect::<Result<Vec<_>, _>>()?;
                    frame.registers[destination.0 as usize] =
                        Slot::Value(evaluate_primitive(*primitive, &arguments)?);
                }
                Instruction::Jump { target } => frame.instruction_pointer = *target,
                Instruction::JumpIfFalse { condition, target } => {
                    match frame.registers[condition.0 as usize] {
                        Slot::Value(Value::Boolean(false)) => frame.instruction_pointer = *target,
                        Slot::Value(Value::Boolean(true)) => {}
                        _ => return Err(Diagnostic::error("condition must be boolean")),
                    }
                }
                Instruction::JumpIfTrue { condition, target } => {
                    match frame.registers[condition.0 as usize] {
                        Slot::Value(Value::Boolean(true)) => frame.instruction_pointer = *target,
                        Slot::Value(Value::Boolean(false)) => {}
                        _ => return Err(Diagnostic::error("condition must be boolean")),
                    }
                }
                Instruction::Call {
                    destination,
                    function,
                    argument,
                } => {
                    let Some(compiled) = program.functions.get(*function as usize) else {
                        return Err(Diagnostic::error("invalid VM function id"));
                    };
                    let argument = frame.registers[argument.0 as usize].clone();
                    let mut registers =
                        vec![Slot::Value(Value::Unit); compiled.register_count as usize];
                    registers[0] = argument;
                    frames.push(Frame {
                        function: Some(*function),
                        instruction_pointer: 0,
                        registers,
                        return_destination: Some(*destination),
                    });
                }
                Instruction::MakeClosure {
                    destination,
                    function,
                    captures,
                } => {
                    frame.registers[destination.0 as usize] = Slot::Closure {
                        function: *function,
                        captures: captures
                            .iter()
                            .map(|register| frame.registers[register.0 as usize].clone())
                            .collect(),
                    };
                }
                Instruction::CallDynamic {
                    destination,
                    callee,
                    argument,
                } => {
                    let Slot::Closure { function, captures } =
                        frame.registers[callee.0 as usize].clone()
                    else {
                        return Err(Diagnostic::error("dynamic callee is not a VM closure"));
                    };
                    let compiled = &program.functions[function as usize];
                    if captures.len() != compiled.capture_count as usize {
                        return Err(Diagnostic::error("VM closure capture count mismatch"));
                    }
                    let mut registers =
                        vec![Slot::Value(Value::Unit); compiled.register_count as usize];
                    registers[0] = frame.registers[argument.0 as usize].clone();
                    for (index, capture) in captures.into_iter().enumerate() {
                        registers[index + 1] = capture;
                    }
                    frames.push(Frame {
                        function: Some(function),
                        instruction_pointer: 0,
                        registers,
                        return_destination: Some(*destination),
                    });
                }
                Instruction::Return { source } => {
                    let value = frame.registers[source.0 as usize].clone();
                    let destination = frame.return_destination;
                    frames.pop();
                    let Some(caller) = frames.last_mut() else {
                        return language_value(&value);
                    };
                    caller.registers
                        [destination.expect("function frame has a destination").0 as usize] = value;
                }
            }
        }
    }
}

struct Frame {
    function: Option<u16>,
    instruction_pointer: usize,
    registers: Vec<Slot>,
    return_destination: Option<super::ir::Register>,
}

#[derive(Clone)]
enum Slot {
    Value(Value),
    Closure { function: u16, captures: Vec<Slot> },
    Cell(std::rc::Rc<std::cell::RefCell<Slot>>),
}

fn language_value(slot: &Slot) -> Result<Value, Diagnostic> {
    language_value_ref(slot).cloned()
}

fn language_value_ref(slot: &Slot) -> Result<&Value, Diagnostic> {
    match slot {
        Slot::Value(value) => Ok(value),
        Slot::Closure { .. } | Slot::Cell(_) => Err(Diagnostic::error(
            "VM closure cannot cross this language-value boundary yet",
        )),
    }
}

fn pattern_error() -> Diagnostic {
    Diagnostic::error("function argument does not match its parameter pattern")
}

fn evaluate_primitive(primitive: Primitive, arguments: &[&Value]) -> Result<Value, Diagnostic> {
    match primitive {
        Primitive::Add => fold_numeric(arguments, "addition", i64::checked_add, |a, b| a + b),
        Primitive::Subtract => {
            fold_numeric(arguments, "subtraction", i64::checked_sub, |a, b| a - b)
        }
        Primitive::Multiply => {
            fold_numeric(arguments, "multiplication", i64::checked_mul, |a, b| a * b)
        }
        Primitive::Divide => {
            let [left, right] = arguments else {
                return Err(Diagnostic::error("division requires two arguments"));
            };
            let denominator = numeric(right)?;
            if denominator == 0.0 {
                return Err(Diagnostic::error("division by zero"));
            }
            Ok(Value::Float(numeric(left)? / denominator))
        }
        Primitive::IntegerDivide => {
            let [Value::Integer(left), Value::Integer(right)] = arguments else {
                return Err(Diagnostic::error("div requires integer arguments"));
            };
            left.checked_div(*right)
                .map(Value::Integer)
                .ok_or_else(|| Diagnostic::error("invalid integer division"))
        }
        Primitive::Modulo => {
            let [Value::Integer(left), Value::Integer(right)] = arguments else {
                return Err(Diagnostic::error("mod requires integer arguments"));
            };
            if *right == 0 {
                return Err(Diagnostic::error("division by zero"));
            }
            Ok(Value::Integer(if *left == i64::MIN && *right == -1 {
                0
            } else {
                left.rem_euclid(*right)
            }))
        }
        Primitive::LessThan => compare(arguments, |left, right| left < right),
        Primitive::GreaterThan => compare(arguments, |left, right| left > right),
        Primitive::Equal => {
            let [left, right] = arguments else {
                return Err(Diagnostic::error("equality requires two arguments"));
            };
            Ok(Value::Boolean(*left == *right))
        }
    }
}

fn fold_numeric(
    arguments: &[&Value],
    operation: &str,
    integer: fn(i64, i64) -> Option<i64>,
    float: fn(f64, f64) -> f64,
) -> Result<Value, Diagnostic> {
    if arguments.len() < 2 {
        return Err(Diagnostic::error(format!(
            "{operation} requires at least two arguments"
        )));
    }
    let Some(first) = arguments.first() else {
        return Err(Diagnostic::error(format!("{operation} requires arguments")));
    };
    let mut accumulator = (*first).clone();
    for argument in &arguments[1..] {
        accumulator = match (&accumulator, *argument) {
            (Value::Integer(left), Value::Integer(right)) => integer(*left, *right)
                .map(Value::Integer)
                .ok_or_else(|| Diagnostic::error(format!("integer overflow in {operation}")))?,
            _ => Value::Float(float(numeric(&accumulator)?, numeric(argument)?)),
        };
    }
    Ok(accumulator)
}

fn compare(arguments: &[&Value], comparison: fn(f64, f64) -> bool) -> Result<Value, Diagnostic> {
    let [left, right] = arguments else {
        return Err(Diagnostic::error("comparison requires two arguments"));
    };
    Ok(Value::Boolean(comparison(numeric(left)?, numeric(right)?)))
}

fn numeric(value: &Value) -> Result<f64, Diagnostic> {
    match value {
        Value::Integer(value) => Ok(*value as f64),
        Value::Float(value) => Ok(*value),
        _ => Err(Diagnostic::error("numeric argument required")),
    }
}
