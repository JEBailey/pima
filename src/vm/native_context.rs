use crate::{
    native::{NativeContext, NativeResult},
    runtime::{SymbolId, SymbolInterner, Value},
};

#[derive(Debug)]
pub(crate) struct VmNativeContext {
    symbols: SymbolInterner,
    host: crate::native::host::HostResources,
    active_span: Option<crate::source::Span>,
    stack: Vec<crate::diagnostic::StackFrame>,
}

impl Default for VmNativeContext {
    fn default() -> Self {
        Self {
            symbols: SymbolInterner::default(),
            host: crate::native::host::HostResources::new(
                std::env::current_dir().unwrap_or_else(|_| ".".into()),
            ),
            active_span: None,
            stack: Vec::new(),
        }
    }
}

impl VmNativeContext {
    pub(crate) fn new(working_directory: std::path::PathBuf) -> Self {
        Self {
            symbols: SymbolInterner::default(),
            host: crate::native::host::HostResources::new(working_directory),
            active_span: None,
            stack: Vec::new(),
        }
    }
}

impl VmNativeContext {
    pub(crate) fn set_execution_metadata(
        &mut self,
        span: Option<crate::source::Span>,
        stack: Vec<crate::diagnostic::StackFrame>,
    ) {
        self.active_span = span;
        self.stack = stack;
    }

    pub(crate) fn attach_error_metadata(&self, value: &Value) {
        let (Some(origin), Value::Namespace(namespace)) = (self.active_span, value) else {
            return;
        };
        if namespace.error_metadata.borrow().is_none() {
            *namespace.error_metadata.borrow_mut() = Some(crate::runtime::ErrorMetadata {
                origin,
                stack: self.stack.clone(),
            });
        }
    }
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

    pub(crate) fn make_native_namespace(
        &mut self,
        bindings: Vec<(std::sync::Arc<str>, bool, Value)>,
    ) -> Value {
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
        Value::Namespace(dumpster::unsync::Gc::new(crate::runtime::Namespace {
            environment: dumpster::unsync::Gc::new(std::cell::RefCell::new(environment)),
            types: Vec::new(),
            error_metadata: std::cell::RefCell::new(None),
        }))
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
        self.attach_error_metadata(&value);
        value
    }
}

impl NativeContext for VmNativeContext {
    fn typed_error(&mut self, types: &[&str], message: String) -> Value {
        let value = crate::runtime::create_typed_error(&mut self.symbols, types, message);
        self.attach_error_metadata(&value);
        value
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
        self.host.working_directory()
    }
    fn tcp_listen(
        &mut self,
        address: &str,
        port: u16,
    ) -> Result<crate::runtime::TcpListenerId, String> {
        self.host.listen(address, port)
    }
    fn tcp_accept(
        &mut self,
        listener: crate::runtime::TcpListenerId,
    ) -> Result<crate::runtime::TcpConnectionId, String> {
        self.host.accept(listener)
    }
    fn tcp_read(
        &mut self,
        connection: crate::runtime::TcpConnectionId,
        maximum: usize,
    ) -> Result<String, String> {
        self.host.read(connection, maximum)
    }
    fn tcp_write(
        &mut self,
        connection: crate::runtime::TcpConnectionId,
        text: &str,
    ) -> Result<(), String> {
        self.host.write(connection, text)
    }
    fn tcp_set_timeout(
        &mut self,
        connection: crate::runtime::TcpConnectionId,
        milliseconds: u64,
    ) -> Result<(), String> {
        self.host.set_timeout(connection, milliseconds)
    }
    fn tcp_close_listener(
        &mut self,
        listener: crate::runtime::TcpListenerId,
    ) -> Result<(), String> {
        self.host.close_listener(listener)
    }
    fn tcp_close_connection(
        &mut self,
        connection: crate::runtime::TcpConnectionId,
    ) -> Result<(), String> {
        self.host.close_connection(connection)
    }
}
