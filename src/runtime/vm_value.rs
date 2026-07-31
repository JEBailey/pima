use dumpster::{TraceWith, Visitor, unsync::Gc};

use super::Value;

#[derive(Clone, Debug)]
pub struct VmClosure {
    pub(crate) program: u64,
    pub(crate) function: u16,
    pub(crate) captures: Vec<VmValue>,
}

pub type VmClosureRef = Gc<VmClosure>;

#[derive(Clone, Debug)]
pub(crate) enum VmValue {
    Uninitialized,
    Value(Value),
    Cell(Gc<VmCell>),
}

#[derive(Debug)]
pub(crate) struct VmCell {
    pub(crate) value: std::cell::RefCell<VmValue>,
    pub(crate) mutable: std::cell::Cell<Option<bool>>,
    pub(crate) fallback: Option<VmValue>,
}

impl VmCell {
    pub(crate) fn binding(fallback: Option<VmValue>) -> Self {
        LIVE_CELL_COUNT.with(|count| count.set(count.get() + 1));
        Self {
            value: std::cell::RefCell::new(VmValue::Uninitialized),
            mutable: std::cell::Cell::new(None),
            fallback,
        }
    }
}

impl Drop for VmCell {
    fn drop(&mut self) {
        LIVE_CELL_COUNT.with(|count| count.set(count.get() - 1));
    }
}

thread_local! {
    static LIVE_CELL_COUNT: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

pub fn live_vm_cell_count() -> usize {
    LIVE_CELL_COUNT.with(std::cell::Cell::get)
}

unsafe impl<V: Visitor> TraceWith<V> for VmClosure {
    fn accept(&self, visitor: &mut V) -> Result<(), ()> {
        self.captures.accept(visitor)
    }
}

unsafe impl<V: Visitor> TraceWith<V> for VmCell {
    fn accept(&self, visitor: &mut V) -> Result<(), ()> {
        self.value
            .borrow()
            .accept(visitor)
            .and_then(|_| self.fallback.accept(visitor))
    }
}

unsafe impl<V: Visitor> TraceWith<V> for VmValue {
    fn accept(&self, visitor: &mut V) -> Result<(), ()> {
        match self {
            Self::Uninitialized => Ok(()),
            Self::Value(value) => value.accept(visitor),
            Self::Cell(cell) => cell.accept(visitor),
        }
    }
}
