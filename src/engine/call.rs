use crate::runtime::{Binding, BindingMutability, BindingVisibility, Environment, Value};

use super::eval::{
    CallContext, CallFrame, EvalResult, Signal, evaluate_node, match_pattern, typed_err,
    unique_captures,
};

/// Evaluates a call. Every Pima call supplies exactly one argument value.
pub(super) fn evaluate(
    context: &mut CallContext,
    callee_id: crate::syntax::ast::NodeId,
    argument_id: crate::syntax::ast::NodeId,
) -> EvalResult {
    let callee = evaluate_node(context, callee_id)?;
    if argument_has_placeholder(context, argument_id) {
        return partial(context, callee, argument_id);
    }

    let argument = evaluate_node(context, argument_id)?;
    match callee {
        Value::Function(function) => call_user(context, function, &argument),
        Value::NativeFunction(native) => call_native(context, native, &argument),
        other => Err(Signal::Throw(typed_err(
            context,
            &["error", "type_error"],
            format!("cannot call value of type {}", other.type_symbol()),
        ))),
    }
}

fn argument_has_placeholder(context: &CallContext, argument: crate::syntax::ast::NodeId) -> bool {
    let module = &context.interpreter.parsed_modules[context.interpreter.current_module];
    match &module.node(argument).kind {
        crate::syntax::ast::NodeKind::Placeholder => true,
        crate::syntax::ast::NodeKind::List(elements) => elements.iter().any(|element| {
            matches!(
                module.node(*element).kind,
                crate::syntax::ast::NodeKind::Placeholder
            )
        }),
        _ => false,
    }
}

fn partial(
    context: &mut CallContext,
    callee: Value,
    argument: crate::syntax::ast::NodeId,
) -> EvalResult {
    let Value::Function(function) = callee else {
        return Err(Signal::Throw(typed_err(
            context,
            &["error", "type_error"],
            "partial application requires a user function".to_owned(),
        )));
    };
    let crate::syntax::ast::Pattern::List(parameter_elements) = &function.parameter else {
        return Err(Signal::Throw(typed_err(
            context,
            &["error", "type_error"],
            "partial application requires a list parameter pattern".to_owned(),
        )));
    };
    let module = &context.interpreter.parsed_modules[context.interpreter.current_module];
    let crate::syntax::ast::NodeKind::List(argument_elements) = &module.node(argument).kind else {
        return Err(Signal::Throw(typed_err(
            context,
            &["error", "type_error"],
            "partial application placeholders must appear in the call argument list".to_owned(),
        )));
    };
    let argument_elements = argument_elements.clone();
    if argument_elements.len() != parameter_elements.len() {
        return Err(Signal::Throw(typed_err(
            context,
            &["error", "match_error"],
            "partial argument list does not match the function parameter pattern".to_owned(),
        )));
    }

    let mut remaining = Vec::new();
    let mut captures = Vec::new();
    for (pattern, argument) in parameter_elements.iter().zip(argument_elements) {
        if matches!(
            context.interpreter.parsed_modules[context.interpreter.current_module]
                .node(argument)
                .kind,
            crate::syntax::ast::NodeKind::Placeholder
        ) {
            remaining.push(pattern.clone());
            continue;
        }
        let value = evaluate_node(context, argument)?;
        let Some(matched) = match_pattern(context, pattern, &value)? else {
            return Err(Signal::Throw(typed_err(
                context,
                &["error", "match_error"],
                "partial argument does not match its parameter pattern".to_owned(),
            )));
        };
        captures.extend(matched);
    }
    let captures = unique_captures(context, captures)?;

    let mut environment = Environment::new(Some(function.environment.clone()));
    for (name, value) in captures {
        let symbol = context.interpreter.symbols.intern(&name);
        environment.bindings.insert(
            symbol,
            Binding {
                value,
                mutability: BindingMutability::Immutable,
                visibility: BindingVisibility::Private,
            },
        );
    }

    Ok(Value::Function(dumpster::unsync::Gc::new(
        crate::runtime::UserFunction {
            name: function.name,
            parameter: crate::syntax::ast::Pattern::List(remaining),
            body: function.body,
            body_module: function.body_module,
            environment: dumpster::unsync::Gc::new(std::cell::RefCell::new(environment)),
            declaration_span: function.declaration_span,
        },
    )))
}

fn call_user(
    context: &mut CallContext,
    function: crate::runtime::FunctionRef,
    argument: &Value,
) -> EvalResult {
    let Some(captures) = match_pattern(context, &function.parameter, argument)? else {
        return Err(Signal::Throw(typed_err(
            context,
            &["error", "match_error"],
            "function argument does not match its parameter pattern".to_owned(),
        )));
    };
    let captures = unique_captures(context, captures)?;
    let mut environment = Environment::new(Some(function.environment.clone()));
    for (name, value) in captures {
        let symbol = context.interpreter.symbols.intern(&name);
        environment.bindings.insert(
            symbol,
            Binding {
                value,
                mutability: BindingMutability::Immutable,
                visibility: BindingVisibility::Private,
            },
        );
    }
    let environment = dumpster::unsync::Gc::new(std::cell::RefCell::new(environment));

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
    let previous_environment = context.interpreter.current_environment.clone();
    let previous_module = context.interpreter.current_module;
    context.interpreter.current_environment = environment;
    context.interpreter.current_module = function.body_module;
    let result = match context.interpreter.parsed_modules[function.body_module]
        .node(function.body)
        .kind
    {
        crate::syntax::ast::NodeKind::Block(block) => super::eval::evaluate_block(context, block),
        _ => evaluate_node(context, function.body),
    };
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
    argument: &Value,
) -> EvalResult {
    let arguments = match argument {
        Value::List(arguments) => arguments.to_vec(),
        other => vec![other.clone()],
    };
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
                "native function `{}` argument list has the wrong length",
                definition.name
            ),
        )));
    }
    (definition.call)(context, &arguments).map_err(Signal::Throw)
}
