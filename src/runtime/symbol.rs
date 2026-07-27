use std::{collections::HashMap, sync::Arc};

use super::SymbolId;

#[derive(Default, Debug)]
pub struct SymbolInterner {
    names: Vec<Arc<str>>,
    ids: HashMap<Arc<str>, SymbolId>,
}

impl SymbolInterner {
    pub fn intern(&mut self, name: &str) -> SymbolId {
        if let Some(id) = self.ids.get(name) {
            return *id;
        }
        let name: Arc<str> = Arc::from(name);
        let id = SymbolId(self.names.len() as u32);
        self.names.push(name.clone());
        self.ids.insert(name, id);
        id
    }

    pub fn resolve(&self, id: SymbolId) -> Option<&str> {
        self.names.get(id.0 as usize).map(AsRef::as_ref)
    }
}
