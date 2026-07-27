use crate::{native::NativeContext, runtime::Value};

use super::Interpreter;

#[derive(Clone, Debug)]
pub enum Signal {
    Return(Value),
    Break(Value),
    Continue,
    Throw(Value),
}

pub type EvalResult = Result<Value, Signal>;

fn typed_err(context: &mut CallContext, types: &[&str], message: String) -> Value {
    <CallContext as NativeContext>::typed_error(context, types, message)
}

/// Evaluate a single AST node and return its resulting value.
pub fn evaluate_node(context: &mut CallContext, node_id: crate::syntax::ast::NodeId) -> EvalResult {
    use crate::syntax::ast::NodeKind;

    // Evaluation mutates interpreter arenas, so no AST borrow may cross the
    // recursive call below.
    let (kind, span) = {
        let module = &context.interpreter.parsed_modules[context.interpreter.current_module];
        let node = module.node(node_id);
        (node.kind.clone(), node.span)
    };

    let previous_span = context.interpreter.active_span.replace(span);
    let result = match kind {
        NodeKind::Unit => Ok(Value::Unit),
        NodeKind::Boolean(b) => Ok(Value::Boolean(b)),
        NodeKind::Integer(n) => Ok(Value::Integer(n)),
        NodeKind::Float(f) => Ok(Value::Float(f)),
        NodeKind::String(s) => Ok(Value::String(s)),
        NodeKind::Symbol(s) => {
            let id = context.interpreter.symbols.intern(&s);
            Ok(Value::Symbol(id))
        }
        NodeKind::Identifier(name) => resolve_identifier(context, &name),
        NodeKind::Placeholder => Err(Signal::Throw(typed_err(
            context,
            &["error", "syntax_error"],
            "placeholder `_` used outside of partial application".to_string(),
        ))),
        NodeKind::List(elements) => evaluate_list(context, &elements),
        NodeKind::Block(block_id) => {
            let value_block_id = context
                .interpreter
                .store_block(block_id, context.interpreter.current_module);
            Ok(Value::Block(crate::runtime::BlockId(value_block_id as u32)))
        }
        NodeKind::Member { object, member } => evaluate_member(context, object, &member),
        NodeKind::Call {
            callee,
            arguments,
            immediate: _,
        } => evaluate_call(context, callee, &arguments),
        NodeKind::Binding {
            visibility,
            mutability,
            name,
            value,
        } => evaluate_binding(context, visibility, mutability, &name, value),
        NodeKind::Assignment { name, value } => evaluate_assignment(context, &name, value),
        NodeKind::Function {
            visibility,
            name,
            parameters,
            body,
        } => evaluate_function(context, visibility, &name, &parameters, body),
        NodeKind::Conditional {
            condition,
            consequent,
            alternative,
        } => evaluate_conditional(context, condition, consequent, alternative),
        NodeKind::Loop {
            kind,
            condition,
            body,
        } => evaluate_loop(context, kind, condition, body),
        NodeKind::Return(value) => evaluate_return(context, value),
        NodeKind::Break(value) => evaluate_break(context, value),
        NodeKind::Continue => Err(Signal::Continue),
        NodeKind::Throw(value) => evaluate_throw(context, value),
        NodeKind::Import { path, alias } => evaluate_import(context, &path, alias.as_deref()),
        NodeKind::New(operand) => evaluate_new(context, operand),
        NodeKind::Eval(operand) => evaluate_eval(context, operand),
        NodeKind::Attempt(block_id) => evaluate_attempt(context, block_id),
    };
    context.interpreter.active_span = previous_span;
    result
}

/// Evaluate a list of statements, returning the last value.
pub fn evaluate_statement_list(
    context: &mut CallContext,
    statement_ids: &[crate::syntax::ast::NodeId],
) -> EvalResult {
    let mut result = Value::Unit;
    for &node_id in statement_ids {
        result = evaluate_node(context, node_id)?;
    }
    Ok(result)
}

/// Evaluate a block (from a BlockId) in the current environment.
pub fn evaluate_block(
    context: &mut CallContext,
    block_id: crate::syntax::ast::BlockId,
) -> EvalResult {
    let module_index = context.interpreter.current_module;
    let statements = context.interpreter.parsed_modules[module_index]
        .block(block_id)
        .statements
        .clone();
    evaluate_statement_list(context, &statements)
}

// ── Identifier Resolution ──

fn resolve_identifier(context: &mut CallContext, name: &str) -> EvalResult {
    let symbol = context.interpreter.symbols.intern(name);
    let mut env_id = context.interpreter.current_environment;

    loop {
        let env = &context.interpreter.environments[env_id.0 as usize];
        if let Some(binding) = env.bindings.get(&symbol) {
            if let crate::runtime::BindingMutability::ImportedReadOnly {
                environment,
                symbol,
            } = binding.mutability
            {
                return Ok(context.interpreter.environments[environment.0 as usize]
                    .bindings
                    .get(&symbol)
                    .expect("import source binding must exist")
                    .value
                    .clone());
            }
            return Ok(binding.value.clone());
        }
        match env.parent {
            Some(parent) => env_id = parent,
            None => break,
        }
    }

    Err(Signal::Throw(typed_err(
        context,
        &["error", "name_error"],
        format!("unbound identifier `{name}`"),
    )))
}

// ── Lists ──

fn evaluate_list(context: &mut CallContext, elements: &[crate::syntax::ast::NodeId]) -> EvalResult {
    use crate::runtime::PersistentList;
    let mut values = Vec::with_capacity(elements.len());
    for &element in elements {
        values.push(evaluate_node(context, element)?);
    }
    Ok(Value::List(values.into_iter().collect::<PersistentList>()))
}

