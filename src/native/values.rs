use crate::runtime::Value;

use super::{Arity, NativeContext, NativeDefinition, NativeRegistry, NativeResult};

pub fn register(registry: &mut NativeRegistry) {
    registry.register(NativeDefinition {
        name: "copy",
        arity: Arity::Exact(1),
        call: copy,
    });
}

fn copy(context: &mut dyn NativeContext, arguments: &[Value]) -> NativeResult {
    let [value] = arguments else {
        unreachable!("native arity is checked before dispatch")
    };
    crate::runtime::copy_snapshot(value).map_err(|message| {
        context.typed_error(
            &["error", "copy_error", "uncopyable_value"],
            message.to_owned(),
        )
    })
}
