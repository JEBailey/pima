use crate::runtime::Value;

use super::{NativeContext, NativeDefinition, NativeRegistry, NativeResult};

pub fn register(registry: &mut NativeRegistry) {
    registry.register(NativeDefinition {
        name: "println",
        arity: super::Arity::AtLeast(0),
        call: native_println,
    });
}

fn native_println(ctx: &mut dyn NativeContext, args: &[Value]) -> NativeResult {
    for arg in args {
        print!("{}", super::display::value(arg, ctx));
        print!(" ");
    }
    println!();
    Ok(Value::Unit)
}