// ── Member Access ──

fn evaluate_member(
    context: &mut CallContext,
    object_id: crate::syntax::ast::NodeId,
    member: &str,
) -> EvalResult {
    let object = evaluate_node(context, object_id)?;
    match object {
        Value::Namespace(ns_id) => {
            // Extract what we need before mutating
            let ns_data = {
                let ns = &context.interpreter.namespaces[ns_id.0 as usize];
                let symbol = context.interpreter.symbols.intern(member);
                let env_id = ns.environment;
                let env = &context.interpreter.environments[env_id.0 as usize];
                let has_binding = env.bindings.contains_key(&symbol);
                if !has_binding {
                    return Err(Signal::Throw(typed_err(
                        context,
                        &["error", "name_error"],
                        format!("namespace has no member `{member}`"),
                    )));
                }
                let ns2 = &context.interpreter.namespaces[ns_id.0 as usize];
                let env_id2 = ns2.environment;
                let env2 = &context.interpreter.environments[env_id2.0 as usize];
                let Some(binding) = env2.bindings.get(&symbol) else {
                    return Err(Signal::Throw(typed_err(
                        context,
                        &["error", "internal_error"],
                        format!("namespace member `{member}` disappeared during lookup"),
                    )));
                };
                if binding.visibility == crate::runtime::BindingVisibility::Private {
                    return Err(Signal::Throw(typed_err(
                        context,
                        &["error", "visibility_error"],
                        format!("member `{member}` is private"),
                    )));
                }
                let value = binding.value.clone();
                let capture_env = ns2.environment;
                (value, capture_env)
            };

            let (value, _capture_env) = ns_data;
            Ok(value)
        }
        _ => Err(Signal::Throw(typed_err(
            context,
            &["error", "type_error"],
            format!(
                "member access `.` requires a namespace, got {}",
                value_type_name(&object)
            ),
        ))),
    }
}

// ── Calls ──

fn evaluate_call(
    context: &mut CallContext,
    callee_id: crate::syntax::ast::NodeId,
    arguments: &[crate::syntax::ast::NodeId],
) -> EvalResult {
    let callee = evaluate_node(context, callee_id)?;

    // Check for partial application (arguments contain Placeholders)
    // We detect this by checking if any argument node is a Placeholder
    let module = &context.interpreter.parsed_modules[context.interpreter.current_module];
    let has_placeholder = arguments.iter().any(|&arg_id| {
        matches!(
            module.node(arg_id).kind,
            crate::syntax::ast::NodeKind::Placeholder
        )
    });

    if has_placeholder {
        return create_partial_application(context, callee, arguments);
    }

    // Evaluate all arguments
    let mut evaluated_args = Vec::with_capacity(arguments.len());
    for &arg_id in arguments {
        evaluated_args.push(evaluate_node(context, arg_id)?);
    }

    // Dispatch based on callee type
    match callee {
        Value::Function(func_id) => call_user_function(context, func_id, &evaluated_args),
        Value::NativeFunction(native_id) => {
            call_native_function(context, native_id, &evaluated_args)
        }
        other => Err(Signal::Throw(typed_err(
            context,
            &["error", "type_error"],
            format!("cannot call value of type {}", value_type_name(&other)),
        ))),
    }
}

fn create_partial_application(
    context: &mut CallContext,
    callee: Value,
    arguments: &[crate::syntax::ast::NodeId],
) -> EvalResult {
    // Only user functions can be partially applied
    let Value::Function(func_id) = callee else {
        return Err(Signal::Throw(typed_err(
            context,
            &["error", "type_error"],
            "partial application requires a function".to_string(),
        )));
    };

    // Clone func data before we need to mutate context
    let func_data = context.interpreter.functions[func_id.0 as usize].clone();
    let func_params: Vec<crate::runtime::SymbolId> = func_data.parameters.clone();

    if arguments.len() != func_params.len() {
        return Err(Signal::Throw(typed_err(
            context,
            &["error", "arity_error"],
            format!(
                "function expects {} arguments but got {}",
                func_params.len(),
                arguments.len()
            ),
        )));
    }

    let has_ph: Vec<bool> = {
        let module = &context.interpreter.parsed_modules[context.interpreter.current_module];
        arguments
            .iter()
            .map(|&arg_id| {
                matches!(
                    module.node(arg_id).kind,
                    crate::syntax::ast::NodeKind::Placeholder
                )
            })
            .collect()
    };

    let mut bound_params: Vec<crate::runtime::SymbolId> = Vec::new();
    let mut remaining_params: Vec<crate::runtime::SymbolId> = Vec::new();
    let mut bound_values: Vec<Value> = Vec::new();

    let mut param_iter = func_params.iter();
    let mut arg_iter = arguments.iter();
    let mut ph_iter = has_ph.iter();

    while let (Some(&param), Some(&arg_id), Some(&is_ph)) =
        (param_iter.next(), arg_iter.next(), ph_iter.next())
    {
        if is_ph {
            remaining_params.push(param);
        } else {
            let value = evaluate_node(context, arg_id)?;
            bound_params.push(param);
            bound_values.push(value);
        }
    }
    for &param in param_iter {
        remaining_params.push(param);
    }

    let mut new_func = crate::runtime::UserFunction {
        name: func_data.name,
        parameters: remaining_params,
        body: func_data.body,
        body_module: func_data.body_module,
        environment: func_data.environment,
        declaration_span: func_data.declaration_span,
    };

    // We need to pre-bind the bound params in the environment
    let parent_env = func_data.environment;
    let new_env_id = crate::runtime::EnvironmentId(context.interpreter.environments.len() as u32);
    {
        let mut new_env = crate::runtime::Environment::new(Some(parent_env));
        for (sym, val) in bound_params.into_iter().zip(bound_values) {
            new_env.bindings.insert(
                sym,
                crate::runtime::Binding {
                    value: val,
                    mutability: crate::runtime::BindingMutability::Immutable,
                    visibility: crate::runtime::BindingVisibility::Private,
                },
            );
        }
        context.interpreter.environments.push(new_env);
    }
    new_func.environment = new_env_id;

    let new_func_id = crate::runtime::FunctionId(context.interpreter.functions.len() as u32);
    context.interpreter.functions.push(new_func);

    Ok(Value::Function(new_func_id))
}

