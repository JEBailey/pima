use indexmap::IndexMap;

use std::cell::RefCell;

use dumpster::{TraceWith, Visitor, unsync::Gc};

use super::{Binding, SymbolId};

pub type EnvironmentRef = Gc<RefCell<Environment>>;

#[cfg(test)]
thread_local! {
    static LIVE_ENVIRONMENTS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[derive(Debug)]
pub struct Environment {
    pub parent: Option<EnvironmentRef>,
    pub bindings: IndexMap<SymbolId, Binding>,
}

impl Environment {
    pub fn new(parent: Option<EnvironmentRef>) -> Self {
        #[cfg(test)]
        LIVE_ENVIRONMENTS.with(|count| count.set(count.get() + 1));
        Self {
            parent,
            bindings: IndexMap::new(),
        }
    }
}

#[cfg(test)]
impl Drop for Environment {
    fn drop(&mut self) {
        LIVE_ENVIRONMENTS.with(|count| count.set(count.get() - 1));
    }
}

#[cfg(test)]
pub(crate) fn live_environment_count() -> usize {
    LIVE_ENVIRONMENTS.with(std::cell::Cell::get)
}

unsafe impl<V: Visitor> TraceWith<V> for Environment {
    fn accept(&self, visitor: &mut V) -> Result<(), ()> {
        self.parent.accept(visitor)?;
        for binding in self.bindings.values() {
            binding.accept(visitor)?;
        }
        Ok(())
    }
}
