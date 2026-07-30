use dumpster::{TraceWith, Visitor, unsync::Gc};

use super::{EnvironmentRef, SymbolId};

pub type NamespaceRef = Gc<Namespace>;

#[derive(Clone, Debug)]
pub struct Namespace {
    pub environment: EnvironmentRef,
    pub types: Vec<SymbolId>,
    pub error_metadata: std::cell::RefCell<Option<super::ErrorMetadata>>,
}

unsafe impl<V: Visitor> TraceWith<V> for Namespace {
    fn accept(&self, visitor: &mut V) -> Result<(), ()> {
        self.environment.accept(visitor)
    }
}
