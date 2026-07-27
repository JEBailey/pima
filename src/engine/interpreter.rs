use crate::{
    diagnostic::Diagnostic,
    engine::{ModuleLoader, Signal, eval::CallFrame},
    native::NativeRegistry,
    runtime::{Environment, ErrorMetadata, Namespace, SymbolInterner, UserFunction, Value},
    source::{SourceMap, Span},
    syntax::{ast::Module, lexer::lex, parser::parse},
};

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

#[derive(Debug)]
pub struct Interpreter {
    pub(crate) sources: SourceMap,
    pub(crate) symbols: SymbolInterner,
    pub(crate) environments: Vec<Environment>,
    pub(crate) functions: Vec<UserFunction>,
    pub(crate) namespaces: Vec<Namespace>,
    pub(crate) natives: NativeRegistry,
    pub(crate) parsed_modules: Vec<Module>,
    pub(crate) stored_blocks: Vec<StoredBlock>,
    pub(crate) module_loader: ModuleLoader,
    pub(crate) module_environments: std::collections::HashSet<crate::runtime::EnvironmentId>,
    pub(crate) error_metadata:
        std::collections::HashMap<crate::runtime::NamespaceId, ErrorMetadata>,

    // Execution state
    pub(crate) current_environment: crate::runtime::EnvironmentId,
    pub(crate) current_module: usize,
    pub(crate) call_stack: Vec<CallFrame>,
    pub(crate) active_span: Option<Span>,
}

impl Interpreter {
    /// Creates an interpreter with a fresh root environment and native registry.
    pub fn new(config: Config) -> Self {
        let working_directory = config
            .working_directory
            .clone()
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_else(|| std::path::PathBuf::from("."));
        let working_directory = camino::Utf8PathBuf::from_path_buf(working_directory)
            .unwrap_or_else(|_| camino::Utf8PathBuf::from("."));
        let root_env_id = crate::runtime::EnvironmentId(0);
        let environments = vec![Environment::new(None)];

        let mut interpreter = Self {
            sources: SourceMap::default(),
            symbols: SymbolInterner::default(),
            environments,
            functions: Vec::new(),
            namespaces: Vec::new(),
            natives: NativeRegistry::default(),
            parsed_modules: Vec::new(),
            stored_blocks: Vec::new(),
            module_loader: ModuleLoader::new(working_directory),
            module_environments: std::iter::once(root_env_id).collect(),
            error_metadata: std::collections::HashMap::new(),
            current_environment: root_env_id,
            current_module: 0,
            call_stack: Vec::new(),
            active_span: None,
        };

        // Register native functions
        register_natives(&mut interpreter);

        interpreter
    }

    /// Lexes, parses, and evaluates `source` in the interpreter's current root
    /// environment.
    ///
    /// The interpreter retains declarations between calls. This is useful for
    /// REPLs, but embedders that require isolation should create a separate
    /// interpreter per session.
    pub fn run_source(&mut self, name: &str, source: &str) -> RunOutcome {
        let source_id = match self.add_source(name, source) {
            Ok(id) => id,
            Err(diagnostics) => {
                return RunOutcome {
                    value: None,
                    diagnostics,
                };
            }
        };

        let tokens = match lex(
            source_id,
            self.sources.get(source_id).unwrap().text.as_ref(),
        ) {
            Ok(tokens) => tokens,
            Err(diagnostics) => {
                return RunOutcome {
                    value: None,
                    diagnostics,
                };
            }
        };

        let module = match parse(&tokens) {
            Ok(module) => module,
            Err(diagnostics) => {
                return RunOutcome {
                    value: None,
                    diagnostics,
                };
            }
        };

        let module_index = self.parsed_modules.len();
        self.parsed_modules.push(module);
        self.current_module = module_index;

        // Evaluate the module
        let statements = self.parsed_modules[module_index].statements.clone();
        let result = {
            let context = &mut crate::engine::eval::CallContext::new(self);
            crate::engine::eval::evaluate_statement_list(context, &statements)
        };

        match result {
            Ok(value) => RunOutcome {
                value: Some(value),
                diagnostics: Vec::new(),
            },
            Err(Signal::Throw(error_value)) => {
                let message = extract_error_message(self, &error_value);
                let metadata = match error_value {
                    Value::Namespace(id) => self.error_metadata.get(&id),
                    _ => None,
                };
                RunOutcome {
                    value: None,
                    diagnostics: vec![Diagnostic {
                        severity: crate::diagnostic::Severity::Error,
                        message,
                        primary_span: metadata.map(|metadata| metadata.origin),
                        stack: metadata
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
    ) -> usize {
        let id = self.stored_blocks.len();
        self.stored_blocks.push(StoredBlock {
            block_id,
            module_index,
        });
        id
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
    crate::native::numbers::register(&mut interpreter.natives);
    crate::native::strings::register(&mut interpreter.natives);
    crate::native::lists::register(&mut interpreter.natives);
    crate::native::types::register(&mut interpreter.natives);
    crate::native::console::register(&mut interpreter.natives);

    // Bind each native in the root environment so identifiers resolve to them
    let root_env = &mut interpreter.environments[0];
    for (id, def) in interpreter.natives.iter_with_ids() {
        let sym = interpreter.symbols.intern(def.name);
        root_env.bindings.insert(
            sym,
            crate::runtime::Binding {
                value: crate::runtime::Value::NativeFunction(id),
                mutability: crate::runtime::BindingMutability::Immutable,
                visibility: crate::runtime::BindingVisibility::Public,
            },
        );
    }

    // I/O is available only through the explicit `/po/io` virtual module.
    crate::native::io::register(&mut interpreter.natives);
}

fn extract_error_message(interpreter: &mut Interpreter, value: &Value) -> String {
    if let Value::Namespace(ns_id) = value {
        let ns = &interpreter.namespaces[ns_id.0 as usize];
        let env = &interpreter.environments[ns.environment.0 as usize];
        let msg_symbol = interpreter.symbols.intern("message");
        if let Some(binding) = env.bindings.get(&msg_symbol)
            && let Value::String(s) = &binding.value
        {
            return s.to_string();
        }
    }
    "<error>".to_string()
}