// ── User Function Call ──

fn call_user_function(
    context: &mut CallContext,
    func_id: crate::runtime::FunctionId,
    arguments: &[Value],
) -> EvalResult {
    let func = &context.interpreter.functions[func_id.0 as usize];

    if arguments.len() != func.parameters.len() {
        return Err(Signal::Throw(typed_err(
            context,
            &["error", "arity_error"],
            format!(
                "function expects {} arguments but got {}",
                func.parameters.len(),
                arguments.len()
            ),
        )));
    }

    let child_env_id = crate::runtime::EnvironmentId(context.interpreter.environments.len() as u32);
    {
        let mut new_env = crate::runtime::Environment::new(Some(func.environment));
        for (param, arg) in func.parameters.iter().zip(arguments) {
            new_env.bindings.insert(
                *param,
                crate::runtime::Binding {
                    value: arg.clone(),
                    mutability: crate::runtime::BindingMutability::Immutable,
                    visibility: crate::runtime::BindingVisibility::Private,
                },
            );
        }
        context.interpreter.environments.push(new_env);
    }

    let function_name = context
        .interpreter
        .symbols
        .resolve(func.name)
        .unwrap_or("<anonymous>")
        .to_owned();
    context.interpreter.call_stack.push(CallFrame {
        is_loop: false,
        function_name: Some(function_name),
        call_span: context.interpreter.active_span,
    });

    let prev_env = context.interpreter.current_environment;
    let prev_module = context.interpreter.current_module;
    context.interpreter.current_environment = child_env_id;
    context.interpreter.current_module = func.body_module;

    // Evaluate body
    let result = evaluate_block(context, func.body);

    // Restore environment and module
    context.interpreter.current_environment = prev_env;
    context.interpreter.current_module = prev_module;
    context.interpreter.call_stack.pop();

    // Handle signals
    match result {
        Ok(value) => Ok(value),
        Err(Signal::Return(value)) => Ok(value),
        Err(Signal::Break(_)) => Err(Signal::Throw(typed_err(
            context,
            &["error", "control_flow_error"],
            "`break` cannot cross function boundary".to_string(),
        ))),
        Err(Signal::Continue) => Err(Signal::Throw(typed_err(
            context,
            &["error", "control_flow_error"],
            "`continue` cannot cross function boundary".to_string(),
        ))),
        Err(Signal::Throw(value)) => Err(Signal::Throw(value)),
    }
}

// ── Native Function Call ──

fn call_native_function(
    context: &mut CallContext,
    native_id: crate::runtime::NativeFunctionId,
    arguments: &[Value],
) -> EvalResult {
    let definition = context
        .interpreter
        .natives
        .get(native_id)
        .expect("native function id not found");

    // Check arity
    if !definition.arity.check(arguments.len()) {
        return Err(Signal::Throw(typed_err(
            context,
            &["error", "arity_error"],
            format!(
                "native function `{}` called with wrong number of arguments",
                definition.name
            ),
        )));
    }

    let result = (definition.call)(context, arguments);
    match result {
        Ok(value) => Ok(value),
        Err(error_value) => Err(Signal::Throw(error_value)),
    }
}

// ── Bindings ──

fn evaluate_binding(
    context: &mut CallContext,
    visibility: crate::syntax::ast::Visibility,
    mutability: crate::syntax::ast::BindingKind,
    name: &str,
    value_id: crate::syntax::ast::NodeId,
) -> EvalResult {
    let value = evaluate_node(context, value_id)?;
    let symbol = context.interpreter.symbols.intern(name);

    let env = &context.interpreter.environments[context.interpreter.current_environment.0 as usize];
    if env.bindings.contains_key(&symbol) {
        return Err(Signal::Throw(typed_err(
            context,
            &["error", "name_error"],
            format!("duplicate binding `{name}` in current scope"),
        )));
    }

    let binding = crate::runtime::Binding {
        value,
        mutability: match mutability {
            crate::syntax::ast::BindingKind::Immutable => {
                crate::runtime::BindingMutability::Immutable
            }
            crate::syntax::ast::BindingKind::Mutable => crate::runtime::BindingMutability::Mutable,
        },
        visibility: match visibility {
            crate::syntax::ast::Visibility::Public => crate::runtime::BindingVisibility::Public,
            crate::syntax::ast::Visibility::Private => crate::runtime::BindingVisibility::Private,
        },
    };

    let env_ref =
        &mut context.interpreter.environments[context.interpreter.current_environment.0 as usize];
    env_ref.bindings.insert(symbol, binding);

    Ok(Value::Unit)
}

