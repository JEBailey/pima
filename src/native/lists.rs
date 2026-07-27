use crate::runtime::{PersistentList, Value};

use super::{NativeContext, NativeDefinition, NativeRegistry, NativeResult};

pub fn register(registry: &mut NativeRegistry) {
    registry.register(NativeDefinition {
        name: "push",
        arity: super::Arity::Exact(2),
        call: native_push,
    });
    registry.register(NativeDefinition {
        name: "append",
        arity: super::Arity::Exact(2),
        call: native_append,
    });
    registry.register(NativeDefinition {
        name: "head",
        arity: super::Arity::Exact(1),
        call: native_head,
    });
    registry.register(NativeDefinition {
        name: "rest",
        arity: super::Arity::Exact(1),
        call: native_rest,
    });
    registry.register(NativeDefinition {
        name: "empty?",
        arity: super::Arity::Exact(1),
        call: native_empty,
    });
}

fn native_push(ctx: &mut dyn NativeContext, args: &[Value]) -> NativeResult {
    let [Value::List(list), value] = args else {
        return Err(ctx.typed_error(
            &["error", "type_error"],
            "push requires a list and a value".into(),
        ));
    };
    Ok(Value::List(list.push_front(value.clone())))
}

fn native_append(ctx: &mut dyn NativeContext, args: &[Value]) -> NativeResult {
    let [Value::List(list), value] = args else {
        return Err(ctx.typed_error(
            &["error", "type_error"],
            "append requires a list and a value".into(),
        ));
    };
    // Append: traverse the list and rebuild with value at the end
    let mut result = PersistentList::empty();
    // Collect all elements in reverse, add the new value, then rebuild
    let mut elements: Vec<Value> = list.iter().cloned().collect();
    elements.push(value.clone());
    for elem in elements.into_iter().rev() {
        result = result.push_front(elem);
    }
    Ok(Value::List(result))
}

fn native_head(ctx: &mut dyn NativeContext, args: &[Value]) -> NativeResult {
    let [Value::List(list)] = args else {
        return Err(ctx.typed_error(
            &["error", "type_error"],
            "head requires a list argument".into(),
        ));
    };
    match list.first() {
        Some(value) => Ok(value.clone()),
        None => Err(ctx.typed_error(
            &["error", "index_error"],
            "head called on empty list".into(),
        )),
    }
}

fn native_rest(ctx: &mut dyn NativeContext, args: &[Value]) -> NativeResult {
    let [Value::List(list)] = args else {
        return Err(ctx.typed_error(
            &["error", "type_error"],
            "rest requires a list argument".into(),
        ));
    };
    match list.rest() {
        Some(rest) => Ok(Value::List(rest)),
        None => Err(ctx.typed_error(
            &["error", "index_error"],
            "rest called on empty list".into(),
        )),
    }
}

fn native_empty(ctx: &mut dyn NativeContext, args: &[Value]) -> NativeResult {
    let [Value::List(list)] = args else {
        return Err(ctx.typed_error(
            &["error", "type_error"],
            "empty? requires a list argument".into(),
        ));
    };
    Ok(Value::Boolean(list.is_empty()))
}
