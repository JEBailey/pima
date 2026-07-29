use crate::runtime::Value;

use super::{NativeContext, NativeDefinition, NativeRegistry, NativeResult};

pub fn register(registry: &mut NativeRegistry) {
    registry.register(NativeDefinition {
        name: "+",
        arity: super::Arity::AtLeast(2),
        call: native_add,
    });
    registry.register(NativeDefinition {
        name: "-",
        arity: super::Arity::AtLeast(2),
        call: native_subtract,
    });
    registry.register(NativeDefinition {
        name: "*",
        arity: super::Arity::AtLeast(2),
        call: native_multiply,
    });
    registry.register(NativeDefinition {
        name: "/",
        arity: super::Arity::Exact(2),
        call: native_divide,
    });
    registry.register(NativeDefinition {
        name: "div",
        arity: super::Arity::Exact(2),
        call: native_int_divide,
    });
    registry.register(NativeDefinition {
        name: "mod",
        arity: super::Arity::Exact(2),
        call: native_int_mod,
    });
    registry.register(NativeDefinition {
        name: "<",
        arity: super::Arity::Exact(2),
        call: native_less_than,
    });
    registry.register(NativeDefinition {
        name: ">",
        arity: super::Arity::Exact(2),
        call: native_greater_than,
    });
    registry.register(NativeDefinition {
        name: "=",
        arity: super::Arity::Exact(2),
        call: native_equals,
    });
    registry.register(NativeDefinition {
        name: "not",
        arity: super::Arity::Exact(1),
        call: native_not,
    });
    registry.register(NativeDefinition {
        name: "int",
        arity: super::Arity::Exact(1),
        call: native_int,
    });
}

// ── Helper: extract i64 or f64 from a Value, error if not numeric ──

fn to_num(v: &Value) -> Option<f64> {
    match v {
        Value::Integer(n) => Some(*n as f64),
        Value::Float(f) => Some(*f),
        _ => None,
    }
}

// ── Variadic arithmetic: fold left ──

fn native_add(ctx: &mut dyn NativeContext, args: &[Value]) -> NativeResult {
    let mut iter = args.iter();
    let mut acc = first_numeric(ctx, iter.next(), "addition")?;
    for arg in iter {
        acc = do_add(ctx, acc, arg)?;
    }
    Ok(acc)
}

fn do_add(ctx: &mut dyn NativeContext, a: Value, b: &Value) -> NativeResult {
    let (a_type, b_type) = (a.type_symbol(), b.type_symbol());
    match (a, b) {
        (Value::Integer(x), Value::Integer(y)) => {
            x.checked_add(*y).map(Value::Integer).ok_or_else(|| {
                ctx.typed_error(
                    &["error", "numeric_error"],
                    "integer overflow in addition".into(),
                )
            })
        }
        (Value::Integer(x), Value::Float(y)) => Ok(Value::Float(x as f64 + y)),
        (Value::Float(x), Value::Integer(y)) => Ok(Value::Float(x + *y as f64)),
        (Value::Float(x), Value::Float(y)) => Ok(Value::Float(x + y)),
        _ => Err(ctx.typed_error(
            &["error", "type_error"],
            format!(
                "addition requires numeric arguments, got {} and {}",
                a_type, b_type
            ),
        )),
    }
}

fn native_subtract(ctx: &mut dyn NativeContext, args: &[Value]) -> NativeResult {
    let mut iter = args.iter();
    let mut acc = first_numeric(ctx, iter.next(), "subtraction")?;
    for arg in iter {
        acc = do_sub(ctx, acc, arg)?;
    }
    Ok(acc)
}

