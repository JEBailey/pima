use std::collections::HashSet;

use super::{
    BindingMutability, BindingVisibility, EnvironmentRef, SymbolId, SymbolInterner, Value,
};

pub fn namespace_types(
    symbols: &mut SymbolInterner,
    environment: &EnvironmentRef,
) -> Result<Vec<SymbolId>, String> {
    let types_symbol = symbols.intern("types");
    let binding = environment.borrow().bindings.get(&types_symbol).cloned();
    let Some(binding) = binding else {
        return Ok(Vec::new());
    };
    if binding.visibility != BindingVisibility::Public
        || !matches!(binding.mutability, BindingMutability::Immutable)
    {
        return Err("object `types` must be declared with `pub val`".into());
    }
    let Value::List(list) = binding.value.resolved() else {
        return Err("object `types` must be a list".into());
    };
    let fundamental = [
        "unit", "boolean", "integer", "float", "string", "symbol", "list", "function", "block",
        "object",
    ]
    .into_iter()
    .map(|name| symbols.intern(name))
    .collect::<HashSet<_>>();
    let mut result = Vec::new();
    let mut seen = HashSet::new();
    for value in list.iter() {
        let Value::Symbol(symbol) = value else {
            return Err("object `types` must contain only symbols".into());
        };
        if fundamental.contains(symbol) {
            return Err("object `types` cannot contain a fundamental runtime type".into());
        }
        if !seen.insert(*symbol) {
            return Err("object `types` cannot contain duplicates".into());
        }
        result.push(*symbol);
    }
    Ok(result)
}

pub fn throwable_error(symbols: &mut SymbolInterner, value: &Value) -> Result<(), String> {
    let error_symbol = symbols.intern("error");
    let message_symbol = symbols.intern("message");
    let Value::Namespace(namespace) = value else {
        return Err(format!(
            "throw requires an error value (type :error), got {}",
            value.type_symbol()
        ));
    };
    if !namespace.types.contains(&error_symbol) {
        return Err(format!(
            "throw requires an error value (type :error), got {}",
            value.type_symbol()
        ));
    }
    let valid_message = namespace
        .environment
        .borrow()
        .bindings
        .get(&message_symbol)
        .is_some_and(|binding| {
            binding.visibility == BindingVisibility::Public
                && matches!(binding.mutability, BindingMutability::Immutable)
                && matches!(binding.value.resolved(), Value::String(_))
        });
    if !valid_message {
        return Err("an error object must expose `message` as a public immutable string".into());
    }
    Ok(())
}
