use crate::runtime::{BindingVisibility, Environment, Namespace, Value};

use super::eval::{CallContext, EvalResult, Signal, evaluate_block, evaluate_node, typed_err};

pub(super) fn member(
    context: &mut CallContext,
    object: crate::syntax::ast::NodeId,
    name: &str,
) -> EvalResult {
    let value = evaluate_node(context, object)?;
    let Value::Namespace(namespace) = value else {
        return Err(Signal::Throw(typed_err(
            context,
            &["error", "type_error"],
            format!(
                "member access `.` requires a namespace, got {}",
                value.type_symbol()
            ),
        )));
    };
    let symbol = context.interpreter.symbols.intern(name);
    let binding = namespace
        .environment
        .borrow()
        .bindings
        .get(&symbol)
        .cloned();
    let Some(binding) = binding else {
        return Err(Signal::Throw(typed_err(
            context,
            &["error", "name_error"],
            format!("namespace has no member `{name}`"),
        )));
    };
    if binding.visibility == BindingVisibility::Private {
        return Err(Signal::Throw(typed_err(
            context,
            &["error", "visibility_error"],
            format!("member `{name}` is private"),
        )));
    }
    Ok(binding.value)
}

/// Instantiates a block and publishes a namespace after successful validation.
pub(super) fn evaluate(
    context: &mut CallContext,
    operand: crate::syntax::ast::NodeId,
) -> EvalResult {
    let value = evaluate_node(context, operand)?;
    let Value::Block(block_reference) = value else {
        return Err(Signal::Throw(typed_err(
            context,
            &["error", "type_error"],
            "new requires a block value".to_owned(),
        )));
    };
    let (module_index, block_id) = { (block_reference.module_index, block_reference.block_id) };

    let environment = dumpster::unsync::Gc::new(std::cell::RefCell::new(Environment::new(Some(
        context.interpreter.current_environment.clone(),
    ))));
    let previous_environment = context.interpreter.current_environment.clone();
    let previous_module = context.interpreter.current_module;
    context.interpreter.current_environment = environment.clone();
    context.interpreter.current_module = module_index;
    let result = evaluate_block(context, block_id);
    context.interpreter.current_environment = previous_environment;
    context.interpreter.current_module = previous_module;
    result?;

    let types = crate::runtime::namespace_types(&mut context.interpreter.symbols, &environment)
        .map_err(|message| Signal::Throw(typed_err(context, &["error", "type_error"], message)))?;
    let namespace = dumpster::unsync::Gc::new(Namespace {
        environment,
        types,
        error_metadata: std::cell::RefCell::new(None),
    });
    Ok(Value::Namespace(namespace))
}
