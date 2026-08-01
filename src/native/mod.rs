pub mod console;
mod display;
pub(crate) mod host;
pub mod io;
pub mod lists;
pub mod numbers;
pub mod registry;
pub mod remote;
pub mod strings;
pub mod tcp;
pub mod types;

pub use registry::{
    Arity, NativeCall, NativeContext, NativeDefinition, NativeRegistry, NativeResult,
};

pub(crate) fn register_core(registry: &mut NativeRegistry) {
    numbers::register(registry);
    strings::register(registry);
    lists::register(registry);
    types::register(registry);
    console::register(registry);
}

pub(crate) fn core_namespace(name: &str) -> Option<&'static str> {
    match name {
        "+" | "-" | "*" | "/" | "<" | ">" | "=" => None,
        "div" | "mod" | "int" => Some("Math"),
        "concat" | "length" | "byte_length" | "slice" | "chars" | "code_point"
        | "from_code_point" | "string" | "lower" | "upper" | "trim" | "contains?"
        | "starts_with?" | "ends_with?" | "replace" | "split" | "join" => Some("String"),
        "push" | "append" | "head" | "rest" | "empty?" => Some("List"),
        "types" | "is?" => Some("Types"),
        "println" => Some("Console"),
        "not" => Some("Logic"),
        _ => None,
    }
}
