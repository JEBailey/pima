use crate::{
    diagnostic::Diagnostic,
    engine::{ModuleIdentity, ModuleLoader},
    runtime::Value,
    source::SourceMap,
    syntax::{ast::Module, lexer::lex, parser::parse},
};

static NEXT_INTERPRETER_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

#[derive(Clone, Debug, Default)]
pub struct Config {
    pub working_directory: Option<std::path::PathBuf>,
}

#[derive(Clone, Debug)]
pub struct RunOutcome {
    pub value: Option<Value>,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PreparedProgram {
    pub(crate) interpreter_id: u64,
    pub(crate) module_index: usize,
}

impl RunOutcome {
    pub fn is_success(&self) -> bool {
        self.diagnostics.is_empty()
    }
}

#[derive(Debug)]
pub struct Interpreter {
    instance_id: u64,
    pub(crate) sources: SourceMap,
    pub(crate) parsed_modules: Vec<Module>,
    pub(crate) module_loader: ModuleLoader,
    pub(crate) vm: crate::vm::Machine,
    pub(crate) vm_programs: std::collections::HashMap<usize, crate::vm::Program>,
    pub(crate) vm_module_indices: std::collections::HashMap<ModuleIdentity, usize>,
    pub(crate) vm_loading: Vec<ModuleIdentity>,
    pub(crate) vm_session_globals: std::collections::HashMap<std::sync::Arc<str>, Value>,
}

impl Interpreter {
    pub fn new(config: Config) -> Self {
        let working_directory = config
            .working_directory
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_else(|| std::path::PathBuf::from("."));
        let module_directory = camino::Utf8PathBuf::from_path_buf(working_directory.clone())
            .unwrap_or_else(|_| camino::Utf8PathBuf::from("."));
        Self {
            instance_id: NEXT_INTERPRETER_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            sources: SourceMap::default(),
            parsed_modules: Vec::new(),
            module_loader: ModuleLoader::new(module_directory),
            vm: crate::vm::Machine::new(working_directory),
            vm_programs: std::collections::HashMap::new(),
            vm_module_indices: std::collections::HashMap::new(),
            vm_loading: Vec::new(),
            vm_session_globals: std::collections::HashMap::new(),
        }
    }

    pub(crate) fn new_remote_worker(
        working_directory: std::path::PathBuf,
        concurrency: std::sync::Arc<crate::runtime::ConcurrencyHub>,
        network: std::sync::Arc<std::sync::Mutex<crate::native::host::NetworkResources>>,
    ) -> Self {
        let module_directory = camino::Utf8PathBuf::from_path_buf(working_directory.clone())
            .unwrap_or_else(|_| camino::Utf8PathBuf::from("."));
        Self {
            instance_id: NEXT_INTERPRETER_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            sources: SourceMap::default(),
            parsed_modules: Vec::new(),
            module_loader: ModuleLoader::new(module_directory),
            vm: crate::vm::Machine::with_concurrency(working_directory, concurrency, network),
            vm_programs: std::collections::HashMap::new(),
            vm_module_indices: std::collections::HashMap::new(),
            vm_loading: Vec::new(),
            vm_session_globals: std::collections::HashMap::new(),
        }
    }

    pub fn run_source(&mut self, name: &str, source: &str) -> RunOutcome {
        match self.prepare_source(name, source) {
            Ok(program) => self.run_prepared(program),
            Err(diagnostics) => RunOutcome {
                value: None,
                diagnostics,
            },
        }
    }

    /// Resolves a symbol returned by this interpreter to its source spelling.
    ///
    /// Symbol identifiers are local to an interpreter. Embedders should use
    /// this method instead of retaining or comparing their numeric IDs.
    pub fn symbol_name(&self, symbol: crate::runtime::SymbolId) -> Option<&str> {
        self.vm.resolve_symbol(symbol)
    }

    pub fn prepare_source(
        &mut self,
        name: &str,
        source: &str,
    ) -> Result<PreparedProgram, Vec<Diagnostic>> {
        let source_id = self.sources.add(name, source);
        let text = self
            .sources
            .get(source_id)
            .expect("newly added source must exist")
            .text
            .as_ref();
        let tokens = lex(source_id, text)?;
        let module = parse(&tokens)?;
        let module_index = self.parsed_modules.len();
        self.parsed_modules.push(module);
        Ok(PreparedProgram {
            interpreter_id: self.instance_id,
            module_index,
        })
    }

    pub fn run_prepared(&mut self, program: PreparedProgram) -> RunOutcome {
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
        super::vm_runner::run(self, program)
    }

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
}

impl Default for Interpreter {
    fn default() -> Self {
        Self::new(Config::default())
    }
}
