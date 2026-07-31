use indexmap::IndexMap;

use std::cell::RefCell;

use dumpster::{TraceWith, Visitor, unsync::Gc};

use super::{Binding, SymbolId};

pub type EnvironmentRef = Gc<RefCell<Environment>>;

#[derive(Debug, Default)]
pub struct Environment {
    pub bindings: IndexMap<SymbolId, Binding>,
}

impl Environment {
    pub fn new() -> Self {
        Self::default()
    }
}

unsafe impl<V: Visitor> TraceWith<V> for Environment {
    fn accept(&self, visitor: &mut V) -> Result<(), ()> {
        for binding in self.bindings.values() {
            binding.accept(visitor)?;
        }
        Ok(())
    }
}
