use std::fmt;

use camino::{Utf8Component, Utf8Path, Utf8PathBuf};

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

#[derive(Debug)]
/// Resolves import paths relative to a configured working directory.
pub struct ModuleLoader {
    working_directory: Utf8PathBuf,
}

impl ModuleLoader {
    /// Creates an empty loader using `working_directory` for pathless importers.
    pub fn new(working_directory: Utf8PathBuf) -> Self {
        Self { working_directory }
    }

    /// Resolves an import to a canonical identity.
    ///
    /// Relative file paths use the importing file's directory when available.
    /// Paths below `/pima/` are virtual and never touch the filesystem.
    pub fn resolve(
        &self,
        requested: &str,
        importer: Option<&Utf8Path>,
    ) -> Result<ModuleIdentity, ModulePathError> {
        if requested.starts_with("/pima/") {
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

    pub fn working_directory(&self) -> &Utf8Path {
        &self.working_directory
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
