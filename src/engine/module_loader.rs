use std::fmt;

use camino::{Utf8Component, Utf8Path, Utf8PathBuf};
use indexmap::IndexMap;

use crate::runtime::{EnvironmentId, Value};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Lifecycle state for a canonical module identity.
pub enum ModuleState {
    Unloaded,
    Loading,
    Loaded,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
/// Canonical cache key for either a built-in or filesystem module.
pub enum ModuleIdentity {
    Virtual(Utf8PathBuf),
    File(Utf8PathBuf),
}

impl ModuleIdentity {
    pub fn path(&self) -> &Utf8Path {
        match self {
            Self::Virtual(path) | Self::File(path) => path,
        }
    }
}

#[derive(Clone, Debug)]
pub struct ModuleRecord {
    pub identity: ModuleIdentity,
    pub state: ModuleState,
    pub environment: Option<EnvironmentId>,
    pub module_index: Option<usize>,
    pub cached_error: Option<Value>,
}

impl ModuleRecord {
    fn unloaded(identity: ModuleIdentity) -> Self {
        Self {
            identity,
            state: ModuleState::Unloaded,
            environment: None,
            module_index: None,
            cached_error: None,
        }
    }
}

#[derive(Debug)]
/// Resolves import paths and owns the per-interpreter module lifecycle cache.
///
/// Evaluation is intentionally handled by the interpreter. Keeping resolution
/// and lifecycle state here makes cycle detection and repeated-import behavior
/// independent of the parser and evaluator.
pub struct ModuleLoader {
    working_directory: Utf8PathBuf,
    records: IndexMap<ModuleIdentity, ModuleRecord>,
    loading_stack: Vec<ModuleIdentity>,
}

impl ModuleLoader {
    /// Creates an empty loader using `working_directory` for pathless importers.
    pub fn new(working_directory: Utf8PathBuf) -> Self {
        Self {
            working_directory,
            records: IndexMap::new(),
            loading_stack: Vec::new(),
        }
    }

    /// Resolves an import to a canonical identity.
    ///
    /// Relative file paths use the importing file's directory when available.
    /// Paths below `/po/` are virtual and never touch the filesystem.
    pub fn resolve(
        &self,
        requested: &str,
        importer: Option<&Utf8Path>,
    ) -> Result<ModuleIdentity, ModulePathError> {
        if requested.starts_with("/po/") {
            return resolve_virtual(requested);
        }

        let requested = Utf8Path::new(requested);
        let candidate = if requested.is_absolute() {
            requested.to_path_buf()
        } else {
            let base = importer
                .and_then(Utf8Path::parent)
                .unwrap_or(&self.working_directory);
            base.join(requested)
        };

        let canonical = std::fs::canonicalize(&candidate)
            .map_err(|source| ModulePathError::Io {
                path: candidate.clone(),
                source,
            })
            .and_then(|path| {
                Utf8PathBuf::from_path_buf(path).map_err(|path| ModulePathError::NonUtf8 { path })
            })?;
        Ok(ModuleIdentity::File(canonical))
    }

    pub fn record_mut(&mut self, identity: ModuleIdentity) -> &mut ModuleRecord {
        self.records
            .entry(identity.clone())
            .or_insert_with(|| ModuleRecord::unloaded(identity))
    }

    pub fn record(&self, identity: &ModuleIdentity) -> Option<&ModuleRecord> {
        self.records.get(identity)
    }

    pub fn working_directory(&self) -> &Utf8Path {
        &self.working_directory
    }

    pub fn begin_loading(&mut self, identity: ModuleIdentity) {
        self.record_mut(identity.clone()).state = ModuleState::Loading;
        self.loading_stack.push(identity);
    }

    pub fn finish_loading(&mut self, identity: &ModuleIdentity) {
        if self.loading_stack.last() == Some(identity) {
            self.loading_stack.pop();
        } else {
            self.loading_stack.retain(|loading| loading != identity);
        }
    }

    pub fn cycle(&self, repeated: &ModuleIdentity) -> Vec<ModuleIdentity> {
        let start = self
            .loading_stack
            .iter()
            .position(|identity| identity == repeated)
            .unwrap_or(0);
        self.loading_stack[start..]
            .iter()
            .chain(std::iter::once(repeated))
            .cloned()
            .collect()
    }
}

#[derive(Debug)]
pub enum ModulePathError {
    InvalidVirtualPath {
        path: String,
    },
    Io {
        path: Utf8PathBuf,
        source: std::io::Error,
    },
    NonUtf8 {
        path: std::path::PathBuf,
    },
}

impl fmt::Display for ModulePathError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidVirtualPath { path } => {
                write!(formatter, "invalid virtual module path `{path}`")
            }
            Self::Io { path, source } => {
                write!(formatter, "could not resolve module `{path}`: {source}")
            }
            Self::NonUtf8 { path } => {
                write!(
                    formatter,
                    "module path is not valid UTF-8: {}",
                    path.display()
                )
            }
        }
    }
}

impl std::error::Error for ModulePathError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

fn resolve_virtual(requested: &str) -> Result<ModuleIdentity, ModulePathError> {
    let path = Utf8Path::new(requested);
    let valid = path
        .components()
        .all(|component| matches!(component, Utf8Component::RootDir | Utf8Component::Normal(_)));

    if !valid || requested.contains('\\') || requested.ends_with('/') {
        return Err(ModulePathError::InvalidVirtualPath {
            path: requested.to_owned(),
        });
    }

    Ok(ModuleIdentity::Virtual(path.to_path_buf()))
}
