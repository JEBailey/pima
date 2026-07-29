use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use pima::{
    diagnostic::{Diagnostic as PimaDiagnostic, Severity},
    source::{SourceMap, Span},
    syntax::{
        ast::{Module, NodeId, NodeKind, Pattern},
        lexer::lex,
        parser::parse,
        token::{Token, TokenKind},
    },
};
use tower_lsp::{
    Client, LanguageServer,
    jsonrpc::Result,
    lsp_types::{
        CompletionItem, CompletionItemKind, CompletionOptions, CompletionParams,
        CompletionResponse, Diagnostic, DiagnosticSeverity, DidChangeTextDocumentParams,
        DidCloseTextDocumentParams, DidOpenTextDocumentParams, DocumentSymbol,
        DocumentSymbolParams, DocumentSymbolResponse, Hover, HoverContents, HoverParams,
        InitializeParams, InitializeResult, InitializedParams, MarkedString, MessageType, OneOf,
        Position, Range, ServerCapabilities, SymbolKind, TextDocumentSyncCapability,
        TextDocumentSyncKind, Url,
    },
};

pub struct Backend {
    client: Client,
    documents: Arc<RwLock<HashMap<Url, String>>>,
}

impl Backend {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            documents: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    async fn update(&self, uri: Url, text: String) {
        self.documents
            .write()
            .expect("document lock poisoned")
            .insert(uri.clone(), text.clone());
        let diagnostics = analyze(&text).diagnostics;
        self.client
            .publish_diagnostics(uri, diagnostics, None)
            .await;
    }

    fn text(&self, uri: &Url) -> Option<String> {
        self.documents
            .read()
            .expect("document lock poisoned")
            .get(uri)
            .cloned()
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                hover_provider: Some(tower_lsp::lsp_types::HoverProviderCapability::Simple(true)),
                completion_provider: Some(CompletionOptions::default()),
                document_symbol_provider: Some(OneOf::Left(true)),
                ..ServerCapabilities::default()
            },
            server_info: Some(tower_lsp::lsp_types::ServerInfo {
                name: "Pima Language Server".into(),
                version: Some(env!("CARGO_PKG_VERSION").into()),
            }),
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "Pima language server initialized")
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        self.update(params.text_document.uri, params.text_document.text)
            .await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        if let Some(change) = params.content_changes.into_iter().last() {
            self.update(params.text_document.uri, change.text).await;
        }
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        self.documents
            .write()
            .expect("document lock poisoned")
            .remove(&params.text_document.uri);
        self.client
            .publish_diagnostics(params.text_document.uri, Vec::new(), None)
            .await;
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let document = params.text_document_position_params.text_document;
        let position = params.text_document_position_params.position;
        let Some(text) = self.text(&document.uri) else {
            return Ok(None);
        };
        let Some(offset) = position_to_offset(&text, position) else {
            return Ok(None);
        };
        let analysis = analyze(&text);
        let Some(token) = analysis
            .tokens
            .iter()
            .find(|token| token.span.start <= offset && offset < token.span.end)
        else {
            return Ok(None);
        };
        let description = describe_token(&token.kind);
        Ok(description.map(|value| Hover {
            contents: HoverContents::Scalar(MarkedString::String(value)),
            range: Some(span_to_range(&text, token.span)),
        }))
    }

    async fn completion(&self, _: CompletionParams) -> Result<Option<CompletionResponse>> {
        Ok(Some(CompletionResponse::Array(completions())))
    }

    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> Result<Option<DocumentSymbolResponse>> {
        let Some(text) = self.text(&params.text_document.uri) else {
            return Ok(None);
        };
        let analysis = analyze(&text);
        Ok(analysis
            .module
            .map(|module| DocumentSymbolResponse::Nested(document_symbols(&text, &module))))
    }
}

struct Analysis {
    tokens: Vec<Token>,
    module: Option<Module>,
    diagnostics: Vec<Diagnostic>,
}

fn analyze(text: &str) -> Analysis {
    let mut sources = SourceMap::default();
    let source = sources.add("<language-server>", text);
    match lex(source, text) {
        Ok(tokens) => match parse(&tokens) {
            Ok(module) => Analysis {
                tokens,
                module: Some(module),
                diagnostics: Vec::new(),
            },
            Err(errors) => Analysis {
                tokens,
                module: None,
                diagnostics: errors.iter().map(|error| diagnostic(text, error)).collect(),
            },
        },
        Err(errors) => Analysis {
            tokens: Vec::new(),
            module: None,
            diagnostics: errors.iter().map(|error| diagnostic(text, error)).collect(),
        },
    }
}

fn diagnostic(text: &str, value: &PimaDiagnostic) -> Diagnostic {
    let range = value
        .primary_span
        .map(|span| span_to_range(text, span))
        .unwrap_or_default();
    Diagnostic {
        range,
        severity: Some(match value.severity {
            Severity::Error => DiagnosticSeverity::ERROR,
            Severity::Warning => DiagnosticSeverity::WARNING,
        }),
        source: Some("pima".into()),
        message: value.message.clone(),
        ..Diagnostic::default()
    }
}

