use crate::{
    diagnostic::Diagnostic,
    engine::{ModuleIdentity, ModuleLoader, Signal, eval::CallFrame},
    native::NativeRegistry,
    runtime::{Environment, EnvironmentRef, Namespace, SymbolInterner, Value},
    source::{SourceMap, Span},
    syntax::{ast::Module, lexer::lex, parser::parse},
};

static NEXT_INTERPRETER_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

#[derive(Clone, Debug, Default)]
/// Host configuration used when creating an [`Interpreter`].
pub struct Config {
    /// Base directory for imports made by sources without a filesystem path.
    ///
    /// File-backed modules resolve their relative imports from the importing
    /// file instead. When omitted, this defaults to the process working
    /// directory.
    pub working_directory: Option<std::path::PathBuf>,
}

#[derive(Clone, Debug)]
/// Result of running one Pima source unit.
pub struct RunOutcome {
    /// The final statement value, or `None` when the run failed.
    pub value: Option<Value>,
    /// Syntax or uncaught runtime errors produced by the run.
    pub diagnostics: Vec<Diagnostic>,
}

/// An immutable parsed Pima source unit owned by one [`Interpreter`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PreparedProgram {
    pub(crate) interpreter_id: u64,
    pub(crate) module_index: usize,
}

impl RunOutcome {
    /// Returns `true` when evaluation completed without diagnostics.
    pub fn is_success(&self) -> bool {
        self.diagnostics.is_empty()
    }
}

/// A code block and the parsed module that owns its AST.
#[derive(Clone, Debug)]
pub struct StoredBlock {
    /// Which parsed module owns this block's AST.
    pub module_index: usize,
    /// The original BlockId within that module.
    pub block_id: crate::syntax::ast::BlockId,
}

unsafe impl<V: dumpster::Visitor> dumpster::TraceWith<V> for StoredBlock {
    fn accept(&self, _visitor: &mut V) -> Result<(), ()> {
        Ok(())
    }
}

#[derive(Debug)]
pub struct Interpreter {
    instance_id: u64,
    execution_engine: ExecutionEngine,
    pub(crate) sources: SourceMap,
    pub(crate) symbols: SymbolInterner,
    pub(crate) prelude_environment: EnvironmentRef,
    pub(crate) primitive_environment: EnvironmentRef,
    pub(crate) natives: NativeRegistry,
    pub(crate) parsed_modules: Vec<Module>,
    pub(crate) module_loader: ModuleLoader,
    pub(crate) module_environments: Vec<EnvironmentRef>,
    pub(crate) host: crate::native::host::HostResources,
    pub(crate) vm: crate::vm::Machine,
    pub(crate) vm_programs: std::collections::HashMap<usize, crate::vm::Program>,
    pub(crate) vm_module_indices: std::collections::HashMap<ModuleIdentity, usize>,
    pub(crate) vm_loading: Vec<ModuleIdentity>,
    pub(crate) vm_session_globals: std::collections::HashMap<std::sync::Arc<str>, Value>,

    // Execution state
    pub(crate) current_environment: EnvironmentRef,
    pub(crate) current_module: usize,
    pub(crate) call_stack: Vec<CallFrame>,
    pub(crate) active_span: Option<Span>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ExecutionEngine {
    TreeWalk,
    RegisterVm,
}

impl Interpreter {
    /// Creates an interpreter with a fresh root environment and native registry.
    pub fn new(config: Config) -> Self {
        let working_directory = config
            .working_directory
            .clone()
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_else(|| std::path::PathBuf::from("."));
        let host = crate::native::host::HostResources::new(working_directory.clone());
        let vm = crate::vm::Machine::new(working_directory.clone());
        let working_directory = camino::Utf8PathBuf::from_path_buf(working_directory)
            .unwrap_or_else(|_| camino::Utf8PathBuf::from("."));
        let prelude_environment =
            dumpster::unsync::Gc::new(std::cell::RefCell::new(Environment::new(None)));
        let primitive_environment =
            dumpster::unsync::Gc::new(std::cell::RefCell::new(Environment::new(None)));
        let root_environment = dumpster::unsync::Gc::new(std::cell::RefCell::new(
            Environment::new(Some(primitive_environment.clone())),
        ));

        let mut interpreter = Self {
            instance_id: NEXT_INTERPRETER_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            execution_engine: ExecutionEngine::TreeWalk,
            sources: SourceMap::default(),
            symbols: SymbolInterner::default(),
            prelude_environment,
            primitive_environment,
            natives: NativeRegistry::default(),
            parsed_modules: Vec::new(),
            module_loader: ModuleLoader::new(working_directory),
            module_environments: vec![root_environment.clone()],
            host,
            vm,
            vm_programs: std::collections::HashMap::new(),
            vm_module_indices: std::collections::HashMap::new(),
            vm_loading: Vec::new(),
            vm_session_globals: std::collections::HashMap::new(),
            current_environment: root_environment,
            current_module: 0,
            call_stack: Vec::new(),
            active_span: None,
        };

        // Register native functions
        register_natives(&mut interpreter);

        interpreter
    }

