use crate::{
    diagnostic::Diagnostic,
    native::{NativeContext, NativeDefinition, NativeRegistry},
    runtime::{SymbolId, Value, VmCell, VmClosure, VmValue as Slot},
};

use super::ir::{Instruction, Primitive, Program};
use super::native_context::VmNativeContext;

pub struct Machine {
    primitive_natives: Vec<NativeDefinition>,
    context: VmNativeContext,
}

impl Default for Machine {
    fn default() -> Self {
        let mut natives = NativeRegistry::default();
        crate::native::numbers::register(&mut natives);
        let primitive_natives = PRIMITIVES
            .iter()
            .map(|(_, name)| {
                let id = natives
                    .find_id(name)
                    .unwrap_or_else(|| panic!("native primitive `{name}` must be registered"));
                natives
                    .get(id)
                    .unwrap_or_else(|| panic!("native primitive `{name}` must exist"))
                    .clone()
            })
            .collect();
        Self {
            primitive_natives,
            context: VmNativeContext::default(),
        }
    }
}

#[derive(Debug)]
pub enum VmError {
    Typed(Value),
    Internal(Diagnostic),
}

impl VmError {
    fn internal(message: impl Into<String>) -> Self {
        Self::Internal(Diagnostic::error(message))
    }

    pub fn value(&self) -> Option<&Value> {
        match self {
            Self::Typed(value) => Some(value),
            Self::Internal(_) => None,
        }
    }
}

impl From<Diagnostic> for VmError {
    fn from(value: Diagnostic) -> Self {
        Self::Internal(value)
    }
}

impl Machine {
    pub fn resolve_symbol(&self, symbol: SymbolId) -> Option<&str> {
        self.context.resolve(symbol)
    }

