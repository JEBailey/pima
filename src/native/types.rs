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

    let mut symbols = vec![Value::Symbol(ctx.intern_symbol(arg.type_name()))];

    if matches!(arg, Value::RemoteNamespace(_)) {
        symbols.push(Value::Symbol(ctx.intern_symbol("namespace")));
    }

    if let Value::Namespace(ns_id) = arg {
        for sym_id in ctx.namespace_type_symbols(ns_id) {
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

    if value.type_name() == type_name {
        return Ok(Value::Boolean(true));
    }

    if matches!(value, Value::RemoteNamespace(_)) && type_name == "namespace" {
        return Ok(Value::Boolean(true));
    }

    if let Value::Namespace(ns_id) = value {
        for sym_id in ctx.namespace_type_symbols(ns_id) {
            if let Some(name) = ctx.resolve_symbol(sym_id)
                && name == type_name
            {
                return Ok(Value::Boolean(true));
            }
        }
    }

    Ok(Value::Boolean(false))
}