    /// Creates an interpreter whose ordinary run methods use the register VM.
    pub fn new_vm(config: Config) -> Self {
        let mut interpreter = Self::new(config);
        interpreter.execution_engine = ExecutionEngine::RegisterVm;
        interpreter
    }

    /// Lexes, parses, and evaluates `source` in the interpreter's current root
    /// environment.
    ///
    /// The interpreter retains declarations between calls. This is useful for
    /// REPLs, but embedders that require isolation should create a separate
    /// interpreter per session.
    pub fn run_source(&mut self, name: &str, source: &str) -> RunOutcome {
        match self.prepare_source(name, source) {
            Ok(program) => self.run_prepared(program),
            Err(diagnostics) => RunOutcome {
                value: None,
                diagnostics,
            },
        }
    }

    /// Lexes, parses, compiles, and executes a source unit with the register VM.
    pub fn run_source_vm(&mut self, name: &str, source: &str) -> RunOutcome {
        match self.prepare_source(name, source) {
            Ok(program) => self.run_prepared_vm(program),
            Err(diagnostics) => RunOutcome {
                value: None,
                diagnostics,
            },
        }
    }

    /// Lexes and parses a source unit without evaluating it.
    ///
    /// The returned handle belongs to this interpreter and can be executed
    /// repeatedly with [`Interpreter::run_prepared`].
    pub fn prepare_source(
        &mut self,
        name: &str,
        source: &str,
    ) -> Result<PreparedProgram, Vec<Diagnostic>> {
        let source_id = self.add_source(name, source)?;

        let tokens = lex(
            source_id,
            self.sources.get(source_id).unwrap().text.as_ref(),
        )?;

        let module = parse(&tokens)?;

        let module_index = self.parsed_modules.len();
        self.parsed_modules.push(module);
        Ok(PreparedProgram {
            interpreter_id: self.instance_id,
            module_index,
        })
    }

    /// Evaluates a source unit previously returned by
    /// [`Interpreter::prepare_source`].
    pub fn run_prepared(&mut self, program: PreparedProgram) -> RunOutcome {
        if self.execution_engine == ExecutionEngine::RegisterVm {
            return self.run_prepared_vm(program);
        }
        if program.interpreter_id != self.instance_id
            || program.module_index >= self.parsed_modules.len()
        {
            return RunOutcome {
                value: None,
                diagnostics: vec![Diagnostic::error(
                    "prepared program belongs to a different interpreter",
                )],
            };
        }

        let module_index = program.module_index;
        self.current_module = module_index;

        let statements = self.parsed_modules[module_index].statements.clone();
        let result = {
            let context = &mut crate::engine::eval::CallContext::new(self);
            crate::engine::eval::evaluate_statement_list(context, &statements)
        };

        self.outcome_from_result(result)
    }

    /// Compiles and executes a prepared source unit with the register VM.
    pub fn run_prepared_vm(&mut self, program: PreparedProgram) -> RunOutcome {
        if program.interpreter_id != self.instance_id
            || program.module_index >= self.parsed_modules.len()
        {
            return RunOutcome {
                value: None,
                diagnostics: vec![Diagnostic::error(
                    "prepared program belongs to a different interpreter",
                )],
            };
        }
        crate::engine::vm_runner::run(self, program)
    }

