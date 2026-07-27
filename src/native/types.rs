use crate::runtime::{PersistentList, Value};

use super::{NativeContext, NativeDefinition, NativeRegistry, NativeResult};

pub fn register(registry: &mut NativeRegistry) {
    registry.register(NativeDefinition {
        name: "types",
        arity: super::Arity::Exact(1),
        call: native_types,
    });
    registry.register(NativeDefinition {
        name: "is?",
        arity: super::Arity::Exact(2),
        call: native_is,
    });
}

fn native_types(ctx: &mut dyn NativeContext, args: &[Value]) -> NativeResult {
    let [arg] = args else {
        return Err(ctx.typed_error(
            &["error", "type_error"],
            "types requires exactly one argument".into(),
        ));
    };

    let type_names = value_type_name(arg);
    let mut symbols: Vec<Value> = vec![Value::Symbol(ctx.intern_symbol(type_names))];

    // For namespaces, append custom types
    if let Value::Namespace(ns_id) = arg {
        for sym_id in ctx.namespace_type_symbols(*ns_id) {
            symbols.push(Value::Symbol(sym_id));
        }
    }

    let list: PersistentList = symbols.into_iter().collect();
    Ok(Value::List(list))
}

fn native_is(ctx: &mut dyn NativeContext, args: &[Value]) -> NativeResult {
    let [value, Value::Symbol(type_symbol)] = args else {
        return Err(ctx.typed_error(
            &["error", "type_error"],
            "is? requires a value and a symbol".into(),
        ));
    };

    let type_name = ctx.resolve_symbol(*type_symbol).unwrap_or("");

    // Check fundamental type
    let fundamental = value_type_name(value);
    if fundamental == type_name {
        return Ok(Value::Boolean(true));
    }

    // For namespaces, check custom types
    if let Value::Namespace(ns_id) = value {
        for sym_id in ctx.namespace_type_symbols(*ns_id) {
            if let Some(name) = ctx.resolve_symbol(sym_id)
                && name == type_name
            {
                return Ok(Value::Boolean(true));
            }
        }
    }

    Ok(Value::Boolean(false))
}

fn value_type_name(value: &Value) -> &'static str {
    match value {
        Value::Unit => "unit",
        Value::Boolean(_) => "boolean",
        Value::Integer(_) => "integer",
        Value::Float(_) => "float",
        Value::String(_) => "string",
        Value::Symbol(_) => "symbol",
        Value::List(_) => "list",
        Value::Function(_) | Value::NativeFunction(_) => "function",
        Value::Block(_) => "block",
        Value::Namespace(_) => "namespace",
    }
}
