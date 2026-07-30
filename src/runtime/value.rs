use std::sync::Arc;

use rpds::ListSync;

use super::{
    BlockId, FunctionId, NamespaceId, NativeFunctionId, SymbolId, TcpConnectionId, TcpListenerId,
};

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
            Self::Function(_) | Self::NativeFunction(_) => "function",
            Self::Block(_) => "block",
            Self::Namespace(_) => "namespace",
            Self::TcpListener(_) => "tcp_listener",
            Self::TcpConnection(_) => "tcp_connection",
        }
    }

    pub(crate) fn type_symbol(&self) -> String {
        format!(":{}", self.type_name())
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
        (Value::Function(a), Value::Function(b)) => a == b,
        (Value::NativeFunction(a), Value::NativeFunction(b)) => a == b,
        (Value::Block(a), Value::Block(b)) => a == b,
        (Value::Namespace(a), Value::Namespace(b)) => a == b,
        (Value::TcpListener(a), Value::TcpListener(b)) => a == b,
        (Value::TcpConnection(a), Value::TcpConnection(b)) => a == b,
        _ => false,
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