fn do_sub(ctx: &mut dyn NativeContext, a: Value, b: &Value) -> NativeResult {
    let (a_type, b_type) = (a.type_symbol(), b.type_symbol());
    match (a, b) {
        (Value::Integer(x), Value::Integer(y)) => {
            x.checked_sub(*y).map(Value::Integer).ok_or_else(|| {
                ctx.typed_error(
                    &["error", "numeric_error"],
                    "integer overflow in subtraction".into(),
                )
            })
        }
        (Value::Integer(x), Value::Float(y)) => Ok(Value::Float(x as f64 - y)),
        (Value::Float(x), Value::Integer(y)) => Ok(Value::Float(x - *y as f64)),
        (Value::Float(x), Value::Float(y)) => Ok(Value::Float(x - y)),
        _ => Err(ctx.typed_error(
            &["error", "type_error"],
            format!(
                "subtraction requires numeric arguments, got {} and {}",
                a_type, b_type
            ),
        )),
    }
}

fn native_multiply(ctx: &mut dyn NativeContext, args: &[Value]) -> NativeResult {
    let mut iter = args.iter();
    let mut acc = first_numeric(ctx, iter.next(), "multiplication")?;
    for arg in iter {
        acc = do_mul(ctx, acc, arg)?;
    }
    Ok(acc)
}

fn do_mul(ctx: &mut dyn NativeContext, a: Value, b: &Value) -> NativeResult {
    let (a_type, b_type) = (a.type_symbol(), b.type_symbol());
    match (a, b) {
        (Value::Integer(x), Value::Integer(y)) => {
            x.checked_mul(*y).map(Value::Integer).ok_or_else(|| {
                ctx.typed_error(
                    &["error", "numeric_error"],
                    "integer overflow in multiplication".into(),
                )
            })
        }
        (Value::Integer(x), Value::Float(y)) => Ok(Value::Float(x as f64 * y)),
        (Value::Float(x), Value::Integer(y)) => Ok(Value::Float(x * *y as f64)),
        (Value::Float(x), Value::Float(y)) => Ok(Value::Float(x * y)),
        _ => Err(ctx.typed_error(
            &["error", "type_error"],
            format!(
                "multiplication requires numeric arguments, got {} and {}",
                a_type, b_type
            ),
        )),
    }
}

fn first_numeric(ctx: &mut dyn NativeContext, first: Option<&Value>, op: &str) -> NativeResult {
    match first {
        Some(v) if matches!(v, Value::Integer(_) | Value::Float(_)) => Ok(v.clone()),
        Some(v) => Err(ctx.typed_error(
            &["error", "type_error"],
            format!("{} requires numeric arguments, got {}", op, v.type_symbol()),
        )),
        None => Err(ctx.typed_error(
            &["error", "type_error"],
            format!("{} requires at least one argument", op),
        )),
    }
}

// ── Binary division (always returns float) ──

fn native_divide(ctx: &mut dyn NativeContext, args: &[Value]) -> NativeResult {
    let [a, b] = args else {
        return Err(ctx.typed_error(
            &["error", "type_error"],
            "division requires two arguments".into(),
        ));
    };
    let denominator_is_zero =
        matches!(b, Value::Integer(0)) || matches!(b, Value::Float(value) if *value == 0.0);
    if denominator_is_zero {
        return Err(ctx.typed_error(&["error", "numeric_error"], "division by zero".into()));
    }

    match (a, b) {
        (Value::Integer(x), Value::Integer(y)) => Ok(Value::Float(*x as f64 / *y as f64)),
        (Value::Integer(x), Value::Float(y)) => Ok(Value::Float(*x as f64 / y)),
        (Value::Float(x), Value::Integer(y)) => Ok(Value::Float(x / *y as f64)),
        (Value::Float(x), Value::Float(y)) => Ok(Value::Float(x / y)),
        _ => Err(ctx.typed_error(
            &["error", "type_error"],
            format!(
                "division requires numeric arguments, got {} and {}",
                a.type_symbol(),
                b.type_symbol()
            ),
        )),
    }
}

