use std::sync::Arc;

use super::{
    Binding, BindingMutability, BindingVisibility, Environment, Namespace, SymbolInterner, Value,
};

pub fn create_typed_error(symbols: &mut SymbolInterner, types: &[&str], message: String) -> Value {
    let mut environment = Environment::new(None);
    let type_symbols = types
        .iter()
        .map(|name| symbols.intern(name))
        .collect::<Vec<_>>();
    environment.bindings.insert(
        symbols.intern("types"),
        Binding {
            value: Value::List(type_symbols.iter().copied().map(Value::Symbol).collect()),
            mutability: BindingMutability::Immutable,
            visibility: BindingVisibility::Public,
        },
    );
    environment.bindings.insert(
        symbols.intern("message"),
        Binding {
            value: Value::String(Arc::from(message)),
            mutability: BindingMutability::Immutable,
            visibility: BindingVisibility::Public,
        },
    );
    Value::Namespace(dumpster::unsync::Gc::new(Namespace {
        environment: dumpster::unsync::Gc::new(std::cell::RefCell::new(environment)),
        types: type_symbols,
        error_metadata: std::cell::RefCell::new(None),
    }))
}
