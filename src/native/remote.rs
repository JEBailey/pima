use crate::runtime::Value;

use super::{Arity, NativeContext, NativeDefinition, NativeRegistry, NativeResult};

pub const EXPORTS: &[(&str, &str)] = &[("alive?", "remote.alive?"), ("stop", "remote.stop")];

pub fn register(registry: &mut NativeRegistry) {
    register_native(registry, "remote.alive?", Arity::Exact(1), alive);
    register_native(registry, "remote.stop", Arity::Exact(1), stop);
}

fn register_native(
    registry: &mut NativeRegistry,
    name: &'static str,
    arity: Arity,
    call: super::NativeCall,
) {
    registry.register(NativeDefinition { name, arity, call });
}

fn alive(context: &mut dyn NativeContext, arguments: &[Value]) -> NativeResult {
    let [Value::RemoteNamespace(handle)] = arguments else {
        return Err(type_error(
            context,
            "Remote.alive? requires one remote object",
        ));
    };
    context
        .remote_alive(*handle)
        .map(Value::Boolean)
        .map_err(|message| remote_error(context, message))
}

fn stop(context: &mut dyn NativeContext, arguments: &[Value]) -> NativeResult {
    let [Value::RemoteNamespace(handle)] = arguments else {
        return Err(type_error(
            context,
            "Remote.stop requires one remote object",
        ));
    };
    context
        .remote_stop(*handle)
        .map(|()| Value::Unit)
        .map_err(|message| remote_error(context, message))
}

fn type_error(context: &mut dyn NativeContext, message: &str) -> Value {
    context.typed_error(&["error", "type_error"], message.to_owned())
}

fn remote_error(context: &mut dyn NativeContext, message: String) -> Value {
    context.typed_error(&["error", "remote_error"], message)
}
