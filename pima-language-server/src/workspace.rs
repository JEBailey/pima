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
        token::TokenKind,
    },
};
use tower_lsp::lsp_types::{SymbolKind, Url};

use crate::ast_utils::{namespace_block, parameter_list, pattern_captures};
use crate::semantic::SemanticModel;

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
struct IndexedReference {
    name: String,
    receiver: Option<String>,
    span: Span,
}

#[derive(Clone, Debug)]
pub struct WorkspaceOccurrence {
    pub uri: Url,
    pub text: String,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct IndexedDocument {
    pub text: String,
    exports: HashMap<String, IndexedSymbol>,
    imports: Vec<Import>,
    references: Vec<IndexedReference>,
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
        let document = index_document(&text).unwrap_or_else(|| IndexedDocument {
            text,
            exports: HashMap::new(),
            imports: Vec::new(),
            references: Vec::new(),
        });
        self.documents.insert(uri, document);
    }

    pub fn remove(&mut self, uri: &Url) {
        self.documents.remove(uri);
    }

    pub fn snapshots(&self) -> Vec<(Url, String)> {
        self.documents
            .iter()
            .map(|(uri, document)| (uri.clone(), document.text.clone()))
            .collect()
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

    pub fn target_at(&self, uri: &Url, offset: usize) -> Option<(&Url, &IndexedSymbol)> {
        let (document_uri, document) = self.documents.get_key_value(uri)?;
        if let Some(symbol) = find_export_at(&document.exports, offset) {
            return Some((document_uri, symbol));
        }
        let reference = document
            .references
            .iter()
            .find(|reference| reference.span.start <= offset && offset < reference.span.end)?;
        let (target_uri, _, symbol) =
            self.definition(uri, &reference.name, reference.receiver.as_deref())?;
        Some((target_uri, symbol))
    }

    pub fn occurrences(
        &self,
        target_uri: &Url,
        target_span: Span,
        include_declaration: bool,
    ) -> Vec<WorkspaceOccurrence> {
        let mut occurrences = Vec::new();
        if include_declaration && let Some(document) = self.documents.get(target_uri) {
            occurrences.push(WorkspaceOccurrence {
                uri: target_uri.clone(),
                text: document.text.clone(),
                span: target_span,
            });
        }
        for (uri, document) in &self.documents {
            for reference in &document.references {
                let Some((resolved_uri, _, symbol)) =
                    self.definition(uri, &reference.name, reference.receiver.as_deref())
                else {
                    continue;
                };
                if resolved_uri == target_uri && symbol.span == target_span {
                    occurrences.push(WorkspaceOccurrence {
                        uri: uri.clone(),
                        text: document.text.clone(),
                        span: reference.span,
                    });
                }
            }
        }
        occurrences
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
    let semantic = SemanticModel::build(&module);
    let mut exports = HashMap::new();
    let mut imports = Vec::new();
    for statement in &module.statements {
        match &module.node(*statement).kind {
            NodeKind::Function {
                visibility: Visibility::Public,
                name,
                parameter,
                ..
            } => {
                exports.insert(
                    name.text.to_string(),
                    IndexedSymbol {
                        name: name.text.to_string(),
                        span: name.span,
                        kind: SymbolKind::FUNCTION,
                        detail: Some(parameter_list(parameter)),
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
    let references = tokens
        .iter()
        .filter_map(|token| {
            let TokenKind::Identifier(name) = &token.kind else {
                return None;
            };
            if semantic.symbol_at(token.span.start).is_some() {
                return None;
            }
            Some(IndexedReference {
                name: name.to_string(),
                receiver: member_receiver(text, token.span.start).map(ToOwned::to_owned),
                span: token.span,
            })
        })
        .collect();
    Some(IndexedDocument {
        text: text.to_owned(),
        exports,
        imports,
        references,
    })
}

fn find_export_at(
    exports: &HashMap<String, IndexedSymbol>,
    offset: usize,
) -> Option<&IndexedSymbol> {
    for symbol in exports.values() {
        if symbol.span.start <= offset && offset < symbol.span.end {
            return Some(symbol);
        }
        if let Some(member) = find_export_at(&symbol.members, offset) {
            return Some(member);
        }
    }
    None
}

fn member_receiver(text: &str, offset: usize) -> Option<&str> {
    let prefix = text.get(..offset)?.trim_end();
    let without_dot = prefix.strip_suffix('.')?;
    let start = without_dot
        .rfind(|character: char| !(character.is_ascii_alphanumeric() || character == '_'))
        .map_or(0, |index| index + 1);
    let receiver = &without_dot[start..];
    (!receiver.is_empty()).then_some(receiver)
}

fn public_members(module: &Module, statements: &[NodeId]) -> HashMap<String, IndexedSymbol> {
    let mut members = HashMap::new();
    for statement in statements {
        match &module.node(*statement).kind {
            NodeKind::Function {
                visibility: Visibility::Public,
                name,
                parameter,
                ..
            } => {
                members.insert(
                    name.text.to_string(),
                    IndexedSymbol {
                        name: name.text.to_string(),
                        span: name.span,
                        kind: SymbolKind::METHOD,
                        detail: Some(parameter_list(parameter)),
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
            "pub val answer 42\nval private 0\npub function read (value) { value }\n",
        )
        .expect("document");
        assert!(document.exports.contains_key("answer"));
        assert!(document.exports.contains_key("read"));
        assert!(!document.exports.contains_key("private"));
    }

    #[test]
    fn indexes_public_namespace_members() {
        let document = index_document(
            "pub val Point {\n    pub val x 0\n    val hidden 1\n    pub function move (amount) { amount }\n}\n",
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
            "pub val answer 42\npub function double (value) { * value 2 }\n",
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
        let double_span = symbol.span;
        let library_uri =
            Url::from_file_path(std::fs::canonicalize(&library).expect("library path"))
                .expect("library uri");
        assert_eq!(uri, &library_uri);
        let occurrences = index.occurrences(&library_uri, double_span, true);
        assert_eq!(occurrences.len(), 3);
        assert_eq!(
            occurrences
                .iter()
                .filter(|occurrence| occurrence.uri == main_uri)
                .count(),
            2
        );

        std::fs::remove_dir_all(&root).expect("remove temporary workspace");
    }
}
