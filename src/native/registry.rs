use crate::runtime::{NativeFunctionId, Value};

pub type NativeResult = Result<Value, Value>;
pub type NativeCall = fn(&mut dyn NativeContext, &[Value]) -> NativeResult;

pub trait NativeContext {
    fn typed_error(&mut self, types: &[&str], message: String) -> Value;
    fn intern_symbol(&mut self, name: &str) -> crate::runtime::SymbolId;
    fn resolve_symbol(&self, id: crate::runtime::SymbolId) -> Option<&str>;
    fn tcp_listen(
        &mut self,
        address: &str,
        port: u16,
    ) -> Result<crate::runtime::TcpListenerId, String>;
    fn tcp_accept(
        &mut self,
        listener: crate::runtime::TcpListenerId,
    ) -> Result<crate::runtime::TcpConnectionId, String>;
    fn tcp_read(
        &mut self,
        connection: crate::runtime::TcpConnectionId,
        maximum: usize,
    ) -> Result<String, String>;
    fn tcp_write(
        &mut self,
        connection: crate::runtime::TcpConnectionId,
        text: &str,
    ) -> Result<(), String>;
    fn tcp_set_timeout(
        &mut self,
        connection: crate::runtime::TcpConnectionId,
        milliseconds: u64,
    ) -> Result<(), String>;
    fn tcp_close_listener(&mut self, listener: crate::runtime::TcpListenerId)
    -> Result<(), String>;
    fn tcp_close_connection(
        &mut self,
        connection: crate::runtime::TcpConnectionId,
    ) -> Result<(), String>;
    /// Return the type symbols for an object value (without the ":object" prefix).
    /// Returns an empty list for non-object values.
    fn namespace_type_symbols(
        &self,
        namespace: &crate::runtime::NamespaceRef,
    ) -> Vec<crate::runtime::SymbolId>;
    fn working_directory(&self) -> &std::path::Path;
    fn remote_alive(&self, handle: crate::runtime::RemoteNamespaceHandle) -> Result<bool, String>;
    fn remote_stop(&self, handle: crate::runtime::RemoteNamespaceHandle) -> Result<(), String>;
    fn make_remote_namespace(
        &mut self,
        blueprint: crate::runtime::RemoteBlueprint,
        context: Vec<(
            std::sync::Arc<str>,
            crate::runtime::ContextTransferMode,
            Value,
        )>,
    ) -> NativeResult;
    fn load_remote_member(
        &mut self,
        handle: crate::runtime::RemoteNamespaceHandle,
        member: &str,
    ) -> NativeResult;
    fn call_remote_function(
        &mut self,
        handle: crate::runtime::RemoteNamespaceHandle,
        member: &str,
        argument: &Value,
    ) -> NativeResult;
    fn task_complete(&self, handle: crate::runtime::TaskHandle) -> Result<bool, String>;
    fn task_await(&mut self, handle: crate::runtime::TaskHandle) -> NativeResult;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Arity {
    Exact(usize),
    AtLeast(usize),
    Range { minimum: usize, maximum: usize },
}

#[derive(Clone)]
pub struct NativeDefinition {
    pub name: &'static str,
    pub arity: Arity,
    pub call: NativeCall,
}

impl Arity {
    pub fn check(&self, actual: usize) -> bool {
        match self {
            Self::Exact(n) => actual == *n,
            Self::AtLeast(n) => actual >= *n,
            Self::Range { minimum, maximum } => actual >= *minimum && actual <= *maximum,
        }
    }
}

impl std::fmt::Debug for NativeDefinition {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("NativeDefinition")
            .field("name", &self.name)
            .field("arity", &self.arity)
            .finish_non_exhaustive()
    }
}

#[derive(Default, Debug)]
pub struct NativeRegistry {
    definitions: Vec<NativeDefinition>,
}

impl NativeRegistry {
    pub fn register(&mut self, definition: NativeDefinition) -> NativeFunctionId {
        let id = NativeFunctionId(self.definitions.len() as u16);
        self.definitions.push(definition);
        id
    }

    pub fn get(&self, id: NativeFunctionId) -> Option<&NativeDefinition> {
        self.definitions.get(id.0 as usize)
    }

    /// Iterate over all definitions with their IDs.
    pub fn iter_with_ids(&self) -> impl Iterator<Item = (NativeFunctionId, &NativeDefinition)> {
        self.definitions
            .iter()
            .enumerate()
            .map(|(i, def)| (NativeFunctionId(i as u16), def))
    }

    pub fn find_id(&self, name: &str) -> Option<NativeFunctionId> {
        self.definitions
            .iter()
            .position(|definition| definition.name == name)
            .map(|index| NativeFunctionId(index as u16))
    }
}