fn evaluate_assignment(
    context: &mut CallContext,
    name: &str,
    value_id: crate::syntax::ast::NodeId,
) -> EvalResult {
    let value = evaluate_node(context, value_id)?;
    let symbol = context.interpreter.symbols.intern(name);

    // Walk environment chain to find the nearest mutable binding
    let mut env_id = context.interpreter.current_environment;
    loop {
        let env = &mut context.interpreter.environments[env_id.0 as usize];
        if let Some(binding) = env.bindings.get_mut(&symbol) {
            match binding.mutability {
                crate::runtime::BindingMutability::Mutable => {
                    binding.value = value.clone();
                    return Ok(value);
                }
                crate::runtime::BindingMutability::Immutable => {
                    return Err(Signal::Throw(typed_err(
                        context,
                        &["error", "mutation_error"],
                        format!("cannot assign to immutable binding `{name}`"),
                    )));
                }
                crate::runtime::BindingMutability::ImportedReadOnly { .. } => {
                    return Err(Signal::Throw(typed_err(
                        context,
                        &["error", "mutation_error"],
                        format!("cannot assign to imported binding `{name}`"),
                    )));
                }
            }
        }
        match env.parent {
            Some(parent) => env_id = parent,
            None => break,
        }
    }

    Err(Signal::Throw(typed_err(
        context,
        &["error", "name_error"],
        format!("unbound identifier `{name}` for assignment"),
    )))
}

// ── Functions ──

fn evaluate_function(
    context: &mut CallContext,
    visibility: crate::syntax::ast::Visibility,
    name: &str,
    parameters: &[std::sync::Arc<str>],
    body: crate::syntax::ast::BlockId,
) -> EvalResult {
    let name_symbol = context.interpreter.symbols.intern(name);

    // Check for duplicate
    let env = &context.interpreter.environments[context.interpreter.current_environment.0 as usize];
    if env.bindings.contains_key(&name_symbol) {
        return Err(Signal::Throw(typed_err(
            context,
            &["error", "name_error"],
            format!("duplicate binding `{name}` in current scope"),
        )));
    }

    // The function captures the CURRENT environment (not a child)
    let func = crate::runtime::UserFunction {
        name: name_symbol,
        parameters: parameters
            .iter()
            .map(|s| context.interpreter.symbols.intern(s.as_ref()))
            .collect(),
        body,
        body_module: context.interpreter.current_module,
        environment: context.interpreter.current_environment,
        declaration_span: crate::source::Span::new(
            context
                .interpreter
                .sources
                .files()
                .first()
                .map(|f| f.id)
                .unwrap_or(crate::source::SourceId(0)),
            0,
            0,
        ),
    };

    let func_id = crate::runtime::FunctionId(context.interpreter.functions.len() as u32);
    context.interpreter.functions.push(func);

    let binding = crate::runtime::Binding {
        value: Value::Function(func_id),
        mutability: crate::runtime::BindingMutability::Immutable,
        visibility: match visibility {
            crate::syntax::ast::Visibility::Public => crate::runtime::BindingVisibility::Public,
            crate::syntax::ast::Visibility::Private => crate::runtime::BindingVisibility::Private,
        },
    };

    let env_ref =
        &mut context.interpreter.environments[context.interpreter.current_environment.0 as usize];
    env_ref.bindings.insert(name_symbol, binding);

    Ok(Value::Function(func_id))
}

// ── Conditionals ──

fn evaluate_conditional(
    context: &mut CallContext,
    condition_id: crate::syntax::ast::NodeId,
    consequent_id: crate::syntax::ast::NodeId,
    alternative_id: crate::syntax::ast::NodeId,
) -> EvalResult {
    let condition = evaluate_node(context, condition_id)?;

    let branch_id = match condition {
        Value::Boolean(true) => consequent_id,
        Value::Boolean(false) => alternative_id,
        _ => {
            return Err(Signal::Throw(typed_err(
                context,
                &["error", "type_error"],
                "if condition must be a boolean".to_string(),
            )));
        }
    };

    // Per spec: if the branch is a block, execute it
    let is_block = {
        let module = &context.interpreter.parsed_modules[context.interpreter.current_module];
        match module.node(branch_id).kind {
            crate::syntax::ast::NodeKind::Block(bid) => Some(bid),
            _ => None,
        }
    };
    if let Some(block_id) = is_block {
        return evaluate_block(context, block_id);
    }

    evaluate_node(context, branch_id)
}

// ── Loops ──

fn evaluate_loop(
    context: &mut CallContext,
    kind: crate::syntax::ast::LoopKind,
    condition_id: crate::syntax::ast::NodeId,
    body_id: crate::syntax::ast::BlockId,
) -> EvalResult {
    context.interpreter.call_stack.push(CallFrame {
        is_loop: true,
        function_name: None,
        call_span: None,
    });

    let result = (|| {
        let mut last_value = Value::Unit;
        loop {
            let condition = evaluate_node(context, condition_id)?;
            let should_run = match condition {
                Value::Boolean(value) => match kind {
                    crate::syntax::ast::LoopKind::While => value,
                    crate::syntax::ast::LoopKind::Until => !value,
                },
                _ => {
                    return Err(Signal::Throw(typed_err(
                        context,
                        &["error", "type_error"],
                        "loop condition must be a boolean".to_string(),
                    )));
                }
            };

            if !should_run {
                return Ok(last_value);
            }

            match evaluate_block(context, body_id) {
                Ok(value) => last_value = value,
                Err(Signal::Break(value)) => return Ok(value),
                Err(Signal::Continue) => continue,
                Err(signal @ Signal::Return(_)) | Err(signal @ Signal::Throw(_)) => {
                    return Err(signal);
                }
            }
        }
    })();
    context.interpreter.call_stack.pop();
    result
}

// ── Control Transfer ──

fn evaluate_return(
    context: &mut CallContext,
    value_id: Option<crate::syntax::ast::NodeId>,
) -> EvalResult {
    if context.interpreter.call_stack.is_empty()
        || !context.interpreter.call_stack.iter().any(|f| !f.is_loop)
    {
        return Err(Signal::Throw(typed_err(
            context,
            &["error", "control_flow_error"],
            "`return` used outside of a function".to_string(),
        )));
    }

    let value = match value_id {
        Some(id) => evaluate_node(context, id)?,
        None => Value::Unit,
    };

    Err(Signal::Return(value))
}