    pub fn execute(&mut self, program: &Program) -> Result<Value, VmError> {
        let mut frames = vec![Frame {
            function: None,
            instruction_pointer: 0,
            registers: initialize_registers(program.register_count, &program.binding_registers),
            return_destination: None,
        }];
        let mut handlers = Vec::new();
        'dispatch: loop {
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
                return Err(VmError::internal("register VM program did not return"));
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
                Instruction::LoadSymbol { destination, name } => {
                    frame.registers[destination.0 as usize] =
                        Slot::Value(Value::Symbol(self.context.intern_symbol(name)));
                }
                Instruction::MakeBlock { destination, block } => {
                    frame.registers[destination.0 as usize] = Slot::Value(Value::Block(
                        dumpster::unsync::Gc::new(crate::engine::StoredBlock {
                            module_index: program.module_index,
                            block_id: crate::syntax::ast::BlockId(*block),
                        }),
                    ));
                }
                Instruction::Move {
                    destination,
                    source,
                } => {
                    frame.registers[destination.0 as usize] =
                        frame.registers[source.0 as usize].clone();
                }
                Instruction::Bind {
                    binding,
                    source,
                    mutable,
                    name,
                } => {
                    let Slot::Cell(cell) = frame.registers[binding.0 as usize].clone() else {
                        return Err(VmError::internal("BIND requires a binding cell"));
                    };
                    if !matches!(*cell.value.borrow(), Slot::Uninitialized) {
                        let error = self.context.typed_error(
                            &["error", "name_error"],
                            format!("duplicate binding `{name}` in current scope"),
                        );
                        catch_typed_error(&mut frames, &mut handlers, error)?;
                        continue 'dispatch;
                    }
                    let value = frame.registers[source.0 as usize].clone();
                    *cell.value.borrow_mut() = value;
                    cell.mutable.set(Some(*mutable));
                }
                Instruction::LoadBinding {
                    destination,
                    binding,
                    name,
                } => match frame.registers[binding.0 as usize].clone() {
                    Slot::Cell(cell) if cell.mutable.get().is_none() => {
                        let error = self.context.typed_error(
                            &["error", "name_error"],
                            format!("unbound identifier `{name}`"),
                        );
                        catch_typed_error(&mut frames, &mut handlers, error)?;
                        continue 'dispatch;
                    }
                    Slot::Cell(cell) => {
                        frame.registers[destination.0 as usize] = cell.value.borrow().clone();
                    }
                    value => frame.registers[destination.0 as usize] = value,
                },
                Instruction::StoreBinding {
                    binding,
                    source,
                    name,
                } => {
                    let value = frame.registers[source.0 as usize].clone();
                    match frame.registers[binding.0 as usize].clone() {
                        Slot::Cell(cell) if cell.mutable.get().is_none() => {
                            let error = self.context.typed_error(
                                &["error", "name_error"],
                                format!("unbound identifier `{name}` for assignment"),
                            );
                            catch_typed_error(&mut frames, &mut handlers, error)?;
                            continue 'dispatch;
                        }
                        Slot::Cell(cell) if cell.mutable.get() == Some(false) => {
                            let error = self.context.typed_error(
                                &["error", "mutation_error"],
                                format!("cannot assign to immutable binding `{name}`"),
                            );
                            catch_typed_error(&mut frames, &mut handlers, error)?;
                            continue 'dispatch;
                        }
                        Slot::Cell(cell) => *cell.value.borrow_mut() = value,
                        _ => {
                            return Err(VmError::internal("STORE_BINDING requires a binding cell"));
                        }
                    }
                }
                Instruction::MakeCell {
                    destination,
                    source,
                } => {
                    frame.registers[destination.0 as usize] =
                        Slot::Cell(dumpster::unsync::Gc::new(VmCell::new(
                            frame.registers[source.0 as usize].clone(),
                        )));
                }
                Instruction::LoadCell { destination, cell } => {
                    let Slot::Cell(cell) = frame.registers[cell.0 as usize].clone() else {
                        return Err(VmError::internal("LOAD_CELL requires a cell"));
                    };
                    let value = cell.value.borrow().clone();
                    frame.registers[destination.0 as usize] = value;
                }
                Instruction::StoreCell { cell, source } => {
                    let value = frame.registers[source.0 as usize].clone();
                    let Slot::Cell(cell) = frame.registers[cell.0 as usize].clone() else {
                        return Err(VmError::internal("STORE_CELL requires a cell"));
                    };
                    *cell.value.borrow_mut() = value;
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
                Instruction::MakeNamespace {
                    destination,
                    bindings,
                } => {
                    let bindings = bindings
                        .iter()
                        .map(|binding| {
                            Ok((
                                binding.name.clone(),
                                binding.public,
                                language_value(&frame.registers[binding.source.0 as usize])?,
                            ))
                        })
                        .collect::<Result<Vec<_>, Diagnostic>>()?;
                    match self.context.make_namespace(bindings) {
                        Ok(namespace) => {
                            frame.registers[destination.0 as usize] = Slot::Value(namespace);
                        }
                        Err(error) => {
                            catch_typed_error(&mut frames, &mut handlers, error)?;
                            continue 'dispatch;
                        }
                    }
                }
                Instruction::LoadMember {
                    destination,
                    namespace,
                    name,
                } => {
                    let namespace = language_value(&frame.registers[namespace.0 as usize])?;
                    match self.context.load_member(namespace, name) {
                        Ok(value) => {
                            frame.registers[destination.0 as usize] = Slot::Value(value);
                        }
                        Err(error) => {
                            catch_typed_error(&mut frames, &mut handlers, error)?;
                            continue 'dispatch;
                        }
                    }
                }
                Instruction::CheckListLength {
                    source,
                    length,
                    message,
                } => {
                    let matches = matches!(
                        &frame.registers[source.0 as usize],
                        Slot::Value(Value::List(list)) if list.len() == *length as usize
                    );
                    if !matches {
                        let error = self
                            .context
                            .typed_error(&["error", "match_error"], message.to_string());
                        catch_typed_error(&mut frames, &mut handlers, error)?;
                        continue 'dispatch;
                    }
                }
                Instruction::JumpIfNotListLength {
                    source,
                    length,
                    target,
                } => match &frame.registers[source.0 as usize] {
                    Slot::Value(Value::List(list)) if list.len() == *length as usize => {}
                    _ => frame.instruction_pointer = *target,
                },
                Instruction::JumpIfNotEqual {
                    left,
                    right,
                    target,
                } => {
                    let left = language_value(&frame.registers[left.0 as usize])?;
                    let right = language_value(&frame.registers[right.0 as usize])?;
                    if left != right {
                        frame.instruction_pointer = *target;
                    }
                }
                Instruction::ListGet {
                    destination,
                    source,
                    index,
                } => {
                    let Slot::Value(Value::List(list)) = &frame.registers[source.0 as usize] else {
                        return Err(VmError::internal("LIST_GET requires a list"));
                    };
                    let Some(value) = list.iter().nth(*index as usize).cloned() else {
                        return Err(VmError::internal("LIST_GET index is out of range"));
                    };
                    frame.registers[destination.0 as usize] = Slot::Value(value);
                }
                Instruction::CallPrimitive {
                    destination,
                    primitive,
                    arguments,
                } => {
                    let mut inline_arguments: [Value; 4] = std::array::from_fn(|_| Value::Unit);
                    let heap_arguments;
                    let arguments = if arguments.len() <= inline_arguments.len() {
                        for (index, register) in arguments.iter().enumerate() {
                            inline_arguments[index] =
                                language_value_ref(&frame.registers[register.0 as usize])?.clone();
                        }
                        &inline_arguments[..arguments.len()]
                    } else {
                        heap_arguments = arguments
                            .iter()
                            .map(|register| {
                                language_value_ref(&frame.registers[register.0 as usize]).cloned()
                            })
                            .collect::<Result<Vec<_>, _>>()?;
                        heap_arguments.as_slice()
                    };
                    let definition = &self.primitive_natives[primitive_index(*primitive)];
                    match (definition.call)(&mut self.context, arguments) {
                        Ok(value) => {
                            frame.registers[destination.0 as usize] = Slot::Value(value);
                        }
                        Err(error) => {
                            catch_typed_error(&mut frames, &mut handlers, error)?;
                            continue 'dispatch;
                        }
                    }
                }
                Instruction::Jump { target } => frame.instruction_pointer = *target,
                Instruction::JumpIfFalse {
                    condition,
                    target,
                    message,
                } => match frame.registers[condition.0 as usize] {
                    Slot::Value(Value::Boolean(false)) => frame.instruction_pointer = *target,
                    Slot::Value(Value::Boolean(true)) => {}
                    _ => {
                        let error = self
                            .context
                            .typed_error(&["error", "type_error"], message.to_string());
                        catch_typed_error(&mut frames, &mut handlers, error)?;
                        continue 'dispatch;
                    }
                },
                Instruction::JumpIfTrue {
                    condition,
                    target,
                    message,
                } => match frame.registers[condition.0 as usize] {
                    Slot::Value(Value::Boolean(true)) => frame.instruction_pointer = *target,
                    Slot::Value(Value::Boolean(false)) => {}
                    _ => {
                        let error = self
                            .context
                            .typed_error(&["error", "type_error"], message.to_string());
                        catch_typed_error(&mut frames, &mut handlers, error)?;
                        continue 'dispatch;
                    }
                },
                Instruction::MakeClosure {
                    destination,
                    function,
                    captures,
                } => {
                    frame.registers[destination.0 as usize] =
                        Slot::Value(Value::VmClosure(dumpster::unsync::Gc::new(VmClosure {
                            function: *function,
                            captures: captures
                                .iter()
                                .map(|register| frame.registers[register.0 as usize].clone())
                                .collect(),
                        })));
                }
                Instruction::CallDynamic {
                    destination,
                    callee,
                    argument,
                } => {
                    let callee = frame.registers[callee.0 as usize].clone();
                    let Slot::Value(Value::VmClosure(closure)) = callee else {
                        let value = language_value(&callee)?;
                        let error = self.context.typed_error(
                            &["error", "type_error"],
                            format!("cannot call value of type {}", value.type_symbol()),
                        );
                        catch_typed_error(&mut frames, &mut handlers, error)?;
                        continue 'dispatch;
                    };
                    let function = closure.function;
                    let captures = closure.captures.clone();
                    let compiled = &program.functions[function as usize];
                    if captures.len() != compiled.capture_count as usize {
                        return Err(VmError::internal("VM closure capture count mismatch"));
                    }
                    let mut registers =
                        initialize_registers(compiled.register_count, &compiled.binding_registers);
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
                Instruction::BeginAttempt {
                    destination,
                    catch_target,
                } => handlers.push(Handler {
                    frame_index,
                    destination: *destination,
                    catch_target: *catch_target,
                }),
                Instruction::EndAttempt => {
                    let Some(handler) = handlers.pop() else {
                        return Err(VmError::internal("END_ATTEMPT has no active handler"));
                    };
                    if handler.frame_index != frame_index {
                        return Err(VmError::internal(
                            "END_ATTEMPT does not match the active frame",
                        ));
                    }
                }
                Instruction::Throw { source } => {
                    let value = language_value(&frame.registers[source.0 as usize])?;
                    let error = self.context.validate_thrown(value);
                    catch_typed_error(&mut frames, &mut handlers, error)?;
                    continue 'dispatch;
                }
                Instruction::RaiseTyped { types, message } => {
                    let type_names = types.iter().map(AsRef::as_ref).collect::<Vec<_>>();
                    let error = self.context.typed_error(&type_names, message.to_string());
                    catch_typed_error(&mut frames, &mut handlers, error)?;
                    continue 'dispatch;
                }
                Instruction::Return { source } => {
                    let value = frame.registers[source.0 as usize].clone();
                    let destination = frame.return_destination;
                    frames.pop();
                    handlers.retain(|handler| handler.frame_index < frame_index);
                    let Some(caller) = frames.last_mut() else {
                        return Ok(language_value(&value)?);
                    };
                    caller.registers
                        [destination.expect("function frame has a destination").0 as usize] = value;
                }
            }
        }
    }
}

