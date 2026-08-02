use crate::{
    diagnostic::Diagnostic,
    native::{NativeContext, NativeRegistry},
    runtime::{NativeFunctionId, SymbolId, Value, VmCell, VmClosure, VmPartial, VmValue as Slot},
};

use super::ir::{Instruction, Primitive, Program};
use super::native_context::VmNativeContext;

#[derive(Debug)]
pub struct Machine {
    natives: NativeRegistry,
    primitive_natives: Vec<NativeFunctionId>,
    context: VmNativeContext,
    programs: std::collections::HashMap<u64, Program>,
    module_exports: std::collections::HashMap<u64, Value>,
}

impl Default for Machine {
    fn default() -> Self {
        Self::with_context(VmNativeContext::default())
    }
}

impl Machine {
    fn with_context(context: VmNativeContext) -> Self {
        let mut natives = NativeRegistry::default();
        crate::native::register_core(&mut natives);
        crate::native::io::register(&mut natives);
        crate::native::tcp::register(&mut natives);
        crate::native::remote::register(&mut natives);
        let primitive_natives = PRIMITIVES
            .iter()
            .map(|(_, name)| {
                natives
                    .find_id(name)
                    .unwrap_or_else(|| panic!("native primitive `{name}` must be registered"))
            })
            .collect();
        Self {
            natives,
            primitive_natives,
            context,
            programs: std::collections::HashMap::new(),
            module_exports: std::collections::HashMap::new(),
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
    pub fn new(working_directory: std::path::PathBuf) -> Self {
        Self::with_context(VmNativeContext::new(working_directory))
    }

    pub(crate) fn with_concurrency(
        working_directory: std::path::PathBuf,
        concurrency: std::sync::Arc<crate::runtime::ConcurrencyHub>,
        network: std::sync::Arc<std::sync::Mutex<crate::native::host::NetworkResources>>,
    ) -> Self {
        Self::with_context(VmNativeContext::with_concurrency(
            working_directory,
            concurrency,
            network,
        ))
    }

    pub fn standard_globals(&mut self) -> std::collections::HashMap<std::sync::Arc<str>, Value> {
        let definitions = self
            .natives
            .iter_with_ids()
            .map(|(id, definition)| (id, definition.name))
            .collect::<Vec<_>>();
        let mut globals = std::collections::HashMap::new();
        let mut namespaces = std::collections::HashMap::<&str, Vec<_>>::new();
        for (id, name) in definitions {
            if let Some(namespace) = crate::native::core_namespace(name) {
                namespaces.entry(namespace).or_default().push((
                    std::sync::Arc::from(name),
                    true,
                    false,
                    Value::NativeFunction(id),
                ));
            } else if matches!(name, "+" | "-" | "*" | "/" | "<" | ">" | "=") {
                globals.insert(std::sync::Arc::from(name), Value::NativeFunction(id));
            }
        }
        for (name, bindings) in namespaces {
            let namespace = self.context.make_native_namespace(bindings);
            globals.insert(std::sync::Arc::from(name), namespace);
        }
        globals
    }

    pub fn resolve_symbol(&self, symbol: SymbolId) -> Option<&str> {
        self.context.resolve(symbol)
    }

    pub(crate) fn import_transport(&mut self, value: crate::runtime::TransportValue) -> Value {
        let symbols = &mut self.context.symbols;
        value.into_value(|name| symbols.intern(name))
    }

    pub(crate) fn export_transport(
        &self,
        value: &Value,
    ) -> Result<crate::runtime::TransportValue, &'static str> {
        crate::runtime::TransportValue::from_value(value, |symbol| {
            self.context.resolve(symbol).map(std::sync::Arc::from)
        })
    }

    pub fn exported_globals(
        &self,
        program: &Program,
    ) -> std::collections::HashMap<std::sync::Arc<str>, Value> {
        let Some(Value::Namespace(namespace)) = self.module_exports.get(&program.id) else {
            return std::collections::HashMap::new();
        };
        namespace
            .environment
            .borrow()
            .bindings
            .iter()
            .filter_map(|(symbol, binding)| {
                Some((
                    std::sync::Arc::from(self.resolve_symbol(*symbol)?),
                    binding.value.clone(),
                ))
            })
            .collect()
    }

    pub fn exported_namespace(&self, program: &Program) -> Option<Value> {
        self.module_exports.get(&program.id).cloned()
    }

    pub fn namespace_globals(
        &self,
        value: &Value,
    ) -> std::collections::HashMap<std::sync::Arc<str>, Value> {
        let Value::Namespace(namespace) = value.resolved() else {
            return std::collections::HashMap::new();
        };
        namespace
            .environment
            .borrow()
            .bindings
            .iter()
            .filter_map(|(symbol, binding)| {
                (binding.visibility == crate::runtime::BindingVisibility::Public).then(|| {
                    Some((
                        std::sync::Arc::from(self.resolve_symbol(*symbol)?),
                        binding.value.clone(),
                    ))
                })?
            })
            .collect()
    }

    pub(crate) fn native_module(&mut self, exports: &[(&str, &str)]) -> Result<Value, Diagnostic> {
        let mut bindings = Vec::new();
        for &(public_name, native_name) in exports {
            let native = self.natives.find_id(native_name).ok_or_else(|| {
                Diagnostic::error(format!("native function `{native_name}` is not registered"))
            })?;
            bindings.push((
                std::sync::Arc::from(public_name),
                true,
                false,
                Value::NativeFunction(native),
            ));
        }
        Ok(self.context.make_native_namespace(bindings))
    }

    pub fn diagnostic(&self, error: VmError) -> Diagnostic {
        match error {
            VmError::Internal(diagnostic) => diagnostic,
            VmError::Typed(value) => {
                let (message, metadata) = match value {
                    Value::Namespace(namespace) => {
                        let message = namespace
                            .environment
                            .borrow()
                            .bindings
                            .iter()
                            .find(|(symbol, _)| self.resolve_symbol(**symbol) == Some("message"))
                            .and_then(|(_, binding)| match &binding.value {
                                Value::String(message) => Some(message.to_string()),
                                _ => None,
                            })
                            .unwrap_or_else(|| "<error>".into());
                        (message, namespace.error_metadata.borrow().clone())
                    }
                    _ => ("<error>".into(), None),
                };
                Diagnostic {
                    severity: crate::diagnostic::Severity::Error,
                    message,
                    primary_span: metadata.as_ref().map(|metadata| metadata.origin),
                    stack: metadata.map(|metadata| metadata.stack).unwrap_or_default(),
                }
            }
        }
    }

    pub fn execute(&mut self, program: &Program) -> Result<Value, VmError> {
        self.programs
            .entry(program.id)
            .or_insert_with(|| program.clone());
        let mut frames = vec![Frame {
            program: program.id,
            function: None,
            instruction_pointer: 0,
            registers: initialize_registers(
                program.register_count,
                &program.binding_registers,
                &program.initial_bindings,
            ),
            return_destination: None,
            call_span: None,
        }];
        let mut handlers = Vec::new();
        'dispatch: loop {
            let frame_index = frames
                .len()
                .checked_sub(1)
                .expect("VM must retain an entry frame until return");
            let program = self
                .programs
                .get(&frames[frame_index].program)
                .ok_or_else(|| VmError::internal("VM frame refers to an unloaded program"))?;
            let instructions = match frames[frame_index].function {
                Some(function) => &program.functions[function as usize].instructions,
                None => &program.instructions,
            };
            let Some(instruction) = instructions.get(frames[frame_index].instruction_pointer)
            else {
                return Err(VmError::internal("register VM program did not return"));
            };
            let instruction_index = frames[frame_index].instruction_pointer;
            let active_span = match frames[frame_index].function {
                Some(function) => program.functions[function as usize]
                    .instruction_spans
                    .get(instruction_index)
                    .copied()
                    .flatten(),
                None => program
                    .instruction_spans
                    .get(instruction_index)
                    .copied()
                    .flatten(),
            };
            let stack = frames
                .iter()
                .filter_map(|frame| {
                    let function = frame.function?;
                    let owner = self.programs.get(&frame.program)?;
                    Some(crate::diagnostic::StackFrame {
                        name: owner.functions[function as usize].name.to_string(),
                        span: frame.call_span?,
                    })
                })
                .rev()
                .collect();
            self.context.set_execution_metadata(active_span, stack);
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
                Instruction::MakeBlock {
                    destination,
                    block,
                    function,
                    context,
                } => {
                    frame.registers[destination.0 as usize] = Slot::Value(Value::Block(
                        dumpster::unsync::Gc::new(crate::runtime::StoredBlock {
                            module_index: program.module_index,
                            block_id: crate::syntax::ast::BlockId(*block),
                            vm_program: program.id,
                            vm_function: *function,
                            vm_context: context.clone(),
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
                        if let Some(fallback) = &cell.fallback {
                            frame.registers[destination.0 as usize] = fallback.clone();
                        } else {
                            let error = self.context.typed_error(
                                &["error", "name_error"],
                                format!("unbound identifier `{name}`"),
                            );
                            catch_typed_error(&mut frames, &mut handlers, error)?;
                            continue 'dispatch;
                        }
                    }
                    Slot::Cell(cell) => {
                        frame.registers[destination.0 as usize] =
                            if cell.contains_reference_like_value() {
                                Slot::Value(Value::VmBinding(cell))
                            } else {
                                cell.value.borrow().clone()
                            };
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
                            let (types, message) = if cell.fallback.is_some() {
                                (
                                    &["error", "mutation_error"][..],
                                    format!("cannot assign to immutable binding `{name}`"),
                                )
                            } else {
                                (
                                    &["error", "name_error"][..],
                                    format!("unbound identifier `{name}` for assignment"),
                                )
                            };
                            let error = self.context.typed_error(types, message);
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
                Instruction::CheckWritable { binding, name } => {
                    let error = match frame.registers[binding.0 as usize].clone() {
                        Slot::Cell(cell) if cell.mutable.get() == Some(true) => None,
                        Slot::Cell(cell)
                            if cell.mutable.get().is_none() && cell.fallback.is_none() =>
                        {
                            Some(self.context.typed_error(
                                &["error", "name_error"],
                                format!("unbound identifier `{name}` for assignment"),
                            ))
                        }
                        Slot::Cell(_) => Some(self.context.typed_error(
                            &["error", "mutation_error"],
                            format!("cannot assign to immutable binding `{name}`"),
                        )),
                        _ => {
                            return Err(VmError::internal(
                                "CHECK_WRITABLE requires a binding cell",
                            ));
                        }
                    };
                    if let Some(error) = error {
                        catch_typed_error(&mut frames, &mut handlers, error)?;
                        continue 'dispatch;
                    }
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
                Instruction::MergeNamespaceTypes {
                    destination,
                    sources,
                } => {
                    let mut merged = Vec::new();
                    let mut merged_symbols = std::collections::HashSet::new();
                    for source in sources {
                        let Value::List(values) =
                            language_value(&frame.registers[source.0 as usize])?
                        else {
                            let error = self.context.typed_error(
                                &["error", "type_error"],
                                "object `types` must be a list".to_owned(),
                            );
                            catch_typed_error(&mut frames, &mut handlers, error)?;
                            continue 'dispatch;
                        };
                        let mut contribution = std::collections::HashSet::new();
                        for value in values.iter() {
                            let Value::Symbol(symbol) = value else {
                                let error = self.context.typed_error(
                                    &["error", "type_error"],
                                    "object `types` must contain only symbols".to_owned(),
                                );
                                catch_typed_error(&mut frames, &mut handlers, error)?;
                                continue 'dispatch;
                            };
                            if !contribution.insert(*symbol) {
                                let error = self.context.typed_error(
                                    &["error", "type_error"],
                                    "object `types` cannot contain duplicates".to_owned(),
                                );
                                catch_typed_error(&mut frames, &mut handlers, error)?;
                                continue 'dispatch;
                            }
                            if merged_symbols.insert(*symbol) {
                                merged.push(value.clone());
                            }
                        }
                    }
                    frame.registers[destination.0 as usize] =
                        Slot::Value(Value::List(merged.into_iter().collect()));
                }
                Instruction::MakeNamespace {
                    destination,
                    bindings,
                    self_binding,
                } => {
                    let bindings = bindings
                        .iter()
                        .map(|binding| {
                            Ok((
                                binding.name.clone(),
                                binding.public,
                                binding.mutable,
                                linked_value(&frame.registers[binding.source.0 as usize])?,
                            ))
                        })
                        .collect::<Result<Vec<_>, Diagnostic>>()?;
                    match self.context.make_namespace(bindings) {
                        Ok(namespace) => {
                            if let Some(binding) = self_binding {
                                let Slot::Cell(cell) = frame.registers[binding.0 as usize].clone()
                                else {
                                    return Err(VmError::internal(
                                        "object self binding is not a cell",
                                    ));
                                };
                                *cell.value.borrow_mut() = Slot::Value(namespace.clone());
                                cell.mutable.set(Some(false));
                            }
                            frame.registers[destination.0 as usize] = Slot::Value(namespace);
                        }
                        Err(error) => {
                            catch_typed_error(&mut frames, &mut handlers, error)?;
                            continue 'dispatch;
                        }
                    }
                }
                Instruction::MakeRemoteNamespace {
                    destination,
                    blueprint,
                    context,
                } => {
                    let transported_context = context
                        .iter()
                        .map(|binding| {
                            Ok((
                                binding.name.clone(),
                                binding.mode,
                                language_value(&frame.registers[binding.source.0 as usize])?,
                            ))
                        })
                        .collect::<Result<Vec<_>, Diagnostic>>()?;
                    match self
                        .context
                        .make_remote_namespace(blueprint.clone(), transported_context)
                    {
                        Ok(namespace) => {
                            for binding in context {
                                let Some(target) = binding.move_target else {
                                    continue;
                                };
                                let moved = Slot::Value(
                                    self.context.moved_value_error(binding.name.as_ref()),
                                );
                                match frame.registers[target.0 as usize].clone() {
                                    Slot::Cell(cell) => replace_referenced_location(&cell, moved),
                                    _ => frame.registers[target.0 as usize] = moved,
                                }
                            }
                            frame.registers[destination.0 as usize] = Slot::Value(namespace);
                        }
                        Err(error) => {
                            catch_typed_error(&mut frames, &mut handlers, error)?;
                            continue 'dispatch;
                        }
                    }
                }
                Instruction::AwaitTask { destination, task } => {
                    let task = language_value(&frame.registers[task.0 as usize])?;
                    let Value::Task(handle) = task else {
                        let error = self.context.typed_error(
                            &["error", "type_error"],
                            format!("`await` requires a future, got {}", task.type_symbol()),
                        );
                        catch_typed_error(&mut frames, &mut handlers, error)?;
                        continue 'dispatch;
                    };
                    match self.context.task_await(handle) {
                        Ok(value) => frame.registers[destination.0 as usize] = Slot::Value(value),
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
                    allow_private,
                } => {
                    let owner = reference_location(&frame.registers[namespace.0 as usize]);
                    let namespace = language_value(&frame.registers[namespace.0 as usize])?;
                    match self.context.load_member(namespace, name, *allow_private) {
                        Ok(Value::RemoteFunction(handle, member)) if owner.is_some() => {
                            frame.registers[destination.0 as usize] = Slot::Value(
                                Value::BoundRemoteFunction(owner.unwrap(), handle, member),
                            );
                        }
                        Ok(value) => {
                            frame.registers[destination.0 as usize] = Slot::Value(value);
                        }
                        Err(error) => {
                            catch_typed_error(&mut frames, &mut handlers, error)?;
                            continue 'dispatch;
                        }
                    }
                }
                Instruction::StoreMember {
                    namespace,
                    source,
                    name,
                    allow_private,
                } => {
                    let namespace = language_value(&frame.registers[namespace.0 as usize])?;
                    let source = language_value(&frame.registers[source.0 as usize])?;
                    if let Err(error) =
                        self.context
                            .store_member(namespace, name, source, *allow_private)
                    {
                        catch_typed_error(&mut frames, &mut handlers, error)?;
                        continue 'dispatch;
                    }
                }
                Instruction::CheckMemberWritable {
                    namespace,
                    name,
                    allow_private,
                } => {
                    let namespace = language_value(&frame.registers[namespace.0 as usize])?;
                    if let Err(error) =
                        self.context
                            .check_member_writable(namespace, name, *allow_private)
                    {
                        catch_typed_error(&mut frames, &mut handlers, error)?;
                        continue 'dispatch;
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
                Instruction::JumpIfNotBlock {
                    source,
                    module,
                    block,
                    target,
                } => match &frame.registers[source.0 as usize] {
                    Slot::Value(Value::Block(value))
                        if value.module_index == *module && value.block_id.0 == *block => {}
                    _ => frame.instruction_pointer = *target,
                },
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
                                language_value(&frame.registers[register.0 as usize])?;
                        }
                        &inline_arguments[..arguments.len()]
                    } else {
                        heap_arguments = arguments
                            .iter()
                            .map(|register| language_value(&frame.registers[register.0 as usize]))
                            .collect::<Result<Vec<_>, _>>()?;
                        heap_arguments.as_slice()
                    };
                    let definition = self
                        .natives
                        .get(self.primitive_natives[primitive_index(*primitive)])
                        .expect("cached primitive id must remain registered");
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
                            program: program.id,
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
                    command,
                } => {
                    let mut callee = language_value(&frame.registers[callee.0 as usize])?;
                    let mut argument = language_value(&frame.registers[argument.0 as usize])?;
                    if *command
                        && !matches!(
                            callee,
                            Value::NativeFunction(_)
                                | Value::VmClosure(_)
                                | Value::VmPartial(_)
                                | Value::RemoteFunction(_, _)
                                | Value::BoundRemoteFunction(_, _, _)
                                | Value::TaskFunction(_, _)
                        )
                    {
                        frame.registers[destination.0 as usize] = Slot::Value(callee);
                        continue 'dispatch;
                    }
                    let contains_placeholder = matches!(argument, Value::Placeholder)
                        || matches!(&argument, Value::List(values) if values.iter().any(|value| matches!(value, Value::Placeholder)));
                    if contains_placeholder {
                        let Value::VmClosure(closure) = callee else {
                            let error = self.context.typed_error(
                                &["error", "type_error"],
                                "partial application requires a user function".into(),
                            );
                            catch_typed_error(&mut frames, &mut handlers, error)?;
                            continue 'dispatch;
                        };
                        let Some(owner) = self.programs.get(&closure.program) else {
                            return Err(VmError::internal(
                                "partial function's module is not loaded",
                            ));
                        };
                        let Some(parameter_count) =
                            owner.functions[closure.function as usize].parameter_count
                        else {
                            let error = self.context.typed_error(
                                &["error", "type_error"],
                                "partial application requires a list parameter pattern".into(),
                            );
                            catch_typed_error(&mut frames, &mut handlers, error)?;
                            continue 'dispatch;
                        };
                        let Value::List(values) = argument else {
                            let error = self.context.typed_error(
                                &["error", "type_error"],
                                "partial application placeholders must appear in the call argument list".into(),
                            );
                            catch_typed_error(&mut frames, &mut handlers, error)?;
                            continue 'dispatch;
                        };
                        if values.len() != parameter_count as usize {
                            let error = self.context.typed_error(
                                &["error", "match_error"],
                                "partial argument list does not match the function parameter pattern".into(),
                            );
                            catch_typed_error(&mut frames, &mut handlers, error)?;
                            continue 'dispatch;
                        }
                        frame.registers[destination.0 as usize] =
                            Slot::Value(Value::VmPartial(dumpster::unsync::Gc::new(VmPartial {
                                closure,
                                arguments: values
                                    .iter()
                                    .map(|value| {
                                        (!matches!(value, Value::Placeholder))
                                            .then(|| value.clone())
                                    })
                                    .collect(),
                            })));
                        continue 'dispatch;
                    }
                    if let Value::VmPartial(partial) = callee {
                        let supplied = match argument {
                            Value::List(values) => values.to_vec(),
                            value => vec![value],
                        };
                        let remaining = partial
                            .arguments
                            .iter()
                            .filter(|value| value.is_none())
                            .count();
                        if supplied.len() != remaining {
                            let error = self.context.typed_error(
                                &["error", "match_error"],
                                "function argument does not match its parameter pattern".into(),
                            );
                            catch_typed_error(&mut frames, &mut handlers, error)?;
                            continue 'dispatch;
                        }
                        let mut supplied = supplied.into_iter();
                        argument = Value::List(
                            partial
                                .arguments
                                .iter()
                                .map(|value| {
                                    value.clone().unwrap_or_else(|| supplied.next().unwrap())
                                })
                                .collect(),
                        );
                        callee = Value::VmClosure(partial.closure.clone());
                    }
                    if let Value::NativeFunction(native) = callee {
                        let arguments = match argument {
                            Value::List(values) => values.to_vec(),
                            value => vec![value],
                        };
                        let definition = self
                            .natives
                            .get(native)
                            .ok_or_else(|| VmError::internal("invalid native function id"))?;
                        if !definition.arity.check(arguments.len()) {
                            let error = self.context.typed_error(
                                &["error", "arity_error"],
                                format!(
                                    "native function `{}` argument list has the wrong length",
                                    definition.name
                                ),
                            );
                            catch_typed_error(&mut frames, &mut handlers, error)?;
                            continue 'dispatch;
                        }
                        match (definition.call)(&mut self.context, &arguments) {
                            Ok(value) => {
                                frame.registers[destination.0 as usize] = Slot::Value(value)
                            }
                            Err(error) => {
                                catch_typed_error(&mut frames, &mut handlers, error)?;
                                continue 'dispatch;
                            }
                        }
                        continue 'dispatch;
                    }
                    if let Value::RemoteFunction(handle, member) = callee {
                        match self
                            .context
                            .call_remote_function(handle, &member, &argument)
                        {
                            Ok(value) => {
                                frame.registers[destination.0 as usize] = Slot::Value(value)
                            }
                            Err(error) => {
                                catch_typed_error(&mut frames, &mut handlers, error)?;
                                continue 'dispatch;
                            }
                        }
                        continue 'dispatch;
                    }
                    if let Value::BoundRemoteFunction(_, handle, member) = callee {
                        match self
                            .context
                            .call_remote_function(handle, &member, &argument)
                        {
                            Ok(value) => {
                                frame.registers[destination.0 as usize] = Slot::Value(value)
                            }
                            Err(error) => {
                                catch_typed_error(&mut frames, &mut handlers, error)?;
                                continue 'dispatch;
                            }
                        }
                        continue 'dispatch;
                    }
                    if let Value::TaskFunction(handle, member) = callee {
                        let Value::List(arguments) = argument else {
                            let error = self.context.typed_error(
                                &["error", "arity_error"],
                                format!("task function `{member}` expects an empty argument list"),
                            );
                            catch_typed_error(&mut frames, &mut handlers, error)?;
                            continue 'dispatch;
                        };
                        if !arguments.is_empty() {
                            let error = self.context.typed_error(
                                &["error", "arity_error"],
                                format!("task function `{member}` expects an empty argument list"),
                            );
                            catch_typed_error(&mut frames, &mut handlers, error)?;
                            continue 'dispatch;
                        }
                        let result = match member.as_ref() {
                            "complete?" => self
                                .context
                                .task_complete(handle)
                                .map(Value::Boolean)
                                .map_err(|message| {
                                    self.context.typed_error(&["error", "task_error"], message)
                                }),
                            _ => unreachable!("task member names are validated during lookup"),
                        };
                        match result {
                            Ok(value) => {
                                frame.registers[destination.0 as usize] = Slot::Value(value)
                            }
                            Err(error) => {
                                catch_typed_error(&mut frames, &mut handlers, error)?;
                                continue 'dispatch;
                            }
                        }
                        continue 'dispatch;
                    }
                    let Value::VmClosure(closure) = callee else {
                        let error = self.context.typed_error(
                            &["error", "type_error"],
                            format!("cannot call value of type {}", callee.type_symbol()),
                        );
                        catch_typed_error(&mut frames, &mut handlers, error)?;
                        continue 'dispatch;
                    };
                    let function = closure.function;
                    let captures = closure.captures.clone();
                    let Some(owner) = self.programs.get(&closure.program) else {
                        let error = self.context.typed_error(
                            &["error", "link_error"],
                            "function's compiled module is not loaded in this VM".into(),
                        );
                        catch_typed_error(&mut frames, &mut handlers, error)?;
                        continue 'dispatch;
                    };
                    let compiled = &owner.functions[function as usize];
                    if captures.len() != compiled.capture_count as usize {
                        return Err(VmError::internal("VM closure capture count mismatch"));
                    }
                    let mut registers = initialize_registers(
                        compiled.register_count,
                        &compiled.binding_registers,
                        &[],
                    );
                    registers[0] = Slot::Value(argument);
                    for (index, capture) in captures.into_iter().enumerate() {
                        registers[index + 1] = capture;
                    }
                    frames.push(Frame {
                        program: closure.program,
                        function: Some(function),
                        instruction_pointer: 0,
                        registers,
                        return_destination: Some(*destination),
                        call_span: active_span,
                    });
                }
                Instruction::DoDynamic {
                    destination,
                    block,
                    context,
                } => {
                    let value = language_value(&frame.registers[block.0 as usize])?;
                    let Value::Block(block) = value else {
                        let error = self.context.typed_error(
                            &["error", "type_error"],
                            "do requires a block value".into(),
                        );
                        catch_typed_error(&mut frames, &mut handlers, error)?;
                        continue 'dispatch;
                    };
                    let owner_id = block.vm_program;
                    let function = block.vm_function;
                    let Some(owner) = self.programs.get(&owner_id) else {
                        return Err(VmError::internal("block's compiled module is not loaded"));
                    };
                    let compiled = &owner.functions[function as usize];
                    let available = context
                        .iter()
                        .map(|(name, register)| {
                            (name.as_ref(), frame.registers[register.0 as usize].clone())
                        })
                        .collect::<std::collections::HashMap<_, _>>();
                    let mut captures = Vec::with_capacity(block.vm_context.len());
                    let mut missing = None;
                    for name in &block.vm_context {
                        let Some(value) = available.get(name.as_ref()).cloned() else {
                            missing = Some(name.clone());
                            break;
                        };
                        captures.push(context_cell(value));
                    }
                    if let Some(name) = missing {
                        let error = self.context.typed_error(
                            &["error", "name_error", "missing_context"],
                            format!("cannot execute block: required context binding `{name}` is unavailable"),
                        );
                        catch_typed_error(&mut frames, &mut handlers, error)?;
                        continue 'dispatch;
                    }
                    let mut registers = initialize_registers(
                        compiled.register_count,
                        &compiled.binding_registers,
                        &[],
                    );
                    registers[0] = Slot::Value(Value::Unit);
                    for (index, capture) in captures.into_iter().enumerate() {
                        registers[index + 1] = capture;
                    }
                    frames.push(Frame {
                        program: owner_id,
                        function: Some(function),
                        instruction_pointer: 0,
                        registers,
                        return_destination: Some(*destination),
                        call_span: active_span,
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
                Instruction::PublishExports { bindings } => {
                    let bindings = bindings
                        .iter()
                        .map(|binding| {
                            Ok((
                                binding.name.clone(),
                                binding.public,
                                binding.mutable,
                                linked_value(&frame.registers[binding.source.0 as usize])?,
                            ))
                        })
                        .collect::<Result<Vec<_>, Diagnostic>>()?;
                    let namespace = self.context.make_native_namespace(bindings);
                    self.module_exports.insert(program.id, namespace);
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
    match value {
        Slot::Value(value) => Ok(value.resolved()),
        Slot::Uninitialized | Slot::Cell(_) => Err(Diagnostic::error(format!(
            "internal VM storage cannot cross the language-value boundary: {value:?}"
        ))),
    }
}

fn linked_value(value: &Slot) -> Result<Value, Diagnostic> {
    match value {
        Slot::Cell(cell) => Ok(Value::VmBinding(cell.clone())),
        value => language_value(value),
    }
}

fn replace_referenced_location(cell: &dumpster::unsync::Gc<VmCell>, replacement: Slot) {
    let linked = match &*cell.value.borrow() {
        Slot::Value(Value::VmBinding(linked)) | Slot::Cell(linked) => Some(linked.clone()),
        _ => None,
    };
    if let Some(linked) = linked {
        replace_referenced_location(&linked, replacement);
    } else {
        *cell.value.borrow_mut() = replacement;
    }
}

fn reference_location(value: &Slot) -> Option<dumpster::unsync::Gc<VmCell>> {
    match value {
        Slot::Value(Value::VmBinding(cell)) | Slot::Cell(cell) => Some(cell.clone()),
        _ => None,
    }
}

struct Frame {
    program: u64,
    function: Option<u16>,
    instruction_pointer: usize,
    registers: Vec<Slot>,
    return_destination: Option<super::ir::Register>,
    call_span: Option<crate::source::Span>,
}

fn initialize_registers(
    register_count: u16,
    binding_registers: &[super::ir::Register],
    initial_bindings: &[(super::ir::Register, Value)],
) -> Vec<Slot> {
    let mut registers = vec![Slot::Uninitialized; register_count as usize];
    let initial_bindings = initial_bindings
        .iter()
        .cloned()
        .collect::<std::collections::HashMap<_, _>>();
    for register in binding_registers {
        let fallback = initial_bindings.get(register).cloned().map(Slot::Value);
        registers[register.0 as usize] =
            Slot::Cell(dumpster::unsync::Gc::new(VmCell::binding(fallback)));
    }
    registers
}

fn context_cell(value: Slot) -> Slot {
    if matches!(value, Slot::Cell(_)) {
        return value;
    }
    let cell = dumpster::unsync::Gc::new(VmCell::binding(None));
    *cell.value.borrow_mut() = value;
    cell.mutable.set(Some(false));
    Slot::Cell(cell)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vm::ir::{Function, Register};

    #[test]
    fn calls_closures_owned_by_another_loaded_program() {
        let owner = Program {
            id: 1,
            constants: vec![Value::Integer(42)],
            instructions: vec![
                Instruction::MakeClosure {
                    destination: Register(0),
                    function: 0,
                    captures: Vec::new(),
                },
                Instruction::Return {
                    source: Register(0),
                },
            ],
            instruction_spans: vec![None; 2],
            register_count: 1,
            functions: vec![Function {
                name: std::sync::Arc::from("answer"),
                instructions: vec![
                    Instruction::LoadConstant {
                        destination: Register(1),
                        constant: 0,
                    },
                    Instruction::Return {
                        source: Register(1),
                    },
                ],
                instruction_spans: vec![None; 2],
                register_count: 2,
                capture_count: 0,
                parameter_count: Some(0),
                binding_registers: Vec::new(),
            }],
            binding_registers: Vec::new(),
            initial_bindings: Vec::new(),
            module_index: 0,
        };
        let mut machine = Machine::default();
        let closure = machine.execute(&owner).unwrap();
        let caller = Program {
            id: 2,
            constants: vec![closure, Value::Unit],
            instructions: vec![
                Instruction::LoadConstant {
                    destination: Register(0),
                    constant: 0,
                },
                Instruction::LoadConstant {
                    destination: Register(1),
                    constant: 1,
                },
                Instruction::CallDynamic {
                    destination: Register(2),
                    callee: Register(0),
                    argument: Register(1),
                    command: false,
                },
                Instruction::Return {
                    source: Register(2),
                },
            ],
            instruction_spans: vec![None; 4],
            register_count: 3,
            functions: Vec::new(),
            binding_registers: Vec::new(),
            initial_bindings: Vec::new(),
            module_index: 0,
        };
        assert_eq!(machine.execute(&caller).unwrap(), Value::Integer(42));
    }
}
