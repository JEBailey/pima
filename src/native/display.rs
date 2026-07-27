use crate::runtime::Value;

use super::NativeContext;

/// Formats a value using Pima's human-readable display representation.
pub(crate) fn value(input: &Value, context: &mut dyn NativeContext) -> String {
    match input {
        Value::Unit => "unit".to_owned(),
        Value::Boolean(boolean) => boolean.to_string(),
        Value::Integer(integer) => integer.to_string(),
        Value::Float(float) if float.fract() == 0.0 && float.is_finite() => {
            format!("{float:.1}")
        }
        Value::Float(float) => float.to_string(),
        Value::String(string) => string.to_string(),
        Value::Symbol(symbol) => {
            let name = context.resolve_symbol(*symbol).unwrap_or("?");
            format!(":{name}")
        }
        Value::List(list) => {
            let elements = list
                .iter()
                .map(|element| value(element, context))
                .collect::<Vec<_>>();
            format!("({})", elements.join(" "))
        }
        Value::Function(_) => "#<function>".to_owned(),
        Value::NativeFunction(_) => "#<native>".to_owned(),
        Value::Block(_) => "#<block>".to_owned(),
        Value::Namespace(_) => "#<namespace>".to_owned(),
    }
}