    fn outcome_from_result(&mut self, result: Result<Value, Signal>) -> RunOutcome {
        match result {
            Ok(value) => RunOutcome {
                value: Some(value),
                diagnostics: Vec::new(),
            },
            Err(Signal::Throw(error_value)) => {
                let message = extract_error_message(self, &error_value);
                let metadata = match &error_value {
                    Value::Namespace(namespace) => namespace.error_metadata.borrow().clone(),
                    _ => None,
                };
                RunOutcome {
                    value: None,
                    diagnostics: vec![Diagnostic {
                        severity: crate::diagnostic::Severity::Error,
                        message,
                        primary_span: metadata.as_ref().map(|metadata| metadata.origin),
                        stack: metadata
                            .as_ref()
                            .map(|metadata| metadata.stack.clone())
                            .unwrap_or_default(),
                    }],
                }
            }
            Err(Signal::Return(_)) | Err(Signal::Break(_)) | Err(Signal::Continue) => {
                let msg = match result {
                    Err(Signal::Return(_)) => "unconsumed return",
                    Err(Signal::Break(_)) => "unconsumed break",
                    Err(Signal::Continue) => "unconsumed continue",
                    _ => unreachable!(),
                };
                RunOutcome {
                    value: None,
                    diagnostics: vec![Diagnostic::error(format!("misplaced control flow: {msg}"))],
                }
            }
        }
    }

    /// Reads and runs a UTF-8 Pima source file.
    pub fn run_file(&mut self, path: impl AsRef<std::path::Path>) -> RunOutcome {
        let path = path.as_ref();
        match std::fs::read_to_string(path) {
            Ok(source) => self.run_source(&path.display().to_string(), &source),
            Err(error) => RunOutcome {
                value: None,
                diagnostics: vec![Diagnostic::error(format!(
                    "could not read {}: {error}",
                    path.display()
                ))],
            },
        }
    }

    fn add_source(
        &mut self,
        name: &str,
        source: &str,
    ) -> Result<crate::source::SourceId, Vec<Diagnostic>> {
        Ok(self.sources.add(name, source))
    }

