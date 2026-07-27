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
        print!("{}", value_display(arg, ctx));
        print!(" ");
    }
    println!();
    Ok(Value::Unit)
}

fn value_display(value: &Value, ctx: &mut dyn NativeContext) -> String {
    match value {
        Value::Unit => "unit".to_string(),
        Value::Boolean(b) => b.to_string(),
        Value::Integer(n) => n.to_string(),
        Value::Float(f) => {
            if *f == (f.round()) && f.is_finite() && f.abs() < 1e15 {
                format!("{:.1}", f)
            } else {
                format!("{}", f)
            }
        }
        Value::String(s) => s.to_string(),
        Value::Symbol(id) => {
            let name = ctx.resolve_symbol(*id).unwrap_or("?");
            format!(":{name}")
        }
        Value::List(list) => {
            let elems: Vec<String> = list.iter().map(|v| value_display(v, ctx)).collect();
            format!("({})", elems.join(" "))
        }
        Value::Function(_) => "#<function>".to_string(),
        Value::NativeFunction(_) => "#<native>".to_string(),
        Value::Block(_) => "#<block>".to_string(),
        Value::Namespace(_) => "#<namespace>".to_string(),
    }
}
