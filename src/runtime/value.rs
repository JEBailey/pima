use std::sync::Arc;

use dumpster::{TraceWith, Visitor, unsync::Gc};

use super::{NamespaceRef, NativeFunctionId, SymbolId, TcpConnectionId, TcpListenerId};

#[derive(Clone, Debug)]
pub enum Value {
    Unit,
    Boolean(bool),
    Integer(i64),
    Float(f64),
    String(Arc<str>),
    Symbol(SymbolId),
    List(PersistentList),
    NativeFunction(NativeFunctionId),
    VmClosure(super::VmClosureRef),
    VmPartial(super::VmPartialRef),
    #[doc(hidden)]
    VmBinding(Gc<super::VmCell>),
    #[doc(hidden)]
    Placeholder,
    Block(super::BlockRef),
    Namespace(NamespaceRef),
    TcpListener(TcpListenerId),
    TcpConnection(TcpConnectionId),
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        language_equal(self, other)
    }
}

impl Value {
    pub(crate) fn type_name(&self) -> &'static str {
        match self {
            Self::Unit => "unit",
            Self::Boolean(_) => "boolean",
            Self::Integer(_) => "integer",
            Self::Float(_) => "float",
            Self::String(_) => "string",
            Self::Symbol(_) => "symbol",
            Self::List(_) => "list",
            Self::NativeFunction(_) | Self::VmClosure(_) | Self::VmPartial(_) => "function",
            Self::Placeholder => "placeholder",
            Self::VmBinding(cell) => cell
                .current_value()
                .as_ref()
                .map_or("unit", Value::type_name),
            Self::Block(_) => "block",
            Self::Namespace(_) => "namespace",
            Self::TcpListener(_) => "tcp_listener",
            Self::TcpConnection(_) => "tcp_connection",
        }
    }

    pub(crate) fn type_symbol(&self) -> String {
        format!(":{}", self.type_name())
    }

    pub(crate) fn resolved(&self) -> Value {
        match self {
            Self::VmBinding(cell) => cell.current_value().unwrap_or(Value::Unit),
            value => value.clone(),
        }
    }
}

pub(crate) fn language_equal(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::Unit, Value::Unit) => true,
        (Value::Boolean(a), Value::Boolean(b)) => a == b,
        (Value::Integer(a), Value::Integer(b)) => a == b,
        (Value::Integer(integer), Value::Float(float))
        | (Value::Float(float), Value::Integer(integer)) => {
            float.fract() == 0.0
                && *float >= i64::MIN as f64
                && *float < -(i64::MIN as f64)
                && (*float as i64) == *integer
        }
        (Value::Float(a), Value::Float(b)) => a == b,
        (Value::String(a), Value::String(b)) => a == b,
        (Value::Symbol(a), Value::Symbol(b)) => a == b,
        (Value::List(a), Value::List(b)) => {
            let av: Vec<_> = a.iter().collect();
            let bv: Vec<_> = b.iter().collect();
            av.len() == bv.len() && av.iter().zip(bv.iter()).all(|(x, y)| x == y)
        }
        (Value::NativeFunction(a), Value::NativeFunction(b)) => a == b,
        (Value::VmClosure(a), Value::VmClosure(b)) => Gc::ptr_eq(a, b),
        (Value::VmPartial(a), Value::VmPartial(b)) => Gc::ptr_eq(a, b),
        (Value::VmBinding(a), Value::VmBinding(b)) => Gc::ptr_eq(a, b),
        (Value::Placeholder, Value::Placeholder) => true,
        (Value::Block(a), Value::Block(b)) => Gc::ptr_eq(a, b),
        (Value::Namespace(a), Value::Namespace(b)) => Gc::ptr_eq(a, b),
        (Value::TcpListener(a), Value::TcpListener(b)) => a == b,
        (Value::TcpConnection(a), Value::TcpConnection(b)) => a == b,
        _ => false,
    }
}

unsafe impl<V: Visitor> TraceWith<V> for Value {
    fn accept(&self, visitor: &mut V) -> Result<(), ()> {
        match self {
            Self::List(list) => {
                for value in list.iter() {
                    value.accept(visitor)?;
                }
            }
            Self::VmClosure(function) => function.accept(visitor)?,
            Self::VmPartial(function) => function.accept(visitor)?,
            Self::VmBinding(cell) => cell.accept(visitor)?,
            Self::Block(block) => block.accept(visitor)?,
            Self::Namespace(namespace) => namespace.accept(visitor)?,
            _ => {}
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Default)]
pub struct PersistentList(Vec<Value>);

impl PersistentList {
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn first(&self) -> Option<&Value> {
        self.0.first()
    }

    pub fn rest(&self) -> Option<Self> {
        (!self.0.is_empty()).then(|| Self(self.0[1..].to_vec()))
    }

    pub fn push_front(&self, value: Value) -> Self {
        let mut values = Vec::with_capacity(self.0.len() + 1);
        values.push(value);
        values.extend(self.0.iter().cloned());
        Self(values)
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
        self.0.to_vec()
    }
}

impl FromIterator<Value> for PersistentList {
    fn from_iter<T: IntoIterator<Item = Value>>(values: T) -> Self {
        Self(values.into_iter().collect())
    }
}