    /// Store a block from a module so it gets a stable Value::Block id.
    pub fn store_block(
        &mut self,
        block_id: crate::syntax::ast::BlockId,
        module_index: usize,
    ) -> crate::runtime::BlockRef {
        dumpster::unsync::Gc::new(StoredBlock {
            block_id,
            module_index,
        })
    }
}

impl Default for Interpreter {
    fn default() -> Self {
        Self::new(Config::default())
    }
}

// ── Native Registration ──

fn register_natives(interpreter: &mut Interpreter) {
    // Register all native definitions
    crate::native::register_core(&mut interpreter.natives);

    let definitions = interpreter
        .natives
        .iter_with_ids()
        .map(|(id, definition)| (id, definition.name))
        .collect::<Vec<_>>();
    for (id, name) in definitions {
        let namespace = crate::native::core_namespace(name);
        if let Some(namespace) = namespace {
            bind_native_member(interpreter, namespace, name, id);
            // Core natives remain available to the implementation prelude.
            bind_native(
                interpreter,
                interpreter.prelude_environment.clone(),
                name,
                id,
            );
        } else {
            bind_native(
                interpreter,
                interpreter.prelude_environment.clone(),
                name,
                id,
            );
            bind_native(
                interpreter,
                interpreter.primitive_environment.clone(),
                name,
                id,
            );
        }
    }

    crate::native::io::register(&mut interpreter.natives);
    crate::native::tcp::register(&mut interpreter.natives);
}

fn bind_native_member(
    interpreter: &mut Interpreter,
    namespace_name: &str,
    member_name: &str,
    native: crate::runtime::NativeFunctionId,
) {
    let namespace_symbol = interpreter.symbols.intern(namespace_name);
    let namespace = {
        let prelude = interpreter.prelude_environment.borrow();
        prelude
            .bindings
            .get(&namespace_symbol)
            .and_then(|binding| match &binding.value {
                Value::Namespace(namespace) => Some(namespace.clone()),
                _ => None,
            })
    };
    let namespace = namespace.unwrap_or_else(|| {
        let environment =
            dumpster::unsync::Gc::new(std::cell::RefCell::new(Environment::new(None)));
        let namespace = dumpster::unsync::Gc::new(Namespace {
            environment: environment.clone(),
            types: Vec::new(),
            error_metadata: std::cell::RefCell::new(None),
        });
        interpreter
            .prelude_environment
            .borrow_mut()
            .bindings
            .insert(
                namespace_symbol,
                crate::runtime::Binding {
                    value: Value::Namespace(namespace.clone()),
                    mutability: crate::runtime::BindingMutability::Immutable,
                    visibility: crate::runtime::BindingVisibility::Private,
                },
            );
        namespace
    });
    let environment = namespace.environment.clone();
    bind_native(interpreter, environment, member_name, native);
}

fn bind_native(
    interpreter: &mut Interpreter,
    environment: crate::runtime::EnvironmentRef,
    name: &str,
    native: crate::runtime::NativeFunctionId,
) {
    let symbol = interpreter.symbols.intern(name);
    environment.borrow_mut().bindings.insert(
        symbol,
        crate::runtime::Binding {
            value: Value::NativeFunction(native),
            mutability: crate::runtime::BindingMutability::Immutable,
            visibility: crate::runtime::BindingVisibility::Public,
        },
    );
}

fn extract_error_message(interpreter: &mut Interpreter, value: &Value) -> String {
    if let Value::Namespace(namespace) = value {
        let env = namespace.environment.borrow();
        let msg_symbol = interpreter.symbols.intern("message");
        if let Some(binding) = env.bindings.get(&msg_symbol)
            && let Value::String(s) = &binding.value
        {
            return s.to_string();
        }
    }
    "<error>".to_string()
}

#[cfg(test)]
mod memory_tests {
    use super::*;

    #[test]
    fn unreachable_recursive_closure_environments_are_collected() {
        let baseline = crate::runtime::live_environment_count();
        let mut interpreter = Interpreter::default();
        let setup = interpreter.run_source(
            "<memory-setup>",
            r#"
function make_cycle () {
    function recursive () { recursive }
}
"#,
        );
        assert!(setup.is_success(), "{:?}", setup.diagnostics);
        let rooted = crate::runtime::live_environment_count();

        for _ in 0..1_000 {
            let outcome = interpreter.run_source("<memory-call>", "[make_cycle ()]");
            assert!(outcome.is_success(), "{:?}", outcome.diagnostics);
        }

        assert!(
            crate::runtime::live_environment_count() > rooted,
            "the test must create unreachable cyclic environments"
        );
        dumpster::unsync::collect();
        assert_eq!(crate::runtime::live_environment_count(), rooted);

        drop(setup);
        drop(interpreter);
        dumpster::unsync::collect();
        assert_eq!(crate::runtime::live_environment_count(), baseline);
    }

    #[test]
    fn escaped_closure_keeps_its_captured_environment_alive() {
        let baseline = crate::runtime::live_environment_count();
        let mut interpreter = Interpreter::default();
        let outcome = interpreter.run_source(
            "<escaped-closure>",
            r#"
function make_closure () {
    val captured 41
    function read () { captured }
    read
}

val saved [make_closure ()]
[saved ()]
"#,
        );
        assert!(outcome.is_success(), "{:?}", outcome.diagnostics);
        let live_with_closure = crate::runtime::live_environment_count();

        dumpster::unsync::collect();
        assert_eq!(
            crate::runtime::live_environment_count(),
            live_with_closure,
            "collection must preserve an environment reached by an escaped closure"
        );

        let call = interpreter.run_source("<escaped-closure-call>", "[saved ()]");
        assert!(call.is_success(), "{:?}", call.diagnostics);
        assert_eq!(call.value, Some(Value::Integer(41)));

        drop(call);
        drop(outcome);
        drop(interpreter);
        dumpster::unsync::collect();
        assert_eq!(crate::runtime::live_environment_count(), baseline);
    }
}