fn document_symbols(text: &str, module: &Module) -> Vec<DocumentSymbol> {
    module
        .statements
        .iter()
        .filter_map(|id| symbol_for_node(text, module, *id))
        .collect()
}

#[allow(deprecated)]
fn symbol_for_node(text: &str, module: &Module, id: NodeId) -> Option<DocumentSymbol> {
    let node = module.node(id);
    let (name, kind) = match &node.kind {
        NodeKind::Function { name, .. } => (name.to_string(), SymbolKind::FUNCTION),
        NodeKind::Binding { pattern, .. } => (pattern_name(pattern)?, SymbolKind::VARIABLE),
        _ => return None,
    };
    let range = span_to_range(text, node.span);
    Some(DocumentSymbol {
        name,
        detail: None,
        kind,
        tags: None,
        deprecated: None,
        range,
        selection_range: range,
        children: None,
    })
}

fn pattern_name(pattern: &Pattern) -> Option<String> {
    match pattern {
        Pattern::Capture(name) => Some(name.to_string()),
        Pattern::List(items) => Some(format!(
            "({})",
            items
                .iter()
                .filter_map(pattern_name)
                .collect::<Vec<_>>()
                .join(" ")
        )),
        Pattern::Wildcard | Pattern::Literal(_) => None,
    }
}

fn describe_token(kind: &TokenKind) -> Option<String> {
    Some(match kind {
        TokenKind::Identifier(name) => format!("Pima identifier `{name}`"),
        TokenKind::Symbol(name) => format!("Pima symbol `:{name}`"),
        TokenKind::Keyword(keyword) => format!("Pima keyword `{keyword:?}`").to_lowercase(),
        TokenKind::Boolean(_) => "Pima boolean".into(),
        TokenKind::Integer(_) => "Pima integer".into(),
        TokenKind::Float(_) => "Pima float".into(),
        TokenKind::String(_) => "Pima string".into(),
        TokenKind::ImportPath(_) => "Pima import path".into(),
        TokenKind::LeftBrace => "Code block: an expression executed with `do`.".into(),
        TokenKind::At => "Context annotation: `@(symbols) { block }`.".into(),
        _ => return None,
    })
}

fn completions() -> Vec<CompletionItem> {
    const KEYWORDS: &[&str] = &[
        "attempt", "break", "continue", "do", "function", "if", "import", "let", "match", "new",
        "pub", "return", "set", "throw", "until", "var", "while",
    ];
    KEYWORDS
        .iter()
        .map(|label| CompletionItem {
            label: (*label).into(),
            kind: Some(CompletionItemKind::KEYWORD),
            ..CompletionItem::default()
        })
        .collect()
}

fn span_to_range(text: &str, span: Span) -> Range {
    Range::new(
        offset_to_position(text, span.start),
        offset_to_position(text, span.end),
    )
}

fn offset_to_position(text: &str, offset: usize) -> Position {
    let offset = offset.min(text.len());
    let prefix = &text[..text.floor_char_boundary(offset)];
    let line = prefix.bytes().filter(|byte| *byte == b'\n').count() as u32;
    let line_start = prefix.rfind('\n').map_or(0, |index| index + 1);
    let character = text[line_start..prefix.len()].encode_utf16().count() as u32;
    Position::new(line, character)
}

fn position_to_offset(text: &str, position: Position) -> Option<usize> {
    let mut line_start = 0;
    for _ in 0..position.line {
        line_start += text[line_start..].find('\n')? + 1;
    }
    let line = text[line_start..].split('\n').next().unwrap_or_default();
    let mut utf16 = 0_u32;
    for (byte, ch) in line.char_indices() {
        if utf16 == position.character {
            return Some(line_start + byte);
        }
        utf16 += ch.len_utf16() as u32;
        if utf16 > position.character {
            return None;
        }
    }
    (utf16 == position.character).then_some(line_start + line.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_parser_diagnostics() {
        let result = analyze("set value\n");
        assert_eq!(result.diagnostics.len(), 1);
        assert_eq!(result.diagnostics[0].source.as_deref(), Some("pima"));
    }

    #[test]
    fn extracts_top_level_symbols() {
        let result = analyze("set answer 42\nfunction double (:x) {\n  * x 2\n}\n");
        let module = result.module.expect("valid module");
        let symbols = document_symbols(
            "set answer 42\nfunction double (:x) {\n  * x 2\n}\n",
            &module,
        );
        assert_eq!(symbols.len(), 2);
        assert_eq!(symbols[0].name, "answer");
        assert_eq!(symbols[1].name, "double");
    }

    #[test]
    fn positions_use_utf16_code_units() {
        let text = "\"😀\" value";
        assert_eq!(offset_to_position(text, 6), Position::new(0, 4));
        assert_eq!(position_to_offset(text, Position::new(0, 4)), Some(6));
    }
}
