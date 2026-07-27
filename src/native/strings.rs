use std::sync::Arc;

use crate::runtime::Value;

use super::{NativeContext, NativeDefinition, NativeRegistry, NativeResult};

pub fn register(registry: &mut NativeRegistry) {
    registry.register(NativeDefinition {
        name: "concat",
        arity: super::Arity::AtLeast(1),
        call: native_concat,
    });
    registry.register(NativeDefinition {
        name: "length",
        arity: super::Arity::Exact(1),
        call: native_length,
    });
    registry.register(NativeDefinition {
        name: "slice",
        arity: super::Arity::Exact(3),
        call: native_slice,
    });
    registry.register(NativeDefinition {
        name: "chars",
        arity: super::Arity::Exact(1),
        call: native_chars,
    });
    registry.register(NativeDefinition {
        name: "string",
        arity: super::Arity::Exact(1),
        call: native_string,
    });
}

fn native_concat(ctx: &mut dyn NativeContext, args: &[Value]) -> NativeResult {
    let mut result = String::new();
    for arg in args {
        if let Value::String(s) = arg {
            result.push_str(s);
        } else {
            return Err(ctx.typed_error(
                &["error", "type_error"],
                format!("concat requires string arguments, got {}", type_str(arg)),
            ));
        }
    }
    Ok(Value::String(Arc::from(result)))
}

fn native_length(ctx: &mut dyn NativeContext, args: &[Value]) -> NativeResult {
    let [Value::String(s)] = args else {
        return Err(ctx.typed_error(
            &["error", "type_error"],
            "length requires a string argument".into(),
        ));
    };
    Ok(Value::Integer(s.chars().count() as i64))
}

fn native_slice(ctx: &mut dyn NativeContext, args: &[Value]) -> NativeResult {
    let [Value::String(s), Value::Integer(begin), Value::Integer(end)] = args else {
        return Err(ctx.typed_error(
            &["error", "type_error"],
            "slice requires a string and two integer arguments".into(),
        ));
    };

    let chars: Vec<char> = s.chars().collect();
    let len = chars.len() as i64;

    if *begin < 0 || *end < 0 || *begin > *end || *end > len {
        return Err(ctx.typed_error(
            &["error", "index_error"],
            format!(
                "slice indices {:?}..{:?} out of range for string of length {}",
                begin, end, len
            ),
        ));
    }

    let slice: String = chars[*begin as usize..*end as usize].iter().collect();
    Ok(Value::String(Arc::from(slice)))
}

fn native_chars(ctx: &mut dyn NativeContext, args: &[Value]) -> NativeResult {
    let [Value::String(s)] = args else {
        return Err(ctx.typed_error(
            &["error", "type_error"],
            "chars requires a string argument".into(),
        ));
    };

    let list: crate::runtime::PersistentList = s
        .chars()
        .map(|c| Value::String(Arc::from(c.to_string())))
        .collect();
    Ok(Value::List(list))
}

fn native_string(ctx: &mut dyn NativeContext, args: &[Value]) -> NativeResult {
    let [arg] = args else {
        return Ok(Value::String(Arc::from("")));
    };
    Ok(Value::String(Arc::from(super::display::value(arg, ctx))))
}

fn type_str(value: &Value) -> &'static str {
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
