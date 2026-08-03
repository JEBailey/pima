use std::sync::Arc;

use crate::runtime::{PersistentList, SymbolId, TaskHandle, Value};

use super::RemoteNamespaceHandle;

/// VM-independent representation permitted across an isolated worker boundary.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum TransportValue {
    Unit,
    Boolean(bool),
    Integer(i64),
    Float(f64),
    String(Arc<str>),
    Symbol(Arc<str>),
    List(Vec<TransportValue>),
    RemoteNamespace(RemoteNamespaceHandle),
    Task(TaskHandle),
    TcpListener(crate::runtime::TcpListenerId),
}

impl TransportValue {
    pub(crate) fn from_value(
        value: &Value,
        resolve_symbol: impl Fn(SymbolId) -> Option<Arc<str>> + Copy,
    ) -> Result<Self, &'static str> {
        match value.resolved() {
            Value::Unit => Ok(Self::Unit),
            Value::Boolean(value) => Ok(Self::Boolean(value)),
            Value::Integer(value) => Ok(Self::Integer(value)),
            Value::Float(value) => Ok(Self::Float(value)),
            Value::String(value) => Ok(Self::String(value)),
            Value::Symbol(value) => resolve_symbol(value)
                .map(Self::Symbol)
                .ok_or("symbol is not interned in the sending VM"),
            Value::List(values) => values
                .iter()
                .map(|value| Self::from_value(value, resolve_symbol))
                .collect::<Result<Vec<_>, _>>()
                .map(Self::List),
            Value::RemoteNamespace(handle) => Ok(Self::RemoteNamespace(handle)),
            Value::Task(handle) => Ok(Self::Task(handle)),
            Value::VmClosure(_) | Value::VmPartial(_) | Value::NativeFunction(_) => {
                Err("local functions are VM-bound and cannot cross a worker boundary")
            }
            Value::Block(_) => {
                Err("local code blocks are VM-bound and cannot cross a worker boundary")
            }
            Value::Namespace(_) => {
                Err("local objects are VM-bound and must be constructed inside the worker")
            }
            Value::VmBinding(_) => {
                Err("local binding cells are VM-bound and cannot cross a worker boundary")
            }
            Value::RemoteFunction(_, _) | Value::BoundRemoteFunction(_, _, _) => {
                Err("bound remote functions cannot cross a worker boundary")
            }
            Value::TaskFunction(_, _) => Err("task functions cannot cross worker boundaries"),
            Value::TcpListener(_) => Err("TCP listeners require explicit `&` sharing"),
            Value::TcpConnection(_) => Err("TCP connections cannot cross a worker boundary"),
            Value::Placeholder => Err("placeholder values cannot cross a worker boundary"),
        }
    }

    pub(crate) fn into_value(self, mut intern_symbol: impl FnMut(&str) -> SymbolId) -> Value {
        self.into_value_with(&mut intern_symbol)
    }

    fn into_value_with(self, intern_symbol: &mut dyn FnMut(&str) -> SymbolId) -> Value {
        match self {
            Self::Unit => Value::Unit,
            Self::Boolean(value) => Value::Boolean(value),
            Self::Integer(value) => Value::Integer(value),
            Self::Float(value) => Value::Float(value),
            Self::String(value) => Value::String(value),
            Self::Symbol(value) => Value::Symbol(intern_symbol(&value)),
            Self::List(values) => Value::List(
                values
                    .into_iter()
                    .map(|value| value.into_value_with(intern_symbol))
                    .collect::<PersistentList>(),
            ),
            Self::RemoteNamespace(handle) => Value::RemoteNamespace(handle),
            Self::Task(handle) => Value::Task(handle),
            Self::TcpListener(handle) => Value::TcpListener(handle),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TransportError {
    pub(crate) types: Vec<Arc<str>>,
    pub(crate) message: Arc<str>,
}
