use crate::{
    native::{NativeContext, NativeResult},
    runtime::{SymbolId, SymbolInterner, Value},
};

#[derive(Debug)]
pub(crate) struct VmNativeContext {
    pub(crate) symbols: SymbolInterner,
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

    pub(crate) fn with_concurrency(
        working_directory: std::path::PathBuf,
        concurrency: std::sync::Arc<crate::runtime::ConcurrencyHub>,
        network: std::sync::Arc<std::sync::Mutex<crate::native::host::NetworkResources>>,
    ) -> Self {
        Self {
            symbols: SymbolInterner::default(),
            host: crate::native::host::HostResources::with_concurrency(
                working_directory,
                concurrency,
                network,
            ),
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

    pub(crate) fn moved_value_error(&mut self, binding: &str) -> Value {
        let origin = self.active_span;
        let location = origin.map_or_else(
            || "an unknown source location".to_owned(),
            |span| {
                format!(
                    "source {} bytes {}..{}",
                    span.source.index(),
                    span.start,
                    span.end
                )
            },
        );
        let error = self.typed_error(
            &["error", "move_error", "moved_value"],
            format!("context binding `{binding}` was moved by remote construction at {location}"),
        );
        if let (Some(span), Value::Namespace(namespace)) = (origin, &error) {
            let mut environment = namespace.environment.borrow_mut();
            for (name, value) in [
                (
                    "move_operation",
                    Value::String("remote construction".into()),
                ),
                (
                    "move_source",
                    Value::Integer(i64::from(span.source.index())),
                ),
                ("move_start", Value::Integer(span.start as i64)),
                ("move_end", Value::Integer(span.end as i64)),
            ] {
                environment.bindings.insert(
                    self.symbols.intern(name),
                    crate::runtime::Binding {
                        value,
                        mutability: crate::runtime::BindingMutability::Immutable,
                        visibility: crate::runtime::BindingVisibility::Public,
                    },
                );
            }
        }
        error
    }

    pub(crate) fn invalid_object_error(&mut self, failure: Value) -> Value {
        let failure_message = match &failure {
            Value::Namespace(namespace) => namespace
                .environment
                .borrow()
                .bindings
                .iter()
                .find(|(symbol, _)| self.symbols.resolve(**symbol) == Some("message"))
                .and_then(|(_, binding)| match &binding.value {
                    Value::String(message) => Some(message.clone()),
                    _ => None,
                }),
            _ => None,
        };
        let invalid = self.typed_error(
            &["error", "object_error", "invalid_object"],
            failure_message.map_or_else(
                || "reference belongs to an object whose construction failed".to_owned(),
                |message| {
                    format!("reference belongs to an object whose construction failed: {message}")
                },
            ),
        );
        if let Value::Namespace(namespace) = &invalid {
            namespace.environment.borrow_mut().bindings.insert(
                self.symbols.intern("construction_error"),
                crate::runtime::Binding {
                    value: failure.clone(),
                    mutability: crate::runtime::BindingMutability::Immutable,
                    visibility: crate::runtime::BindingVisibility::Public,
                },
            );
            if let Value::Namespace(failure) = &failure
                && let Some(metadata) = failure.error_metadata.borrow().clone()
            {
                *namespace.error_metadata.borrow_mut() = Some(metadata);
            }
        }
        invalid
    }
    pub(crate) fn resolve(&self, symbol: SymbolId) -> Option<&str> {
        self.symbols.resolve(symbol)
    }

    pub(crate) fn make_namespace(
        &mut self,
        bindings: Vec<(std::sync::Arc<str>, bool, bool, Value)>,
    ) -> NativeResult {
        let mut environment = crate::runtime::Environment::new();
        for (name, public, mutable, value) in bindings {
            environment.bindings.insert(
                self.symbols.intern(&name),
                crate::runtime::Binding {
                    value,
                    mutability: if mutable {
                        crate::runtime::BindingMutability::Mutable
                    } else {
                        crate::runtime::BindingMutability::Immutable
                    },
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
        bindings: Vec<(std::sync::Arc<str>, bool, bool, Value)>,
    ) -> Value {
        let mut environment = crate::runtime::Environment::new();
        for (name, public, mutable, value) in bindings {
            environment.bindings.insert(
                self.symbols.intern(&name),
                crate::runtime::Binding {
                    value,
                    mutability: if mutable {
                        crate::runtime::BindingMutability::Mutable
                    } else {
                        crate::runtime::BindingMutability::Immutable
                    },
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

    pub(crate) fn load_member(
        &mut self,
        value: Value,
        name: &str,
        allow_private: bool,
    ) -> NativeResult {
        if let Value::RemoteNamespace(handle) = value {
            return <Self as NativeContext>::load_remote_member(self, handle, name);
        }
        if let Value::Task(handle) = value {
            return match name {
                "complete?" => Ok(Value::TaskFunction(handle, std::sync::Arc::from(name))),
                _ => Err(self.typed_error(
                    &["error", "name_error"],
                    format!("task has no member `{name}`"),
                )),
            };
        }
        let Value::Namespace(namespace) = value else {
            return Err(self.typed_error(
                &["error", "type_error"],
                format!(
                    "member access `.` requires an object, got {}",
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
                format!("object has no member `{name}`"),
            ));
        };
        if binding.visibility == crate::runtime::BindingVisibility::Private && !allow_private {
            return Err(self.typed_error(
                &["error", "visibility_error"],
                format!("member `{name}` is private"),
            ));
        }
        Ok(binding.value)
    }

    pub(crate) fn store_member(
        &mut self,
        value: Value,
        name: &str,
        replacement: Value,
        allow_private: bool,
    ) -> NativeResult {
        let Value::Namespace(namespace) = value else {
            return Err(self.typed_error(
                &["error", "type_error"],
                "member assignment requires a local object".to_owned(),
            ));
        };
        let symbol = self.symbols.intern(name);
        let mut environment = namespace.environment.borrow_mut();
        let Some(binding) = environment.bindings.get_mut(&symbol) else {
            drop(environment);
            return Err(self.typed_error(
                &["error", "name_error"],
                format!("object has no member `{name}`"),
            ));
        };
        if binding.visibility == crate::runtime::BindingVisibility::Private && !allow_private {
            drop(environment);
            return Err(self.typed_error(
                &["error", "visibility_error"],
                format!("member `{name}` is private"),
            ));
        }
        if !matches!(
            binding.mutability,
            crate::runtime::BindingMutability::Mutable
        ) {
            drop(environment);
            return Err(self.typed_error(
                &["error", "mutation_error"],
                format!("cannot assign to immutable member `{name}`"),
            ));
        }
        if let Value::VmBinding(cell) = &binding.value {
            *cell.value.borrow_mut() = crate::runtime::VmValue::Value(replacement.clone());
        } else {
            binding.value = replacement.clone();
        }
        Ok(replacement)
    }

    pub(crate) fn check_member_writable(
        &mut self,
        value: Value,
        name: &str,
        allow_private: bool,
    ) -> NativeResult {
        let Value::Namespace(namespace) = value else {
            return Err(self.typed_error(
                &["error", "type_error"],
                "member assignment requires a local object".to_owned(),
            ));
        };
        let symbol = self.symbols.intern(name);
        let environment = namespace.environment.borrow();
        let Some(binding) = environment.bindings.get(&symbol) else {
            drop(environment);
            return Err(self.typed_error(
                &["error", "name_error"],
                format!("object has no member `{name}`"),
            ));
        };
        if binding.visibility == crate::runtime::BindingVisibility::Private && !allow_private {
            drop(environment);
            return Err(self.typed_error(
                &["error", "visibility_error"],
                format!("member `{name}` is private"),
            ));
        }
        if !matches!(
            binding.mutability,
            crate::runtime::BindingMutability::Mutable
        ) {
            drop(environment);
            return Err(self.typed_error(
                &["error", "mutation_error"],
                format!("cannot assign to immutable member `{name}`"),
            ));
        }
        Ok(Value::Unit)
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
    fn remote_alive(&self, handle: crate::runtime::RemoteNamespaceHandle) -> Result<bool, String> {
        self.host.remote_alive(handle)
    }
    fn remote_stop(&self, handle: crate::runtime::RemoteNamespaceHandle) -> Result<(), String> {
        self.host.stop_remote(handle)
    }
    fn make_remote_namespace(
        &mut self,
        blueprint: crate::runtime::RemoteBlueprint,
        context: Vec<(
            std::sync::Arc<str>,
            crate::runtime::ContextTransferMode,
            Value,
        )>,
    ) -> NativeResult {
        let context = context
            .into_iter()
            .map(|(name, mode, value)| {
                let resolved = value.resolved();
                if mode == crate::runtime::ContextTransferMode::Share {
                    if let Value::TcpListener(listener) = resolved {
                        return Ok((name, crate::runtime::TransportValue::TcpListener(listener)));
                    }
                    if !matches!(resolved, Value::RemoteNamespace(_) | Value::Task(_)) {
                        return Err(
                            "shared context must be a remote object, future, or TCP listener handle",
                        );
                    }
                }
                crate::runtime::TransportValue::from_value(&resolved, |symbol| {
                    self.symbols.resolve(symbol).map(std::sync::Arc::from)
                })
                .map(|value| (name, value))
            })
            .collect::<Result<Vec<_>, _>>()
            .map_err(|message| {
                self.typed_error(
                    &["error", "remote_error", "unsendable_value"],
                    message.to_owned(),
                )
            })?;
        self.host
            .make_remote(blueprint, context)
            .map(Value::RemoteNamespace)
            .map_err(|message| self.typed_error(&["error", "remote_error"], message))
    }
    fn load_remote_member(
        &mut self,
        handle: crate::runtime::RemoteNamespaceHandle,
        member: &str,
    ) -> NativeResult {
        match self.host.remote_member_is_function(handle, member) {
            Ok(true) => Ok(Value::RemoteFunction(handle, std::sync::Arc::from(member))),
            Ok(false) => self
                .host
                .future_remote(handle, std::sync::Arc::from(member), None)
                .map(Value::Task)
                .map_err(|message| self.typed_error(&["error", "remote_error"], message)),
            Err(message) => Err(self.typed_error(&["error", "remote_error"], message)),
        }
    }
    fn call_remote_function(
        &mut self,
        handle: crate::runtime::RemoteNamespaceHandle,
        member: &str,
        argument: &Value,
    ) -> NativeResult {
        let Value::List(arguments) = argument.resolved() else {
            return Err(self.typed_error(
                &["error", "remote_error", "invalid_arguments"],
                "remote function arguments must be a list".to_owned(),
            ));
        };
        let arguments = arguments
            .iter()
            .map(|value| {
                crate::runtime::TransportValue::from_value(value, |symbol| {
                    self.symbols.resolve(symbol).map(std::sync::Arc::from)
                })
            })
            .collect::<Result<Vec<_>, _>>()
            .map_err(|message| {
                self.typed_error(
                    &["error", "remote_error", "unsendable_value"],
                    message.to_owned(),
                )
            })?;
        self.host
            .future_remote(handle, std::sync::Arc::from(member), Some(arguments))
            .map(Value::Task)
            .map_err(|message| self.typed_error(&["error", "remote_error"], message))
    }
    fn task_complete(&self, handle: crate::runtime::TaskHandle) -> Result<bool, String> {
        self.host.task_complete(handle)
    }
    fn task_await(&mut self, handle: crate::runtime::TaskHandle) -> NativeResult {
        match self.host.await_task(handle) {
            Ok(Ok(value)) => {
                let symbols = &mut self.symbols;
                Ok(value.into_value(|name| symbols.intern(name)))
            }
            Ok(Err(error)) => {
                let types = error.types.iter().map(AsRef::as_ref).collect::<Vec<_>>();
                Err(self.typed_error(&types, error.message.to_string()))
            }
            Err(message) => Err(self.typed_error(&["error", "task_error"], message)),
        }
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

impl VmNativeContext {}
