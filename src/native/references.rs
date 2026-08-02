use crate::runtime::Value;

use super::{Arity, NativeContext, NativeDefinition, NativeRegistry, NativeResult};

pub fn register(registry: &mut NativeRegistry) {
    registry.register(NativeDefinition {
        name: "same?",
        arity: Arity::Exact(2),
        call: same,
    });
}

fn same(_context: &mut dyn NativeContext, arguments: &[Value]) -> NativeResult {
    let [left, right] = arguments else {
        return Ok(Value::Boolean(false));
    };
    Ok(Value::Boolean(crate::runtime::same_reference(left, right)))
}
