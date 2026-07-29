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
        name: "code_point",
        arity: super::Arity::Exact(1),
        call: native_code_point,
    });
    registry.register(NativeDefinition {
        name: "from_code_point",
        arity: super::Arity::Exact(1),
        call: native_from_code_point,
    });
    registry.register(NativeDefinition {
        name: "string",
        arity: super::Arity::Exact(1),
        call: native_string,
    });
    for (name, arity, call) in [
        ("lower", super::Arity::Exact(1), native_lower as _),
        ("upper", super::Arity::Exact(1), native_upper as _),
        ("trim", super::Arity::Exact(1), native_trim as _),
        ("contains?", super::Arity::Exact(2), native_contains as _),
        (
            "starts_with?",
            super::Arity::Exact(2),
            native_starts_with as _,
        ),
        ("ends_with?", super::Arity::Exact(2), native_ends_with as _),
        ("replace", super::Arity::Exact(3), native_replace as _),
        ("split", super::Arity::Exact(2), native_split as _),
        ("join", super::Arity::Exact(2), native_join as _),
    ] {
        registry.register(NativeDefinition { name, arity, call });
    }
}

fn native_concat(ctx: &mut dyn NativeContext, args: &[Value]) -> NativeResult {
    let mut result = String::new();
    for arg in args {
        if let Value::String(s) = arg {
            result.push_str(s);
        } else {
            return Err(ctx.typed_error(
                &["error", "type_error"],
                format!(
                    "concat requires string arguments, got {}",
                    arg.type_symbol()
                ),
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

fn native_code_point(ctx: &mut dyn NativeContext, args: &[Value]) -> NativeResult {
    let [Value::String(string)] = args else {
        return Err(ctx.typed_error(
            &["error", "type_error"],
            "code_point requires a string".to_owned(),
        ));
    };
    let mut characters = string.chars();
    let Some(character) = characters.next() else {
        return Err(ctx.typed_error(
            &["error", "value_error"],
            "code_point requires exactly one Unicode scalar value".to_owned(),
        ));
    };
    if characters.next().is_some() {
        return Err(ctx.typed_error(
            &["error", "value_error"],
            "code_point requires exactly one Unicode scalar value".to_owned(),
        ));
    }
    Ok(Value::Integer(i64::from(u32::from(character))))
}

fn native_from_code_point(ctx: &mut dyn NativeContext, args: &[Value]) -> NativeResult {
    let [Value::Integer(code_point)] = args else {
        return Err(ctx.typed_error(
            &["error", "type_error"],
            "from_code_point requires an integer".to_owned(),
        ));
    };
    let Ok(code_point) = u32::try_from(*code_point) else {
        return Err(ctx.typed_error(
            &["error", "value_error"],
            "from_code_point requires a valid Unicode scalar value".to_owned(),
        ));
    };
    let Some(character) = char::from_u32(code_point) else {
        return Err(ctx.typed_error(
            &["error", "value_error"],
            "from_code_point requires a valid Unicode scalar value".to_owned(),
        ));
    };
    Ok(Value::String(Arc::from(character.to_string())))
}

fn native_string(ctx: &mut dyn NativeContext, args: &[Value]) -> NativeResult {
    let [arg] = args else {
        return Ok(Value::String(Arc::from("")));
    };
    Ok(Value::String(Arc::from(super::display::value(arg, ctx))))
}

fn unary_string(
    ctx: &mut dyn NativeContext,
    args: &[Value],
    name: &str,
    operation: impl FnOnce(&str) -> String,
) -> NativeResult {
    let [Value::String(input)] = args else {
        return Err(ctx.typed_error(
            &["error", "type_error"],
            format!("{name} requires a string"),
        ));
    };
    Ok(Value::String(Arc::from(operation(input))))
}

fn native_lower(ctx: &mut dyn NativeContext, args: &[Value]) -> NativeResult {
    unary_string(ctx, args, "lower", str::to_lowercase)
}

fn native_upper(ctx: &mut dyn NativeContext, args: &[Value]) -> NativeResult {
    unary_string(ctx, args, "upper", str::to_uppercase)
}

fn native_trim(ctx: &mut dyn NativeContext, args: &[Value]) -> NativeResult {
    unary_string(ctx, args, "trim", |input| input.trim().to_owned())
}

fn string_pair<'a>(
    ctx: &mut dyn NativeContext,
    args: &'a [Value],
    name: &str,
) -> Result<(&'a str, &'a str), Value> {
    let [Value::String(left), Value::String(right)] = args else {
        return Err(ctx.typed_error(
            &["error", "type_error"],
            format!("{name} requires two strings"),
        ));
    };
    Ok((left, right))
}

fn native_contains(ctx: &mut dyn NativeContext, args: &[Value]) -> NativeResult {
    let (text, pattern) = string_pair(ctx, args, "contains?")?;
    Ok(Value::Boolean(text.contains(pattern)))
}

fn native_starts_with(ctx: &mut dyn NativeContext, args: &[Value]) -> NativeResult {
    let (text, pattern) = string_pair(ctx, args, "starts_with?")?;
    Ok(Value::Boolean(text.starts_with(pattern)))
}

fn native_ends_with(ctx: &mut dyn NativeContext, args: &[Value]) -> NativeResult {
    let (text, pattern) = string_pair(ctx, args, "ends_with?")?;
    Ok(Value::Boolean(text.ends_with(pattern)))
}

fn native_replace(ctx: &mut dyn NativeContext, args: &[Value]) -> NativeResult {
    let [Value::String(text), Value::String(from), Value::String(to)] = args else {
        return Err(ctx.typed_error(
            &["error", "type_error"],
            "replace requires three strings".to_owned(),
        ));
    };
    Ok(Value::String(Arc::from(text.replace(from.as_ref(), to))))
}

fn native_split(ctx: &mut dyn NativeContext, args: &[Value]) -> NativeResult {
    let (text, separator) = string_pair(ctx, args, "split")?;
    if separator.is_empty() {
        return Err(ctx.typed_error(
            &["error", "value_error"],
            "split separator cannot be empty".to_owned(),
        ));
    }
    Ok(Value::List(
        text.split(separator)
            .map(|part| Value::String(Arc::from(part)))
            .collect(),
    ))
}

fn native_join(ctx: &mut dyn NativeContext, args: &[Value]) -> NativeResult {
    let [Value::List(values), Value::String(separator)] = args else {
        return Err(ctx.typed_error(
            &["error", "type_error"],
            "join requires a list and a string separator".to_owned(),
        ));
    };
    let mut strings = Vec::with_capacity(values.len());
    for value in values.iter() {
        let Value::String(string) = value else {
            return Err(ctx.typed_error(
                &["error", "type_error"],
                "join list must contain only strings".to_owned(),
            ));
        };
        strings.push(string.as_ref());
    }
    Ok(Value::String(Arc::from(strings.join(separator))))
}