fn evaluate_break(
    context: &mut CallContext,
    value_id: Option<crate::syntax::ast::NodeId>,
) -> EvalResult {
    if !context.interpreter.call_stack.iter().any(|f| f.is_loop) {
        return Err(Signal::Throw(typed_err(
            context,
            &["error", "control_flow_error"],
            "`break` used outside of a loop".to_string(),
        )));
    }

    let value = match value_id {
        Some(id) => evaluate_node(context, id)?,
        None => Value::Unit,
    };

    Err(Signal::Break(value))
}

fn evaluate_throw(context: &mut CallContext, value_id: crate::syntax::ast::NodeId) -> EvalResult {
    let value = evaluate_node(context, value_id)?;

    // Validate that the thrown value is classified as :error
    let error_symbol = context.interpreter.symbols.intern("error");
    let is_error = match &value {
        Value::Namespace(ns_id) => {
            let ns = &context.interpreter.namespaces[ns_id.0 as usize];
            ns.types.contains(&error_symbol)
        }
        _ => false,
    };

    if !is_error {
        return Err(Signal::Throw(typed_err(
            context,
            &["error", "type_error"],
            format!(
                "throw requires an error value (type :error), got {}",
                value_type_name(&value)
            ),
        )));
    }

    if let Value::Namespace(namespace) = value {
        attach_error_metadata(context.interpreter, namespace);
        return Err(Signal::Throw(Value::Namespace(namespace)));
    }

    Err(Signal::Throw(value))
}

// ── eval ──

fn evaluate_eval(context: &mut CallContext, operand_id: crate::syntax::ast::NodeId) -> EvalResult {
    let value = evaluate_node(context, operand_id)?;

    let Value::Block(block_ref_id) = value else {
        return Err(Signal::Throw(typed_err(
            context,
            &["error", "type_error"],
            "eval requires a block value".to_string(),
        )));
    };

    // Evaluate the block in the current environment (no child scope)
    let (module_index, statements) = {
        let stored = &context.interpreter.stored_blocks[block_ref_id.0 as usize];
        let module = &context.interpreter.parsed_modules[stored.module_index];
        (
            stored.module_index,
            module.block(stored.block_id).statements.clone(),
        )
    };
    let previous_module = context.interpreter.current_module;
    context.interpreter.current_module = module_index;
    let result = evaluate_statement_list(context, &statements);
    context.interpreter.current_module = previous_module;
    result
}

// ── attempt ──

fn evaluate_attempt(
    context: &mut CallContext,
    block_id: crate::syntax::ast::BlockId,
) -> EvalResult {
    match evaluate_block(context, block_id) {
        Ok(value) => Ok(value),
        Err(Signal::Throw(error)) => Ok(error),
        Err(other) => Err(other),
    }
}

// ── new ──

fn evaluate_new(context: &mut CallContext, operand_id: crate::syntax::ast::NodeId) -> EvalResult {
    let value = evaluate_node(context, operand_id)?;

    let Value::Block(block_ref_id) = value else {
        return Err(Signal::Throw(typed_err(
            context,
            &["error", "type_error"],
            "new requires a block value".to_string(),
        )));
    };

    // Clone block statements before mutating context
    let (block_module, block_statements) = {
        let stored = &context.interpreter.stored_blocks[block_ref_id.0 as usize];
        let module = &context.interpreter.parsed_modules[stored.module_index];
        (
            stored.module_index,
            module.block(stored.block_id).statements.clone(),
        )
    };

    let ns_env_id = crate::runtime::EnvironmentId(context.interpreter.environments.len() as u32);
    context
        .interpreter
        .environments
        .push(crate::runtime::Environment::new(Some(
            context.interpreter.current_environment,
        )));

    let prev_env = context.interpreter.current_environment;
    let prev_module = context.interpreter.current_module;
    context.interpreter.current_environment = ns_env_id;
    context.interpreter.current_module = block_module;

    context.interpreter.call_stack.push(CallFrame {
        is_loop: false,
        function_name: None,
        call_span: None,
    });

    let result = evaluate_statement_list(context, &block_statements);
    context.interpreter.call_stack.pop();
    context.interpreter.current_environment = prev_env;
    context.interpreter.current_module = prev_module;

    // If execution failed, discard incomplete namespace
    match result {
        Ok(_) => {
            // Block completed — we ignore its value and return the namespace
        }
        Err(Signal::Return(value)) => {
            // return inside new — propagate it (it goes to the enclosing function)
            return Err(Signal::Return(value));
        }
        Err(Signal::Throw(error)) => {
            // Remove the failed namespace environment
            context.interpreter.environments.pop();
            return Err(Signal::Throw(error));
        }
        Err(other) => return Err(other),
    };

    // Validate optional types declaration
    let types_symbol = context.interpreter.symbols.intern("types");
    let fundamental_types: std::collections::HashSet<_> = [
        "unit",
        "boolean",
        "integer",
        "float",
        "string",
        "symbol",
        "list",
        "function",
        "block",
        "namespace",
    ]
    .into_iter()
    .map(|name| context.interpreter.symbols.intern(name))
    .collect();
    let ns_env = &context.interpreter.environments[ns_env_id.0 as usize];
    let mut types: Vec<crate::runtime::SymbolId> = Vec::new();
    let mut seen_types = std::collections::HashSet::new();

    if let Some(types_binding) = ns_env.bindings.get(&types_symbol) {
        // Must be pub set (immutable, public)
        if types_binding.visibility != crate::runtime::BindingVisibility::Public {
            context.interpreter.environments.pop();
            return Err(Signal::Throw(typed_err(
                context,
                &["error", "type_error"],
                "namespace `types` must be declared with `pub set`".to_string(),
            )));
        }
        if types_binding.mutability != crate::runtime::BindingMutability::Immutable {
            context.interpreter.environments.pop();
            return Err(Signal::Throw(typed_err(
                context,
                &["error", "type_error"],
                "namespace `types` must be immutable".to_string(),
            )));
        }

        // Must be a list of symbols
        if let Value::List(list) = &types_binding.value {
            let items: Vec<_> = list.iter().collect();
            for value in items {
                if let Value::Symbol(sym) = value {
                    if fundamental_types.contains(sym) {
                        context.interpreter.environments.pop();
                        return Err(Signal::Throw(typed_err(
                            context,
                            &["error", "type_error"],
                            "namespace `types` cannot contain a fundamental runtime type"
                                .to_string(),
                        )));
                    }
                    if !seen_types.insert(*sym) {
                        context.interpreter.environments.pop();
                        return Err(Signal::Throw(typed_err(
                            context,
                            &["error", "type_error"],
                            "namespace `types` cannot contain duplicates".to_string(),
                        )));
                    }
                    types.push(*sym);
                } else {
                    context.interpreter.environments.pop();
                    return Err(Signal::Throw(typed_err(
                        context,
                        &["error", "type_error"],
                        "namespace `types` must contain only symbols".to_string(),
                    )));
                }
            }
        } else {
            context.interpreter.environments.pop();
            return Err(Signal::Throw(typed_err(
                context,
                &["error", "type_error"],
                "namespace `types` must be a list".to_string(),
            )));
        }
    }

    let ns = crate::runtime::Namespace {
        environment: ns_env_id,
        types,
    };

    let ns_id = crate::runtime::NamespaceId(context.interpreter.namespaces.len() as u32);
    context.interpreter.namespaces.push(ns);

    Ok(Value::Namespace(ns_id))
}

