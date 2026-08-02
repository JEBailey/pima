use dumpster::{TraceWith, Visitor, unsync::Gc};

use super::Value;

#[derive(Clone, Debug)]
pub struct VmClosure {
    pub(crate) program: u64,
    pub(crate) function: u16,
    pub(crate) captures: Vec<VmValue>,
    pub(crate) construction_capture: Option<usize>,
}

pub type VmClosureRef = Gc<VmClosure>;

#[derive(Clone, Debug)]
pub struct VmPartial {
    pub(crate) closure: VmClosureRef,
    pub(crate) arguments: Vec<Option<Value>>,
}

pub type VmPartialRef = Gc<VmPartial>;

#[derive(Clone, Debug)]
pub(crate) enum VmValue {
    Uninitialized,
    Value(Value),
    Cell(Gc<VmCell>),
}

#[derive(Debug)]
#[doc(hidden)]
pub struct VmCell {
    pub(crate) value: std::cell::RefCell<VmValue>,
    pub(crate) mutable: std::cell::Cell<Option<bool>>,
    pub(crate) fallback: Option<VmValue>,
    pub(crate) construction_ready: std::cell::Cell<Option<bool>>,
}

impl VmCell {
    pub(crate) fn binding(fallback: Option<VmValue>) -> Self {
        LIVE_CELL_COUNT.with(|count| count.set(count.get() + 1));
        Self {
            value: std::cell::RefCell::new(VmValue::Uninitialized),
            mutable: std::cell::Cell::new(None),
            fallback,
            construction_ready: std::cell::Cell::new(None),
        }
    }

    pub(crate) fn current_value(&self) -> Option<Value> {
        match &*self.value.borrow() {
            VmValue::Value(value) => Some(value.resolved()),
            VmValue::Cell(cell) => cell.current_value(),
            VmValue::Uninitialized => match &self.fallback {
                Some(VmValue::Value(value)) => Some(value.resolved()),
                Some(VmValue::Cell(cell)) => cell.current_value(),
                _ => None,
            },
        }
    }

    pub(crate) fn contains_reference_like_value(&self) -> bool {
        match &*self.value.borrow() {
            VmValue::Value(Value::VmBinding(cell)) | VmValue::Cell(cell) => {
                cell.contains_reference_like_value()
            }
            VmValue::Value(value) => value.is_reference_like(),
            VmValue::Uninitialized => false,
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

unsafe impl<V: Visitor> TraceWith<V> for VmPartial {
    fn accept(&self, visitor: &mut V) -> Result<(), ()> {
        self.closure.accept(visitor)?;
        self.arguments.accept(visitor)
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
