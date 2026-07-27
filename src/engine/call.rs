use crate::runtime::{Binding, BindingMutability, BindingVisibility, Environment, Value};

use super::eval::{
    CallContext, CallFrame, EvalResult, Signal, evaluate_block, evaluate_node, typed_err,
    value_type_name,
};

/// Evaluates a call expression and dispatches it to a user or native function.
pub(super) fn evaluate(
    context: &mut CallContext,
    callee_id: crate::syntax::ast::NodeId,
    arguments: &[crate::syntax::ast::NodeId],
) -> EvalResult {
    let callee = evaluate_node(context, callee_id)?;
    let module = &context.interpreter.parsed_modules[context.interpreter.current_module];
    let has_placeholder = arguments.iter().any(|argument| {
        matches!(
            module.node(*argument).kind,
            crate::syntax::ast::NodeKind::Placeholder
        )
    });
    if has_placeholder {
        return partial(context, callee, arguments);
    }

    let evaluated = arguments
        .iter()
        .map(|argument| evaluate_node(context, *argument))
        .collect::<Result<Vec<_>, _>>()?;
    match callee {
        Value::Function(function) => call_user(context, function, &evaluated),
        Value::NativeFunction(native) => call_native(context, native, &evaluated),
        other => Err(Signal::Throw(typed_err(
            context,
            &["error", "type_error"],
            format!("cannot call value of type {}", value_type_name(&other)),
        ))),
    }
}

fn partial(
    context: &mut CallContext,
    callee: Value,
    arguments: &[crate::syntax::ast::NodeId],
) -> EvalResult {
    let Value::Function(function_id) = callee else {
        return Err(Signal::Throw(typed_err(
            context,
            &["error", "type_error"],
            "partial application requires a function".to_owned(),
        )));
    };
    let source = context.interpreter.functions[function_id.0 as usize].clone();
    if arguments.len() != source.parameters.len() {
        return Err(Signal::Throw(typed_err(
            context,
            &["error", "arity_error"],
            format!(
                "function expects {} arguments but got {}",
                source.parameters.len(),
                arguments.len()
            ),
        )));
    }

    let module = &context.interpreter.parsed_modules[context.interpreter.current_module];
    let placeholders = arguments
        .iter()
        .map(|argument| {
            matches!(
                module.node(*argument).kind,
                crate::syntax::ast::NodeKind::Placeholder
            )
        })
        .collect::<Vec<_>>();
    let mut remaining = Vec::new();
    let mut bound = Vec::new();
    for ((parameter, argument), placeholder) in
        source.parameters.iter().zip(arguments).zip(placeholders)
    {
        if placeholder {
            remaining.push(*parameter);
        } else {
            bound.push((*parameter, evaluate_node(context, *argument)?));
        }
    }

    let environment = crate::runtime::EnvironmentId(context.interpreter.environments.len() as u32);
    let mut bindings = Environment::new(Some(source.environment));
    for (symbol, value) in bound {
        bindings.bindings.insert(
            symbol,
            Binding {
                value,
                mutability: BindingMutability::Immutable,
                visibility: BindingVisibility::Private,
            },
        );
    }
    context.interpreter.environments.push(bindings);

    let function = crate::runtime::UserFunction {
        name: source.name,
        parameters: remaining,
        body: source.body,
        body_module: source.body_module,
        environment,
        declaration_span: source.declaration_span,
    };
    let id = crate::runtime::FunctionId(context.interpreter.functions.len() as u32);
    context.interpreter.functions.push(function);
    Ok(Value::Function(id))
}

fn call_user(
    context: &mut CallContext,
    function_id: crate::runtime::FunctionId,
    arguments: &[Value],
) -> EvalResult {
    let function = &context.interpreter.functions[function_id.0 as usize];
    if arguments.len() != function.parameters.len() {
        return Err(Signal::Throw(typed_err(
            context,
            &["error", "arity_error"],
            format!(
                "function expects {} arguments but got {}",
                function.parameters.len(),
                arguments.len()
            ),
        )));
    }

    let environment = crate::runtime::EnvironmentId(context.interpreter.environments.len() as u32);
    let mut bindings = Environment::new(Some(function.environment));
    for (parameter, value) in function.parameters.iter().zip(arguments) {
        bindings.bindings.insert(
            *parameter,
            Binding {
                value: value.clone(),
                mutability: BindingMutability::Immutable,
                visibility: BindingVisibility::Private,
            },
        );
    }
    context.interpreter.environments.push(bindings);

    let name = context
        .interpreter
        .symbols
        .resolve(function.name)
        .unwrap_or("<anonymous>")
        .to_owned();
    context.interpreter.call_stack.push(CallFrame {
        is_loop: false,
        function_name: Some(name),
        call_span: context.interpreter.active_span,
    });
    let previous_environment = context.interpreter.current_environment;
    let previous_module = context.interpreter.current_module;
    context.interpreter.current_environment = environment;
    context.interpreter.current_module = function.body_module;
    let result = evaluate_block(context, function.body);
    context.interpreter.current_environment = previous_environment;
    context.interpreter.current_module = previous_module;
    context.interpreter.call_stack.pop();

    match result {
        Ok(value) | Err(Signal::Return(value)) => Ok(value),
        Err(Signal::Break(_)) => Err(Signal::Throw(typed_err(
            context,
            &["error", "control_flow_error"],
            "`break` cannot cross function boundary".to_owned(),
        ))),
        Err(Signal::Continue) => Err(Signal::Throw(typed_err(
            context,
            &["error", "control_flow_error"],
            "`continue` cannot cross function boundary".to_owned(),
        ))),
        Err(Signal::Throw(error)) => Err(Signal::Throw(error)),
    }
}

fn call_native(
    context: &mut CallContext,
    native: crate::runtime::NativeFunctionId,
    arguments: &[Value],
) -> EvalResult {
    let definition = context
        .interpreter
        .natives
        .get(native)
        .expect("registered native function id must remain valid");
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
    (definition.call)(context, arguments).map_err(Signal::Throw)
}
