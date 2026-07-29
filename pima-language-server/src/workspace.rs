use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use pima::{
    source::{SourceMap, Span},
    syntax::{
        ast::{Module, NodeId, NodeKind, Visibility},
        lexer::lex,
        parser::parse_recovering,
    },
};
use tower_lsp::lsp_types::{SymbolKind, Url};

use crate::ast_utils::{namespace_block, parameter_list, pattern_captures};

#[derive(Clone, Debug)]
pub struct IndexedSymbol {
    pub name: String,
    pub span: Span,
    pub kind: SymbolKind,
    pub detail: Option<String>,
    pub members: HashMap<String, IndexedSymbol>,
}

#[derive(Clone, Debug)]
struct Import {
    path: String,
    alias: Option<String>,
}

#[derive(Clone, Debug)]
pub struct IndexedDocument {
    pub text: String,
    exports: HashMap<String, IndexedSymbol>,
    imports: Vec<Import>,
}

#[derive(Default)]
pub struct WorkspaceIndex {
    documents: HashMap<Url, IndexedDocument>,
}

impl WorkspaceIndex {
    pub fn scan(&mut self, roots: &[Url]) {
        for root in roots {
            let Ok(path) = root.to_file_path() else {
                continue;
            };
            for file in pima_files(&path) {
                if let Ok(text) = std::fs::read_to_string(&file)
                    && let Ok(uri) = Url::from_file_path(&file)
                {
                    self.upsert(uri, text);
                }
            }
        }
    }

    pub fn upsert(&mut self, uri: Url, text: String) {
        if let Some(document) = index_document(&text) {
            self.documents.insert(uri, document);
        }
    }

    pub fn remove(&mut self, uri: &Url) {
        self.documents.remove(uri);
    }

    pub fn imported_completions(&self, uri: &Url) -> Vec<&IndexedSymbol> {
        let Some(document) = self.documents.get(uri) else {
            return Vec::new();
        };
        let mut symbols = Vec::new();
        for import in &document.imports {
            if import.alias.is_some() {
                continue;
            }
            if let Some(target) = self.resolve_import(uri, &import.path)
                && let Some(target) = self.documents.get(&target)
            {
                symbols.extend(target.exports.values());
            }
        }
        symbols
    }

    pub fn import_aliases(&self, uri: &Url) -> Vec<&str> {
        self.documents
            .get(uri)
            .into_iter()
            .flat_map(|document| &document.imports)
            .filter_map(|import| import.alias.as_deref())
            .collect()
    }

    pub fn member_completions(&self, uri: &Url, receiver: &str) -> Vec<&IndexedSymbol> {
        let Some(document) = self.documents.get(uri) else {
            return Vec::new();
        };
        if let Some(namespace) = document.exports.get(receiver) {
            return namespace.members.values().collect();
        }
        let Some(import) = document
            .imports
            .iter()
            .find(|import| import.alias.as_deref() == Some(receiver))
        else {
            return Vec::new();
        };
        self.resolve_import(uri, &import.path)
            .and_then(|target| self.documents.get(&target))
            .map(|target| target.exports.values().collect())
            .unwrap_or_default()
    }

    pub fn definition(
        &self,
        uri: &Url,
        name: &str,
        receiver: Option<&str>,
    ) -> Option<(&Url, &IndexedDocument, &IndexedSymbol)> {
        let (document_uri, document) = self.documents.get_key_value(uri)?;
        if let Some(receiver) = receiver {
            if let Some(namespace) = document.exports.get(receiver)
                && let Some(symbol) = namespace.members.get(name)
            {
                return Some((document_uri, document, symbol));
            }
            let import = document
                .imports
                .iter()
                .find(|import| import.alias.as_deref() == Some(receiver))?;
            let target_uri = self.resolve_import(uri, &import.path)?;
            let target = self.documents.get_key_value(&target_uri)?;
            let symbol = target.1.exports.get(name)?;
            return Some((target.0, target.1, symbol));
        }
        for import in &document.imports {
            if import.alias.is_some() {
                continue;
            }
            let target_uri = self.resolve_import(uri, &import.path)?;
            let target = self.documents.get_key_value(&target_uri)?;
            if let Some(symbol) = target.1.exports.get(name) {
                return Some((target.0, target.1, symbol));
            }
        }
        None
    }

    fn resolve_import(&self, source: &Url, import: &str) -> Option<Url> {
        if import.starts_with('/') {
            return None;
        }
        let source = source.to_file_path().ok()?;
        let path = source.parent()?.join(import);
        let path = std::fs::canonicalize(&path).unwrap_or(path);
        Url::from_file_path(path).ok()
    }
}