// ── Import ──

fn evaluate_import(context: &mut CallContext, path: &str, alias: Option<&str>) -> EvalResult {
    if !context
        .interpreter
        .module_environments
        .contains(&context.interpreter.current_environment)
    {
        return Err(Signal::Throw(typed_err(
            context,
            &["error", "import_error"],
            "imports are permitted only at module scope".to_owned(),
        )));
    }

    let importer_name = context
        .interpreter
        .parsed_modules
        .get(context.interpreter.current_module)
        .and_then(|module| context.interpreter.sources.get(module.source))
        .map(|source| source.name.to_string());
    let importer = importer_name
        .as_deref()
        .filter(|name| !name.starts_with('<') && !name.starts_with("/po/"))
        .map(camino::Utf8Path::new);
    let identity = context
        .interpreter
        .module_loader
        .resolve(path, importer)
        .map_err(|error| {
            Signal::Throw(typed_err(
                context,
                &["error", "import_error"],
                error.to_string(),
            ))
        })?;

    let mod_env_id = load_module(context, &identity)?;

    let public_bindings: Vec<_> = {
        let mod_env = &context.interpreter.environments[mod_env_id.0 as usize];
        mod_env
            .bindings
            .iter()
            .filter(|(_, b)| b.visibility == crate::runtime::BindingVisibility::Public)
            .map(|(s, b)| (*s, b.value.clone()))
            .collect()
    };

    if let Some(alias_name) = alias {
        let ns = crate::runtime::Namespace {
            environment: mod_env_id,
            types: Vec::new(),
        };
        let ns_id = crate::runtime::NamespaceId(context.interpreter.namespaces.len() as u32);
        context.interpreter.namespaces.push(ns);

        let alias_symbol = context.interpreter.symbols.intern(alias_name);
        let curr_env_idx = context.interpreter.current_environment.0 as usize;
        {
            let curr_env = &context.interpreter.environments[curr_env_idx];
            if curr_env.bindings.contains_key(&alias_symbol) {
                return Err(Signal::Throw(typed_err(
                    context,
                    &["error", "name_error"],
                    format!("duplicate binding `{alias_name}`"),
                )));
            }
        }
        let curr_env = &mut context.interpreter.environments[curr_env_idx];
        curr_env.bindings.insert(
            alias_symbol,
            crate::runtime::Binding {
                value: Value::Namespace(ns_id),
                mutability: crate::runtime::BindingMutability::Immutable,
                visibility: crate::runtime::BindingVisibility::Public,
            },
        );
    } else {
        let curr_env_idx = context.interpreter.current_environment.0 as usize;
        {
            let curr_env = &context.interpreter.environments[curr_env_idx];
            for (symbol, _) in &public_bindings {
                if curr_env.bindings.contains_key(symbol) {
                    let name = context.interpreter.symbols.resolve(*symbol).unwrap_or("?");
                    return Err(Signal::Throw(typed_err(
                        context,
                        &["error", "name_error"],
                        format!("import collision: `{name}` already bound"),
                    )));
                }
            }
        }
        let curr_env = &mut context.interpreter.environments[curr_env_idx];
        for (symbol, value) in &public_bindings {
            curr_env.bindings.insert(
                *symbol,
                crate::runtime::Binding {
                    value: value.clone(),
                    mutability: crate::runtime::BindingMutability::ImportedReadOnly {
                        environment: mod_env_id,
                        symbol: *symbol,
                    },
                    visibility: crate::runtime::BindingVisibility::Public,
                },
            );
        }
    }

    Ok(Value::Unit)
}

