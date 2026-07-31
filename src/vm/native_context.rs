use crate::{
    native::{NativeContext, NativeResult},
    runtime::{SymbolId, SymbolInterner, Value},
};

pub(crate) struct VmNativeContext {
    symbols: SymbolInterner,
    working_directory: std::path::PathBuf,
}

impl Default for VmNativeContext {
    fn default() -> Self {
        Self {
            symbols: SymbolInterner::default(),
            working_directory: std::env::current_dir().unwrap_or_else(|_| ".".into()),
        }
    }
}

impl VmNativeContext {
    pub(crate) fn resolve(&self, symbol: SymbolId) -> Option<&str> {
        self.symbols.resolve(symbol)
    }

    pub(crate) fn make_namespace(
        &mut self,
        bindings: Vec<(std::sync::Arc<str>, bool, Value)>,
    ) -> NativeResult {
        let mut environment = crate::runtime::Environment::new(None);
        for (name, public, value) in bindings {
            environment.bindings.insert(
                self.symbols.intern(&name),
                crate::runtime::Binding {
                    value,
                    mutability: crate::runtime::BindingMutability::Immutable,
                    visibility: if public {
                        crate::runtime::BindingVisibility::Public
                    } else {
                        crate::runtime::BindingVisibility::Private
                    },
                },
            );
        }
        let environment = dumpster::unsync::Gc::new(std::cell::RefCell::new(environment));
        let types = crate::runtime::namespace_types(&mut self.symbols, &environment)
            .map_err(|message| self.typed_error(&["error", "type_error"], message))?;
        Ok(Value::Namespace(dumpster::unsync::Gc::new(
            crate::runtime::Namespace {
                environment,
                types,
                error_metadata: std::cell::RefCell::new(None),
            },
        )))
    }

    pub(crate) fn load_member(&mut self, value: Value, name: &str) -> NativeResult {
        let Value::Namespace(namespace) = value else {
            return Err(self.typed_error(
                &["error", "type_error"],
                format!(
                    "member access `.` requires a namespace, got {}",
                    value.type_symbol()
                ),
            ));
        };
        let symbol = self.symbols.intern(name);
        let binding = namespace
            .environment
            .borrow()
            .bindings
            .get(&symbol)
            .cloned();
        let Some(binding) = binding else {
            return Err(self.typed_error(
                &["error", "name_error"],
                format!("namespace has no member `{name}`"),
            ));
        };
        if binding.visibility == crate::runtime::BindingVisibility::Private {
            return Err(self.typed_error(
                &["error", "visibility_error"],
                format!("member `{name}` is private"),
            ));
        }
        Ok(binding.value)
    }

    pub(crate) fn validate_thrown(&mut self, value: Value) -> Value {
        if let Err(message) = crate::runtime::throwable_error(&mut self.symbols, &value) {
            return self.typed_error(&["error", "type_error"], message);
        }
        value
    }
}

impl NativeContext for VmNativeContext {
    fn typed_error(&mut self, types: &[&str], message: String) -> Value {
        crate::runtime::create_typed_error(&mut self.symbols, types, message)
    }
    fn intern_symbol(&mut self, name: &str) -> SymbolId {
        self.symbols.intern(name)
    }
    fn resolve_symbol(&self, id: SymbolId) -> Option<&str> {
        self.symbols.resolve(id)
    }
    fn namespace_type_symbols(&self, namespace: &crate::runtime::NamespaceRef) -> Vec<SymbolId> {
        namespace.types.clone()
    }
    fn working_directory(&self) -> &std::path::Path {
        &self.working_directory
    }
    fn tcp_listen(&mut self, _: &str, _: u16) -> Result<crate::runtime::TcpListenerId, String> {
        Err("TCP is not available in the register VM yet".into())
    }
    fn tcp_accept(
        &mut self,
        _: crate::runtime::TcpListenerId,
    ) -> Result<crate::runtime::TcpConnectionId, String> {
        Err("TCP is not available in the register VM yet".into())
    }
    fn tcp_read(&mut self, _: crate::runtime::TcpConnectionId, _: usize) -> Result<String, String> {
        Err("TCP is not available in the register VM yet".into())
    }
    fn tcp_write(&mut self, _: crate::runtime::TcpConnectionId, _: &str) -> Result<(), String> {
        Err("TCP is not available in the register VM yet".into())
    }
    fn tcp_set_timeout(
        &mut self,
        _: crate::runtime::TcpConnectionId,
        _: u64,
    ) -> Result<(), String> {
        Err("TCP is not available in the register VM yet".into())
    }
    fn tcp_close_listener(&mut self, _: crate::runtime::TcpListenerId) -> Result<(), String> {
        Err("TCP is not available in the register VM yet".into())
    }
    fn tcp_close_connection(&mut self, _: crate::runtime::TcpConnectionId) -> Result<(), String> {
        Err("TCP is not available in the register VM yet".into())
    }
}
