use std::sync::Arc;

use rpds::ListSync;

use super::{BlockId, FunctionId, NamespaceId, NativeFunctionId, SymbolId};

#[derive(Clone, Debug)]
pub enum Value {
    Unit,
    Boolean(bool),
    Integer(i64),
    Float(f64),
    String(Arc<str>),
    Symbol(SymbolId),
    List(PersistentList),
    Function(FunctionId),
    NativeFunction(NativeFunctionId),
    Block(BlockId),
    Namespace(NamespaceId),
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Unit, Self::Unit) => true,
            (Self::Boolean(a), Self::Boolean(b)) => a == b,
            (Self::Integer(a), Self::Integer(b)) => a == b,
            (Self::Integer(a), Self::Float(b)) => {
                let af = *a as f64;
                af.is_finite() && (af - b).abs() < f64::EPSILON
            }
            (Self::Float(a), Self::Integer(b)) => {
                let bf = *b as f64;
                a.is_finite() && (a - bf).abs() < f64::EPSILON
            }
            (Self::Float(a), Self::Float(b)) => a == b,
            (Self::String(a), Self::String(b)) => a == b,
            (Self::Symbol(a), Self::Symbol(b)) => a == b,
            (Self::List(a), Self::List(b)) => {
                let av: Vec<_> = a.iter().collect();
                let bv: Vec<_> = b.iter().collect();
                av.len() == bv.len() && av.iter().zip(bv.iter()).all(|(x, y)| x == y)
            }
            (Self::Function(a), Self::Function(b)) => a == b,
            (Self::NativeFunction(a), Self::NativeFunction(b)) => a == b,
            (Self::Block(a), Self::Block(b)) => a == b,
            (Self::Namespace(a), Self::Namespace(b)) => a == b,
            _ => false,
        }
    }
}

#[derive(Clone, Debug)]
pub struct PersistentList(ListSync<Value>);

impl Default for PersistentList {
    fn default() -> Self {
        Self(ListSync::new_sync())
    }
}

impl PersistentList {
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn first(&self) -> Option<&Value> {
        self.0.first()
    }

    pub fn rest(&self) -> Option<Self> {
        self.0.drop_first().map(Self)
    }

    pub fn push_front(&self, value: Value) -> Self {
        Self(self.0.push_front(value))
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &Value> {
        self.0.iter()
    }

    /// Collect elements into a Vec. O(n) — use for testing/inspection.
    pub fn to_vec(&self) -> Vec<Value> {
        self.0.iter().cloned().collect()
    }
}

impl FromIterator<Value> for PersistentList {
    fn from_iter<T: IntoIterator<Item = Value>>(values: T) -> Self {
        let values: Vec<_> = values.into_iter().collect();
        values
            .into_iter()
            .rev()
            .fold(Self::empty(), |list, value| list.push_front(value))
    }
}