fn load_module(
    context: &mut CallContext,
    identity: &crate::engine::ModuleIdentity,
) -> Result<crate::runtime::EnvironmentId, Signal> {
    if let Some(record) = context.interpreter.module_loader.record(identity) {
        match record.state {
            crate::engine::ModuleState::Loaded => {
                if let Some(environment) = record.environment {
                    return Ok(environment);
                }
                return Err(Signal::Throw(typed_err(
                    context,
                    &["error", "internal_error"],
                    format!("loaded module `{}` has no environment", identity.path()),
                )));
            }
            crate::engine::ModuleState::Loading => {
                let cycle = context
                    .interpreter
                    .module_loader
                    .cycle(identity)
                    .into_iter()
                    .map(|module| module.path().to_string())
                    .collect::<Vec<_>>()
                    .join(" -> ");
                return Err(Signal::Throw(typed_err(
                    context,
                    &["error", "import_error", "import_cycle"],
                    format!("import cycle detected: {cycle}"),
                )));
            }
            crate::engine::ModuleState::Failed => {
                if let Some(error) = record.cached_error.clone() {
                    return Err(Signal::Throw(error));
                }
                return Err(Signal::Throw(typed_err(
                    context,
                    &["error", "internal_error"],
                    format!("failed module `{}` has no cached error", identity.path()),
                )));
            }
            crate::engine::ModuleState::Unloaded => {}
        }
    }

    context
        .interpreter
        .module_loader
        .begin_loading(identity.clone());

    if matches!(
        identity,
        crate::engine::ModuleIdentity::Virtual(path) if path.as_str() == "/po/io"
    ) {
        return load_io_module(context, identity);
    }

    let source = match identity {
        crate::engine::ModuleIdentity::Virtual(path) if path.as_str() == "/po/library/standard" => {
            include_str!("../../stdlib/standard.po").to_owned()
        }
        crate::engine::ModuleIdentity::Virtual(path) => {
            return fail_module(
                context,
                identity,
                format!("virtual module `{path}` not found"),
            );
        }
        crate::engine::ModuleIdentity::File(path) => {
            std::fs::read_to_string(path).map_err(|error| {
                let value = typed_err(
                    context,
                    &["error", "import_error"],
                    format!("could not read module `{path}`: {error}"),
                );
                let record = context
                    .interpreter
                    .module_loader
                    .record_mut(identity.clone());
                record.state = crate::engine::ModuleState::Failed;
                record.cached_error = Some(value.clone());
                context.interpreter.module_loader.finish_loading(identity);
                Signal::Throw(value)
            })?
        }
    };

    let source_id = context
        .interpreter
        .sources
        .add(identity.path().as_str(), source.clone());
    let tokens = crate::syntax::lexer::lex(source_id, &source)
        .map_err(|diagnostics| module_diagnostic_error(context, identity, "lex", &diagnostics))?;
    let module = crate::syntax::parser::parse(&tokens)
        .map_err(|diagnostics| module_diagnostic_error(context, identity, "parse", &diagnostics))?;
    let module_index = context.interpreter.parsed_modules.len();
    context.interpreter.parsed_modules.push(module);

    let mod_env_id = crate::runtime::EnvironmentId(context.interpreter.environments.len() as u32);
    context
        .interpreter
        .environments
        .push(crate::runtime::Environment::new(Some(
            crate::runtime::EnvironmentId(0),
        )));
    context.interpreter.module_environments.insert(mod_env_id);
    {
        let record = context
            .interpreter
            .module_loader
            .record_mut(identity.clone());
        record.environment = Some(mod_env_id);
        record.module_index = Some(module_index);
    }

    let prev_env = context.interpreter.current_environment;
    let prev_module = context.interpreter.current_module;
    context.interpreter.current_environment = mod_env_id;
    context.interpreter.current_module = module_index;
    let statements = context.interpreter.parsed_modules[module_index]
        .statements
        .clone();
    let result = evaluate_statement_list(context, &statements);
    context.interpreter.current_environment = prev_env;
    context.interpreter.current_module = prev_module;

    match result {
        Ok(_) => {
            context
                .interpreter
                .module_loader
                .record_mut(identity.clone())
                .state = crate::engine::ModuleState::Loaded;
            context.interpreter.module_loader.finish_loading(identity);
            Ok(mod_env_id)
        }
        Err(Signal::Throw(value)) => {
            let record = context
                .interpreter
                .module_loader
                .record_mut(identity.clone());
            record.state = crate::engine::ModuleState::Failed;
            record.cached_error = Some(value.clone());
            context.interpreter.module_loader.finish_loading(identity);
            Err(Signal::Throw(value))
        }
        Err(_) => fail_module(
            context,
            identity,
            format!(
                "module `{}` used control flow outside of a function or loop",
                identity.path()
            ),
        ),
    }
}

fn load_io_module(
    context: &mut CallContext,
    identity: &crate::engine::ModuleIdentity,
) -> Result<crate::runtime::EnvironmentId, Signal> {
    let environment = crate::runtime::EnvironmentId(context.interpreter.environments.len() as u32);
    let mut module_environment =
        crate::runtime::Environment::new(Some(crate::runtime::EnvironmentId(0)));

    for name in [crate::native::io::READ_TEXT, crate::native::io::WRITE_TEXT] {
        let Some(native) = context.interpreter.natives.find_id(name) else {
            return fail_module(
                context,
                identity,
                format!("native function `{name}` is not registered"),
            );
        };
        let symbol = context.interpreter.symbols.intern(name);
        module_environment.bindings.insert(
            symbol,
            crate::runtime::Binding {
                value: Value::NativeFunction(native),
                mutability: crate::runtime::BindingMutability::Immutable,
                visibility: crate::runtime::BindingVisibility::Public,
            },
        );
    }

    context.interpreter.environments.push(module_environment);
    context.interpreter.module_environments.insert(environment);
    let record = context
        .interpreter
        .module_loader
        .record_mut(identity.clone());
    record.environment = Some(environment);
    record.state = crate::engine::ModuleState::Loaded;
    context.interpreter.module_loader.finish_loading(identity);
    Ok(environment)
}

