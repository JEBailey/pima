use std::{cmp::Ordering, sync::Arc};

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
    RemoteNamespace(super::RemoteNamespaceHandle),
    RemoteFunction(super::RemoteNamespaceHandle, Arc<str>),
    Task(super::TaskHandle),
    TaskFunction(super::TaskHandle, Arc<str>),
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
            Self::NativeFunction(_)
            | Self::VmClosure(_)
            | Self::VmPartial(_)
            | Self::RemoteFunction(_, _)
            | Self::TaskFunction(_, _) => "function",
            Self::Placeholder => "placeholder",
            Self::VmBinding(cell) => cell
                .current_value()
                .as_ref()
                .map_or("unit", Value::type_name),
            Self::Block(_) => "block",
            Self::Namespace(_) => "object",
            Self::TcpListener(_) => "tcp_listener",
            Self::TcpConnection(_) => "tcp_connection",
            Self::RemoteNamespace(_) => "remote",
            Self::Task(_) => "future",
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
        (Value::Integer(_) | Value::Float(_), Value::Integer(_) | Value::Float(_)) => {
            numeric_compare(left, right) == Some(Ordering::Equal)
        }
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
        (Value::RemoteNamespace(a), Value::RemoteNamespace(b)) => a == b,
        (Value::RemoteFunction(a_handle, a_name), Value::RemoteFunction(b_handle, b_name)) => {
            a_handle == b_handle && a_name == b_name
        }
        (Value::Task(a), Value::Task(b)) => a == b,
        (Value::TaskFunction(a_handle, a_name), Value::TaskFunction(b_handle, b_name)) => {
            a_handle == b_handle && a_name == b_name
        }
        _ => false,
    }
}

/// Compares numeric values without rounding an integer through `f64`.
///
/// Returns `None` for non-numeric values and unordered comparisons involving
/// NaN, matching IEEE 754 comparison behavior.
pub(crate) fn numeric_compare(left: &Value, right: &Value) -> Option<Ordering> {
    match (left, right) {
        (Value::Integer(left), Value::Integer(right)) => Some(left.cmp(right)),
        (Value::Float(left), Value::Float(right)) => left.partial_cmp(right),
        (Value::Integer(integer), Value::Float(float)) => compare_integer_float(*integer, *float),
        (Value::Float(float), Value::Integer(integer)) => {
            compare_integer_float(*integer, *float).map(Ordering::reverse)
        }
        _ => None,
    }
}

fn compare_integer_float(integer: i64, float: f64) -> Option<Ordering> {
    if float.is_nan() {
        return None;
    }

    // These boundaries are exactly representable as f64. `i64::MAX as f64`
    // rounds to 2^63, which is one greater than the largest integer.
    const I64_MIN_AS_F64: f64 = -9_223_372_036_854_775_808.0;
    const I64_UPPER_BOUND_AS_F64: f64 = 9_223_372_036_854_775_808.0;
    if float < I64_MIN_AS_F64 {
        return Some(Ordering::Greater);
    }
    if float >= I64_UPPER_BOUND_AS_F64 {
        return Some(Ordering::Less);
    }

    let truncated = float as i64;
    match integer.cmp(&truncated) {
        Ordering::Equal => {
            // The cast truncates toward zero. When a fractional part remains,
            // its sign determines which side of the truncated integer it lies.
            if float > truncated as f64 {
                Some(Ordering::Less)
            } else if float < truncated as f64 {
                Some(Ordering::Greater)
            } else {
                Some(Ordering::Equal)
            }
        }
        ordering => Some(ordering),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_numeric_ordering_handles_boundaries_and_special_floats() {
        assert_eq!(
            numeric_compare(
                &Value::Integer(i64::MAX),
                &Value::Float(9_223_372_036_854_775_808.0)
            ),
            Some(Ordering::Less)
        );
        assert_eq!(
            numeric_compare(
                &Value::Integer(i64::MIN),
                &Value::Float(-9_223_372_036_854_775_808.0)
            ),
            Some(Ordering::Equal)
        );
        assert_eq!(
            numeric_compare(&Value::Integer(0), &Value::Float(f64::INFINITY)),
            Some(Ordering::Less)
        );
        assert_eq!(
            numeric_compare(&Value::Integer(0), &Value::Float(f64::NEG_INFINITY)),
            Some(Ordering::Greater)
        );
        assert_eq!(
            numeric_compare(&Value::Integer(0), &Value::Float(f64::NAN)),
            None
        );
    }

    #[test]
    fn equality_uses_the_same_exact_numeric_ordering() {
        assert!(language_equal(
            &Value::Integer(9_007_199_254_740_992),
            &Value::Float(9_007_199_254_740_992.0)
        ));
        assert!(!language_equal(
            &Value::Integer(9_007_199_254_740_993),
            &Value::Float(9_007_199_254_740_992.0)
        ));
        assert!(!language_equal(
            &Value::Float(f64::NAN),
            &Value::Float(f64::NAN)
        ));
    }
}
