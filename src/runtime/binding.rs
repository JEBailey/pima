use dumpster::{TraceWith, Visitor};

use super::{EnvironmentRef, SymbolId, Value};

#[derive(Clone, Debug)]
pub enum BindingMutability {
    Immutable,
    Mutable,
    ImportedReadOnly {
        environment: EnvironmentRef,
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

unsafe impl<V: Visitor> TraceWith<V> for BindingMutability {
    fn accept(&self, visitor: &mut V) -> Result<(), ()> {
        if let Self::ImportedReadOnly { environment, .. } = self {
            environment.accept(visitor)?;
        }
        Ok(())
    }
}

unsafe impl<V: Visitor> TraceWith<V> for Binding {
    fn accept(&self, visitor: &mut V) -> Result<(), ()> {
        self.value.accept(visitor)?;
        self.mutability.accept(visitor)
    }
}