fn index_document(text: &str) -> Option<IndexedDocument> {
    let mut sources = SourceMap::default();
    let source = sources.add("<workspace>", text);
    let tokens = lex(source, text).ok()?;
    let output = parse_recovering(&tokens);
    let module = output.module;
    let mut exports = HashMap::new();
    let mut imports = Vec::new();
    for statement in &module.statements {
        match &module.node(*statement).kind {
            NodeKind::Function {
                visibility: Visibility::Public,
                name,
                parameters,
                ..
            } => {
                exports.insert(
                    name.text.to_string(),
                    IndexedSymbol {
                        name: name.text.to_string(),
                        span: name.span,
                        kind: SymbolKind::FUNCTION,
                        detail: Some(parameter_list(parameters)),
                        members: HashMap::new(),
                    },
                );
            }
            NodeKind::Binding {
                visibility: Visibility::Public,
                pattern,
                value,
                ..
            } => {
                for name in pattern_captures(pattern) {
                    let members = namespace_block(&module, *value)
                        .map(|block| public_members(&module, &module.block(block).statements))
                        .unwrap_or_default();
                    exports.insert(
                        name.text.to_string(),
                        IndexedSymbol {
                            name: name.text.to_string(),
                            span: name.span,
                            kind: if members.is_empty() {
                                SymbolKind::VARIABLE
                            } else {
                                SymbolKind::NAMESPACE
                            },
                            detail: None,
                            members,
                        },
                    );
                }
            }
            NodeKind::Import { path, alias } => imports.push(Import {
                path: path.to_string(),
                alias: alias.as_ref().map(ToString::to_string),
            }),
            _ => {}
        }
    }
    Some(IndexedDocument {
        text: text.to_owned(),
        exports,
        imports,
    })
}

fn public_members(module: &Module, statements: &[NodeId]) -> HashMap<String, IndexedSymbol> {
    let mut members = HashMap::new();
    for statement in statements {
        match &module.node(*statement).kind {
            NodeKind::Function {
                visibility: Visibility::Public,
                name,
                parameters,
                ..
            } => {
                members.insert(
                    name.text.to_string(),
                    IndexedSymbol {
                        name: name.text.to_string(),
                        span: name.span,
                        kind: SymbolKind::METHOD,
                        detail: Some(parameter_list(parameters)),
                        members: HashMap::new(),
                    },
                );
            }
            NodeKind::Binding {
                visibility: Visibility::Public,
                pattern,
                ..
            } => {
                for name in pattern_captures(pattern) {
                    members.insert(
                        name.text.to_string(),
                        IndexedSymbol {
                            name: name.text.to_string(),
                            span: name.span,
                            kind: SymbolKind::FIELD,
                            detail: None,
                            members: HashMap::new(),
                        },
                    );
                }
            }
            _ => {}
        }
    }
    members
}

fn pima_files(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(path) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(path) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if path
                    .file_name()
                    .is_none_or(|name| name != "target" && name != "node_modules")
                {
                    pending.push(path);
                }
            } else if path
                .extension()
                .is_some_and(|extension| extension == "pima")
            {
                files.push(path);
            }
        }
    }
    files
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    #[test]
    fn indexes_only_public_top_level_declarations() {
        let document = index_document(
            "pub set answer 42\nset private 0\npub function read (:value) { value }\n",
        )
        .expect("document");
        assert!(document.exports.contains_key("answer"));
        assert!(document.exports.contains_key("read"));
        assert!(!document.exports.contains_key("private"));
    }

    #[test]
    fn indexes_public_namespace_members() {
        let document = index_document(
            "pub set Point {\n    pub set x 0\n    set hidden 1\n    pub function move (:amount) { amount }\n}\n",
        )
        .expect("document");
        let point = &document.exports["Point"];
        assert!(point.members.contains_key("x"));
        assert!(point.members.contains_key("move"));
        assert!(!point.members.contains_key("hidden"));
    }

    #[test]
    fn resolves_unaliased_and_aliased_imports_across_files() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "pima-language-server-{}-{unique}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).expect("temporary workspace");
        let library = root.join("library.pima");
        let main = root.join("main.pima");
        std::fs::write(
            &library,
            "pub set answer 42\npub function double (:value) { * value 2 }\n",
        )
        .expect("library");
        std::fs::write(
            &main,
            "import \"library.pima\"\nimport \"library.pima\" as Library\ndouble answer\nLibrary.double 2\n",
        )
        .expect("main");

        let root_uri = Url::from_directory_path(&root).expect("root uri");
        let main_uri = Url::from_file_path(std::fs::canonicalize(&main).expect("main path"))
            .expect("main uri");
        let mut index = WorkspaceIndex::default();
        index.scan(&[root_uri]);

        assert!(
            index
                .imported_completions(&main_uri)
                .iter()
                .any(|symbol| symbol.name == "double")
        );
        assert!(
            index
                .member_completions(&main_uri, "Library")
                .iter()
                .any(|symbol| symbol.name == "answer")
        );
        let (uri, _, symbol) = index
            .definition(&main_uri, "double", Some("Library"))
            .expect("cross-file definition");
        assert_eq!(symbol.name, "double");
        assert_eq!(
            uri,
            &Url::from_file_path(std::fs::canonicalize(&library).expect("library path"))
                .expect("library uri")
        );

        std::fs::remove_dir_all(&root).expect("remove temporary workspace");
    }
}
