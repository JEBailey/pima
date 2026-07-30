use std::collections::HashSet;

use crate::runtime::{BindingMutability, BindingVisibility, Environment, Namespace, Value};

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

    let types = validate_types(context, environment.clone())?;
    let namespace = dumpster::unsync::Gc::new(Namespace {
        environment,
        types,
        error_metadata: std::cell::RefCell::new(None),
    });
    Ok(Value::Namespace(namespace))
}

fn validate_types(
    context: &mut CallContext,
    environment: crate::runtime::EnvironmentRef,
) -> Result<Vec<crate::runtime::SymbolId>, Signal> {
    let types_symbol = context.interpreter.symbols.intern("types");
    let fundamental = [
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
    .collect::<HashSet<_>>();
    let binding = environment.borrow().bindings.get(&types_symbol).cloned();
    let Some(binding) = binding else {
        return Ok(Vec::new());
    };
    if binding.visibility != BindingVisibility::Public
        || !matches!(binding.mutability, BindingMutability::Immutable)
    {
        return Err(Signal::Throw(typed_err(
            context,
            &["error", "type_error"],
            "namespace `types` must be declared with `pub set`".to_owned(),
        )));
    }
    let Value::List(list) = binding.value else {
        return Err(Signal::Throw(typed_err(
            context,
            &["error", "type_error"],
            "namespace `types` must be a list".to_owned(),
        )));
    };

    let mut result = Vec::new();
    let mut seen = HashSet::new();
    for value in list.iter() {
        let Value::Symbol(symbol) = value else {
            return Err(Signal::Throw(typed_err(
                context,
                &["error", "type_error"],
                "namespace `types` must contain only symbols".to_owned(),
            )));
        };
        if fundamental.contains(symbol) {
            return Err(Signal::Throw(typed_err(
                context,
                &["error", "type_error"],
                "namespace `types` cannot contain a fundamental runtime type".to_owned(),
            )));
        }
        if !seen.insert(*symbol) {
            return Err(Signal::Throw(typed_err(
                context,
                &["error", "type_error"],
                "namespace `types` cannot contain duplicates".to_owned(),
            )));
        }
        result.push(*symbol);
    }
    Ok(result)
}