fn language_value(value: &Slot) -> Result<Value, Diagnostic> {
    language_value_ref(value).cloned()
}

fn language_value_ref(value: &Slot) -> Result<&Value, Diagnostic> {
    match value {
        Slot::Value(value) => Ok(value),
        Slot::Uninitialized | Slot::Cell(_) => Err(Diagnostic::error(
            "internal VM storage cannot cross the language-value boundary",
        )),
    }
}

struct Frame {
    function: Option<u16>,
    instruction_pointer: usize,
    registers: Vec<Slot>,
    return_destination: Option<super::ir::Register>,
}

fn initialize_registers(
    register_count: u16,
    binding_registers: &[super::ir::Register],
) -> Vec<Slot> {
    let mut registers = vec![Slot::Uninitialized; register_count as usize];
    for register in binding_registers {
        registers[register.0 as usize] = Slot::Cell(dumpster::unsync::Gc::new(VmCell::binding()));
    }
    registers
}

struct Handler {
    frame_index: usize,
    destination: super::ir::Register,
    catch_target: usize,
}

fn catch_typed_error(
    frames: &mut Vec<Frame>,
    handlers: &mut Vec<Handler>,
    error: Value,
) -> Result<(), VmError> {
    let Some(handler) = handlers.pop() else {
        return Err(VmError::Typed(error));
    };
    frames.truncate(handler.frame_index + 1);
    let Some(frame) = frames.get_mut(handler.frame_index) else {
        return Err(VmError::internal(
            "attempt handler refers to a missing frame",
        ));
    };
    frame.registers[handler.destination.0 as usize] = Slot::Value(error);
    frame.instruction_pointer = handler.catch_target;
    Ok(())
}

fn primitive_index(primitive: Primitive) -> usize {
    primitive as usize
}

const PRIMITIVES: [(Primitive, &str); 9] = [
    (Primitive::Add, "+"),
    (Primitive::Subtract, "-"),
    (Primitive::Multiply, "*"),
    (Primitive::Divide, "/"),
    (Primitive::IntegerDivide, "div"),
    (Primitive::Modulo, "mod"),
    (Primitive::LessThan, "<"),
    (Primitive::GreaterThan, ">"),
    (Primitive::Equal, "="),
];
