use std::sync::Arc;

use dumpster::{TraceWith, Visitor, unsync::Gc};

pub type BlockRef = Gc<StoredBlock>;

/// A compiled block's stable identity and linked VM entry point.
#[derive(Clone, Debug)]
pub struct StoredBlock {
    pub module_index: usize,
    pub block_id: crate::syntax::ast::BlockId,
    pub(crate) vm_program: u64,
    pub(crate) vm_function: u16,
    pub(crate) vm_context: Vec<Arc<str>>,
}

unsafe impl<V: Visitor> TraceWith<V> for StoredBlock {
    fn accept(&self, _visitor: &mut V) -> Result<(), ()> {
        Ok(())
    }
}