fn module_diagnostic_error(
    context: &mut CallContext,
    identity: &crate::engine::ModuleIdentity,
    phase: &str,
    diagnostics: &[crate::diagnostic::Diagnostic],
) -> Signal {
    let detail = diagnostics
        .first()
        .map(|diagnostic| diagnostic.message.as_str())
        .unwrap_or("unknown error");
    let value = typed_err(
        context,
        &["error", "import_error"],
        format!("module `{}` failed to {phase}: {detail}", identity.path()),
    );
    let record = context
        .interpreter
        .module_loader
        .record_mut(identity.clone());
    record.state = crate::engine::ModuleState::Failed;
    record.cached_error = Some(value.clone());
    context.interpreter.module_loader.finish_loading(identity);
    Signal::Throw(value)
}

fn fail_module<T>(
    context: &mut CallContext,
    identity: &crate::engine::ModuleIdentity,
    message: String,
) -> Result<T, Signal> {
    let value = typed_err(context, &["error", "import_error"], message);
    let record = context
        .interpreter
        .module_loader
        .record_mut(identity.clone());
    record.state = crate::engine::ModuleState::Failed;
    record.cached_error = Some(value.clone());
    context.interpreter.module_loader.finish_loading(identity);
    Err(Signal::Throw(value))
}

// ── Helpers ──

fn value_type_name(value: &Value) -> &'static str {
    match value {
        Value::Unit => ":unit",
        Value::Boolean(_) => ":boolean",
        Value::Integer(_) => ":integer",
        Value::Float(_) => ":float",
        Value::String(_) => ":string",
        Value::Symbol(_) => ":symbol",
        Value::List(_) => ":list",
        Value::Function(_) | Value::NativeFunction(_) => ":function",
        Value::Block(_) => ":block",
        Value::Namespace(_) => ":namespace",
    }
}

// ── Call Frame ──

#[derive(Debug)]
pub struct CallFrame {
    pub is_loop: bool,
    pub function_name: Option<String>,
    pub call_span: Option<crate::source::Span>,
}

// ── CallContext (NativeContext bridge) ──

pub struct CallContext<'a> {
    pub interpreter: &'a mut Interpreter,
}

impl<'a> CallContext<'a> {
    pub fn new(interpreter: &'a mut Interpreter) -> Self {
        Self { interpreter }
    }
}

impl crate::native::NativeContext for CallContext<'_> {
    fn typed_error(&mut self, types: &[&str], message: String) -> Value {
        create_error_value(self.interpreter, types, message)
    }
    fn intern_symbol(&mut self, name: &str) -> crate::runtime::SymbolId {
        self.interpreter.symbols.intern(name)
    }
    fn resolve_symbol(&self, id: crate::runtime::SymbolId) -> Option<&str> {
        self.interpreter.symbols.resolve(id)
    }
    fn namespace_type_symbols(
        &self,
        id: crate::runtime::NamespaceId,
    ) -> Vec<crate::runtime::SymbolId> {
        self.interpreter
            .namespaces
            .get(id.0 as usize)
            .map(|ns| ns.types.clone())
            .unwrap_or_default()
    }

    fn working_directory(&self) -> &std::path::Path {
        self.interpreter
            .module_loader
            .working_directory()
            .as_std_path()
    }
}

fn create_error_value(interpreter: &mut Interpreter, types: &[&str], message: String) -> Value {
    // Create a minimal error namespace
    let ns_env_id = crate::runtime::EnvironmentId(interpreter.environments.len() as u32);
    let mut ns_env = crate::runtime::Environment::new(None);

    // types list
    let type_symbols: Vec<crate::runtime::SymbolId> = types
        .iter()
        .map(|t| interpreter.symbols.intern(t))
        .collect();
    let types_list: crate::runtime::PersistentList =
        type_symbols.iter().map(|s| Value::Symbol(*s)).collect();

    ns_env.bindings.insert(
        interpreter.symbols.intern("types"),
        crate::runtime::Binding {
            value: Value::List(types_list),
            mutability: crate::runtime::BindingMutability::Immutable,
            visibility: crate::runtime::BindingVisibility::Public,
        },
    );

    // message
    ns_env.bindings.insert(
        interpreter.symbols.intern("message"),
        crate::runtime::Binding {
            value: Value::String(std::sync::Arc::from(message)),
            mutability: crate::runtime::BindingMutability::Immutable,
            visibility: crate::runtime::BindingVisibility::Public,
        },
    );

    interpreter.environments.push(ns_env);

    let ns = crate::runtime::Namespace {
        environment: ns_env_id,
        types: type_symbols,
    };

    let ns_id = crate::runtime::NamespaceId(interpreter.namespaces.len() as u32);
    interpreter.namespaces.push(ns);
    attach_error_metadata(interpreter, ns_id);

    Value::Namespace(ns_id)
}

fn attach_error_metadata(interpreter: &mut Interpreter, namespace: crate::runtime::NamespaceId) {
    let Some(origin) = interpreter.active_span else {
        return;
    };
    let stack = interpreter
        .call_stack
        .iter()
        .rev()
        .filter_map(|frame| {
            Some(crate::diagnostic::StackFrame {
                name: frame.function_name.clone()?,
                span: frame.call_span?,
            })
        })
        .collect();
    interpreter
        .error_metadata
        .insert(namespace, crate::runtime::ErrorMetadata { origin, stack });
}
