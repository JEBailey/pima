use std::sync::Arc;

use crate::runtime::Value;

use super::{Arity, NativeContext, NativeDefinition, NativeRegistry, NativeResult};

pub const EXPORTS: &[(&str, &str)] = &[
    ("listen", "tcp.listen"),
    ("accept", "tcp.accept"),
    ("read", "tcp.read"),
    ("write", "tcp.write"),
    ("set_timeout", "tcp.set_timeout"),
    ("close", "tcp.close"),
];

pub fn register(registry: &mut NativeRegistry) {
    register_native(registry, "tcp.listen", Arity::Exact(2), listen);
    register_native(registry, "tcp.accept", Arity::Exact(1), accept);
    register_native(registry, "tcp.read", Arity::Exact(2), read);
    register_native(registry, "tcp.write", Arity::Exact(2), write);
    register_native(registry, "tcp.set_timeout", Arity::Exact(2), set_timeout);
    register_native(registry, "tcp.close", Arity::Exact(1), close);
}

fn register_native(
    registry: &mut NativeRegistry,
    name: &'static str,
    arity: Arity,
    call: super::NativeCall,
) {
    registry.register(NativeDefinition { name, arity, call });
}

fn listen(context: &mut dyn NativeContext, arguments: &[Value]) -> NativeResult {
    let [Value::String(address), Value::Integer(port)] = arguments else {
        return Err(type_error(
            context,
            "listen requires an address string and integer port",
        ));
    };
    let port = u16::try_from(*port)
        .map_err(|_| value_error(context, "TCP port must be between 0 and 65535"))?;
    context
        .tcp_listen(address, port)
        .map(Value::TcpListener)
        .map_err(|message| tcp_error(context, message))
}

fn accept(context: &mut dyn NativeContext, arguments: &[Value]) -> NativeResult {
    let [Value::TcpListener(listener)] = arguments else {
        return Err(type_error(context, "accept requires a TCP listener"));
    };
    context
        .tcp_accept(*listener)
        .map(Value::TcpConnection)
        .map_err(|message| tcp_error(context, message))
}

fn read(context: &mut dyn NativeContext, arguments: &[Value]) -> NativeResult {
    let [Value::TcpConnection(connection), Value::Integer(maximum)] = arguments else {
        return Err(type_error(
            context,
            "read requires a TCP connection and integer maximum",
        ));
    };
    let maximum = usize::try_from(*maximum)
        .ok()
        .filter(|maximum| (1..=1024 * 1024).contains(maximum))
        .ok_or_else(|| value_error(context, "TCP read maximum must be between 1 and 1048576"))?;
    context
        .tcp_read(*connection, maximum)
        .map(|text| Value::String(Arc::from(text)))
        .map_err(|message| tcp_error(context, message))
}

fn write(context: &mut dyn NativeContext, arguments: &[Value]) -> NativeResult {
    let [Value::TcpConnection(connection), Value::String(text)] = arguments else {
        return Err(type_error(
            context,
            "write requires a TCP connection and string",
        ));
    };
    context
        .tcp_write(*connection, text)
        .map(|_| Value::Unit)
        .map_err(|message| tcp_error(context, message))
}

fn set_timeout(context: &mut dyn NativeContext, arguments: &[Value]) -> NativeResult {
    let [
        Value::TcpConnection(connection),
        Value::Integer(milliseconds),
    ] = arguments
    else {
        return Err(type_error(
            context,
            "set_timeout requires a TCP connection and integer milliseconds",
        ));
    };
    let milliseconds = u64::try_from(*milliseconds)
        .map_err(|_| value_error(context, "TCP timeout cannot be negative"))?;
    context
        .tcp_set_timeout(*connection, milliseconds)
        .map(|_| Value::Unit)
        .map_err(|message| tcp_error(context, message))
}

fn close(context: &mut dyn NativeContext, arguments: &[Value]) -> NativeResult {
    let result = match arguments {
        [Value::TcpListener(listener)] => context.tcp_close_listener(*listener),
        [Value::TcpConnection(connection)] => context.tcp_close_connection(*connection),
        _ => return Err(type_error(context, "close requires a TCP resource")),
    };
    result
        .map(|_| Value::Unit)
        .map_err(|message| tcp_error(context, message))
}

fn type_error(context: &mut dyn NativeContext, message: &str) -> Value {
    context.typed_error(&["error", "type_error"], message.to_owned())
}

fn value_error(context: &mut dyn NativeContext, message: &str) -> Value {
    context.typed_error(&["error", "value_error"], message.to_owned())
}

fn tcp_error(context: &mut dyn NativeContext, message: String) -> Value {
    context.typed_error(&["error", "tcp_error"], message)
}
