use indexmap::IndexMap;

use super::{Binding, EnvironmentId, SymbolId};

#[derive(Debug)]
pub struct Environment {
    pub parent: Option<EnvironmentId>,
    pub bindings: IndexMap<SymbolId, Binding>,
}

impl Environment {
    pub fn new(parent: Option<EnvironmentId>) -> Self {
        Self {
            parent,
            bindings: IndexMap::new(),
        }
    }
}
