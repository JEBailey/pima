use super::{EnvironmentId, SymbolId, Value};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BindingMutability {
    Immutable,
    Mutable,
    ImportedReadOnly {
        environment: EnvironmentId,
        symbol: SymbolId,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BindingVisibility {
    Private,
    Public,
}

#[derive(Clone, Debug)]
pub struct Binding {
    pub value: Value,
    pub mutability: BindingMutability,
    pub visibility: BindingVisibility,
}