fn native_int_divide(ctx: &mut dyn NativeContext, args: &[Value]) -> NativeResult {
    let [Value::Integer(a), Value::Integer(b)] = args else {
        return Err(ctx.typed_error(
            &["error", "type_error"],
            "div requires integer arguments".into(),
        ));
    };
    match a.checked_div(*b) {
        Some(value) => Ok(Value::Integer(value)),
        None if *b == 0 => {
            Err(ctx.typed_error(&["error", "numeric_error"], "division by zero".into()))
        }
        None => Err(ctx.typed_error(
            &["error", "numeric_error"],
            "integer overflow in division".into(),
        )),
    }
}

fn native_int_mod(ctx: &mut dyn NativeContext, args: &[Value]) -> NativeResult {
    let [Value::Integer(a), Value::Integer(b)] = args else {
        return Err(ctx.typed_error(
            &["error", "type_error"],
            "mod requires integer arguments".into(),
        ));
    };
    if *b == 0 {
        return Err(ctx.typed_error(&["error", "numeric_error"], "division by zero".into()));
    }
    if *a == i64::MIN && *b == -1 {
        return Ok(Value::Integer(0));
    }
    Ok(Value::Integer(a.rem_euclid(*b)))
}

// ── Comparison ──

fn native_less_than(ctx: &mut dyn NativeContext, args: &[Value]) -> NativeResult {
    let [a, b] = args else {
        return Err(ctx.typed_error(&["error", "type_error"], "< requires two arguments".into()));
    };
    cmp_num(ctx, a, b, |x, y| x < y)
}

fn native_greater_than(ctx: &mut dyn NativeContext, args: &[Value]) -> NativeResult {
    let [a, b] = args else {
        return Err(ctx.typed_error(&["error", "type_error"], "> requires two arguments".into()));
    };
    cmp_num(ctx, a, b, |x, y| x > y)
}

fn cmp_num(
    ctx: &mut dyn NativeContext,
    a: &Value,
    b: &Value,
    cmp: impl FnOnce(f64, f64) -> bool,
) -> NativeResult {
    match (to_num(a), to_num(b)) {
        (Some(x), Some(y)) => Ok(Value::Boolean(cmp(x, y))),
        _ => Err(ctx.typed_error(
            &["error", "type_error"],
            format!(
                "comparison requires numeric arguments, got {} and {}",
                a.type_symbol(),
                b.type_symbol()
            ),
        )),
    }
}

fn native_equals(_ctx: &mut dyn NativeContext, args: &[Value]) -> NativeResult {
    let [a, b] = args else {
        return Ok(Value::Boolean(false));
    };
    Ok(Value::Boolean(crate::runtime::language_equal(a, b)))
}

// ── Boolean ──

fn native_not(ctx: &mut dyn NativeContext, args: &[Value]) -> NativeResult {
    let [Value::Boolean(b)] = args else {
        return Err(ctx.typed_error(
            &["error", "type_error"],
            "not requires a boolean argument".into(),
        ));
    };
    Ok(Value::Boolean(!b))
}

// ── Conversion ──

fn native_int(ctx: &mut dyn NativeContext, args: &[Value]) -> NativeResult {
    let [arg] = args else {
        return Err(ctx.typed_error(
            &["error", "type_error"],
            "int requires exactly one argument".into(),
        ));
    };
    match arg {
        Value::Integer(n) => Ok(Value::Integer(*n)),
        Value::Float(f) => {
            if f.is_nan() || f.is_infinite() {
                return Err(ctx.typed_error(
                    &["error", "conversion_error"],
                    "cannot convert NaN or infinity to integer".into(),
                ));
            }
            if *f >= i64::MAX as f64 || *f <= i64::MIN as f64 {
                return Err(ctx.typed_error(
                    &["error", "conversion_error"],
                    "float out of 64-bit integer range".into(),
                ));
            }
            Ok(Value::Integer(*f as i64))
        }
        _ => Err(ctx.typed_error(
            &["error", "type_error"],
            format!("int requires a number, got {}", arg.type_symbol()),
        )),
    }
}
