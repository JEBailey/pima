use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use pima::{
    diagnostic::{Diagnostic as PimaDiagnostic, Severity},
    source::{SourceMap, Span},
    syntax::{
        ast::{Module, NodeId, NodeKind},
        lexer::lex,
        parser::parse_recovering,
        token::{Token, TokenKind},
    },
};
use tower_lsp::{
    Client, LanguageServer,
    jsonrpc::Result,
    lsp_types::{
        CodeAction, CodeActionKind, CodeActionOrCommand, CodeActionParams,
        CodeActionProviderCapability, CompletionItem, CompletionItemKind, CompletionOptions,
        CompletionParams, CompletionResponse, Diagnostic, DiagnosticSeverity,
        DidChangeTextDocumentParams, DidChangeWatchedFilesParams,
        DidChangeWatchedFilesRegistrationOptions, DidCloseTextDocumentParams,
        DidOpenTextDocumentParams, DocumentFormattingParams, DocumentSymbol, DocumentSymbolParams,
        DocumentSymbolResponse, FileChangeType, FileSystemWatcher, FoldingRange,
        FoldingRangeParams, FoldingRangeProviderCapability, GlobPattern, GotoDefinitionParams,
        GotoDefinitionResponse, Hover, HoverContents, HoverParams, InitializeParams,
        InitializeResult, InitializedParams, InlayHint, InlayHintKind, InlayHintLabel,
        InlayHintParams, Location, MarkedString, MessageType, OneOf, Position,
        PrepareRenameResponse, Range, ReferenceParams, Registration, RenameOptions, RenameParams,
        SelectionRange, SelectionRangeParams, SelectionRangeProviderCapability, SemanticToken,
        SemanticTokenModifier, SemanticTokenType, SemanticTokens, SemanticTokensFullOptions,
        SemanticTokensLegend, SemanticTokensOptions, SemanticTokensParams, SemanticTokensResult,
        SemanticTokensServerCapabilities, ServerCapabilities, SignatureHelp, SignatureHelpOptions,
        SignatureHelpParams, SignatureInformation, SymbolKind, TextDocumentSyncCapability,
        TextDocumentSyncKind, TextEdit, Url, WorkspaceEdit,
    },
};

use crate::{
    ast_utils::{namespace_block, parameter_list, pattern_captures},
    catalog, formatting,
    semantic::Symbol,
    semantic::{IssueSeverity, SemanticModel, SymbolKind as SemanticSymbolKind},
    workspace::{IndexedSymbol, WorkspaceIndex},
};

pub struct Backend {
    client: Client,
    documents: Arc<RwLock<HashMap<Url, String>>>,
    analyses: Arc<RwLock<HashMap<Url, Arc<Analysis>>>>,
    versions: Arc<RwLock<HashMap<Url, i32>>>,
    workspace: Arc<RwLock<WorkspaceIndex>>,
}

impl Backend {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            documents: Arc::new(RwLock::new(HashMap::new())),
            analyses: Arc::new(RwLock::new(HashMap::new())),
            versions: Arc::new(RwLock::new(HashMap::new())),
            workspace: Arc::new(RwLock::new(WorkspaceIndex::default())),
        }
    }

    async fn update(&self, uri: Url, text: String, version: i32) {
        let analysis = Arc::new(analyze(&text));
        self.documents
            .write()
            .expect("document lock poisoned")
            .insert(uri.clone(), text.clone());
        self.analyses
            .write()
            .expect("analysis lock poisoned")
            .insert(uri.clone(), analysis.clone());
        self.versions
            .write()
            .expect("version lock poisoned")
            .insert(uri.clone(), version);
        self.workspace
            .write()
            .expect("workspace lock poisoned")
            .upsert(uri.clone(), text.clone());
        self.client
            .publish_diagnostics(uri, analysis.diagnostics.clone(), None)
            .await;
    }

    fn queue_change(&self, uri: Url, text: String, version: i32) {
        {
            let mut versions = self.versions.write().expect("version lock poisoned");
            if versions
                .get(&uri)
                .is_some_and(|current| *current >= version)
            {
                return;
            }
            versions.insert(uri.clone(), version);
        }
        self.documents
            .write()
            .expect("document lock poisoned")
            .insert(uri.clone(), text.clone());
        self.analyses
            .write()
            .expect("analysis lock poisoned")
            .remove(&uri);
        self.workspace
            .write()
            .expect("workspace lock poisoned")
            .upsert(uri.clone(), text);

        let client = self.client.clone();
        let documents = self.documents.clone();
        let analyses = self.analyses.clone();
        let versions = self.versions.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(75)).await;
            if versions.read().expect("version lock poisoned").get(&uri) != Some(&version) {
                return;
            }
            let Some(text) = documents
                .read()
                .expect("document lock poisoned")
                .get(&uri)
                .cloned()
            else {
                return;
            };
            let analysis = Arc::new(analyze(&text));
            if versions.read().expect("version lock poisoned").get(&uri) != Some(&version) {
                return;
            }
            let diagnostics = analysis.diagnostics.clone();
            analyses
                .write()
                .expect("analysis lock poisoned")
                .insert(uri.clone(), analysis);
            client.publish_diagnostics(uri, diagnostics, None).await;
        });
    }

    fn text(&self, uri: &Url) -> Option<String> {
        self.documents
            .read()
            .expect("document lock poisoned")
            .get(uri)
            .cloned()
    }

    fn analysis(&self, uri: &Url, text: &str) -> Arc<Analysis> {
        if let Some(analysis) = self
            .analyses
            .read()
            .expect("analysis lock poisoned")
            .get(uri)
            .cloned()
        {
            return analysis;
        }
        let analysis = Arc::new(analyze(text));
        self.analyses
            .write()
            .expect("analysis lock poisoned")
            .insert(uri.clone(), analysis.clone());
        analysis
    }

    fn symbol_at(
        &self,
        uri: &Url,
        position: Position,
    ) -> Option<(String, SemanticModel, crate::semantic::SymbolId)> {
        let text = self.text(uri)?;
        let offset = position_to_offset(&text, position)?;
        let analysis = self.analysis(uri, &text);
        let model = analysis.semantic.clone()?;
        let symbol = model.symbol_at(offset)?;
        Some((text, model, symbol))
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        let roots = params
            .workspace_folders
            .map(|folders| folders.into_iter().map(|folder| folder.uri).collect())
            .or_else(|| params.root_uri.map(|root| vec![root]))
            .unwrap_or_default();
        self.workspace
            .write()
            .expect("workspace lock poisoned")
            .scan(&roots);
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                hover_provider: Some(tower_lsp::lsp_types::HoverProviderCapability::Simple(true)),
                completion_provider: Some(CompletionOptions::default()),
                document_symbol_provider: Some(OneOf::Left(true)),
                definition_provider: Some(OneOf::Left(true)),
                references_provider: Some(OneOf::Left(true)),
                rename_provider: Some(OneOf::Right(RenameOptions {
                    prepare_provider: Some(true),
                    work_done_progress_options: Default::default(),
                })),
                signature_help_provider: Some(SignatureHelpOptions {
                    trigger_characters: Some(vec![" ".into(), "[".into()]),
                    retrigger_characters: None,
                    work_done_progress_options: Default::default(),
                }),
                semantic_tokens_provider: Some(
                    SemanticTokensServerCapabilities::SemanticTokensOptions(
                        SemanticTokensOptions {
                            legend: semantic_legend(),
                            range: None,
                            full: Some(SemanticTokensFullOptions::Bool(true)),
                            work_done_progress_options: Default::default(),
                        },
                    ),
                ),
                folding_range_provider: Some(FoldingRangeProviderCapability::Simple(true)),
                selection_range_provider: Some(SelectionRangeProviderCapability::Simple(true)),
                inlay_hint_provider: Some(OneOf::Left(true)),
                document_formatting_provider: Some(OneOf::Left(true)),
                code_action_provider: Some(CodeActionProviderCapability::Simple(true)),
                ..ServerCapabilities::default()
            },
            server_info: Some(tower_lsp::lsp_types::ServerInfo {
                name: "Pima Language Server".into(),
                version: Some(env!("CARGO_PKG_VERSION").into()),
            }),
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        let options = DidChangeWatchedFilesRegistrationOptions {
            watchers: vec![FileSystemWatcher {
                glob_pattern: GlobPattern::String("**/*.pima".into()),
                kind: None,
            }],
        };
        if let Ok(register_options) = serde_json::to_value(options) {
            let client = self.client.clone();
            tokio::spawn(async move {
                let _ = client
                    .register_capability(vec![Registration {
                        id: "pima-workspace-files".into(),
                        method: "workspace/didChangeWatchedFiles".into(),
                        register_options: Some(register_options),
                    }])
                    .await;
            });
        }
        let snapshots = self
            .workspace
            .read()
            .expect("workspace lock poisoned")
            .snapshots();
        for (uri, text) in snapshots {
            let diagnostics = analyze(&text).diagnostics;
            self.client
                .publish_diagnostics(uri, diagnostics, None)
                .await;
        }
        self.client
            .log_message(MessageType::INFO, "Pima language server initialized")
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        self.update(
            params.text_document.uri,
            params.text_document.text,
            params.text_document.version,
        )
        .await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        if let Some(change) = params.content_changes.into_iter().last() {
            self.queue_change(
                params.text_document.uri,
                change.text,
                params.text_document.version,
            );
        }
    }

    async fn did_change_watched_files(&self, params: DidChangeWatchedFilesParams) {
        for change in params.changes {
            if self
                .documents
                .read()
                .expect("document lock poisoned")
                .contains_key(&change.uri)
            {
                continue;
            }
            if change.typ == FileChangeType::DELETED {
                self.workspace
                    .write()
                    .expect("workspace lock poisoned")
                    .remove(&change.uri);
                self.client
                    .publish_diagnostics(change.uri, Vec::new(), None)
                    .await;
                continue;
            }
            let Ok(path) = change.uri.to_file_path() else {
                continue;
            };
            let Ok(text) = std::fs::read_to_string(path) else {
                continue;
            };
            self.workspace
                .write()
                .expect("workspace lock poisoned")
                .upsert(change.uri.clone(), text.clone());
            let diagnostics = analyze(&text).diagnostics;
            self.client
                .publish_diagnostics(change.uri, diagnostics, None)
                .await;
        }
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        self.documents
            .write()
            .expect("document lock poisoned")
            .remove(&params.text_document.uri);
        self.analyses
            .write()
            .expect("analysis lock poisoned")
            .remove(&params.text_document.uri);
        self.versions
            .write()
            .expect("version lock poisoned")
            .remove(&params.text_document.uri);
        {
            let uri = &params.text_document.uri;
            let mut workspace = self.workspace.write().expect("workspace lock poisoned");
            if let Ok(path) = uri.to_file_path()
                && let Ok(text) = std::fs::read_to_string(path)
            {
                workspace.upsert(uri.clone(), text);
            } else {
                workspace.remove(uri);
            }
        }
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
        let analysis = self.analysis(&document.uri, &text);
        if let Some((member, span)) = catalog_member_at(&text, &analysis, offset) {
            return Ok(Some(Hover {
                contents: HoverContents::Scalar(MarkedString::String(member.signature.into())),
                range: Some(span_to_range(&text, span)),
            }));
        }
        if let Some((model, symbol_id)) = analysis
            .semantic
            .as_ref()
            .and_then(|model| model.symbol_at(offset).map(|symbol| (model, symbol)))
        {
            let symbol = &model.symbols[symbol_id];
            let range = model
                .occurrence_span(symbol_id, offset)
                .unwrap_or(symbol.declaration);
            return Ok(Some(Hover {
                contents: HoverContents::Scalar(MarkedString::String(format!(
                    "{} `{}`{}",
                    symbol.kind.description(),
                    symbol.name,
                    symbol
                        .inferred_type
                        .map(|kind| format!(" : {kind}"))
                        .unwrap_or_default()
                ))),
                range: Some(span_to_range(&text, range)),
            }));
        }
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

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let position = params.text_document_position.position;
        let uri = params.text_document_position.text_document.uri;
        let Some(text) = self.text(&uri) else {
            return Ok(None);
        };
        let Some(offset) = position_to_offset(&text, position) else {
            return Ok(None);
        };
        let analysis = self.analysis(&uri, &text);
        if let Some(namespace) = member_receiver(&text, offset)
            && let Some(members) = receiver_members(namespace, offset, &analysis)
        {
            return Ok(Some(CompletionResponse::Array(
                members
                    .iter()
                    .map(|member| CompletionItem {
                        label: member.name.into(),
                        kind: Some(
                            if member
                                .name
                                .chars()
                                .all(|character| character.is_ascii_uppercase() || character == '_')
                            {
                                CompletionItemKind::CONSTANT
                            } else {
                                CompletionItemKind::METHOD
                            },
                        ),
                        detail: Some(member.signature.into()),
                        ..CompletionItem::default()
                    })
                    .collect(),
            )));
        }
        if let Some(receiver) = member_receiver(&text, offset) {
            let items = self
                .workspace
                .read()
                .expect("workspace lock poisoned")
                .member_completions(&uri, receiver)
                .into_iter()
                .map(indexed_completion)
                .collect::<Vec<_>>();
            if !items.is_empty() {
                return Ok(Some(CompletionResponse::Array(items)));
            }
        }

        let mut items = keyword_completions();
        if let Some(model) = analysis.semantic.as_ref() {
            items.extend(
                model
                    .visible_symbols_at(offset)
                    .into_iter()
                    .map(|symbol| symbol_completion(&model.symbols[symbol])),
            );
        }
        if text.contains("/pima/library/standard") {
            items.extend(catalog::NAMESPACES.iter().map(|namespace| CompletionItem {
                label: (*namespace).into(),
                kind: Some(CompletionItemKind::MODULE),
                ..CompletionItem::default()
            }));
        }
        {
            let workspace = self.workspace.read().expect("workspace lock poisoned");
            items.extend(
                workspace
                    .imported_completions(&uri)
                    .into_iter()
                    .map(indexed_completion),
            );
            items.extend(
                workspace
                    .import_aliases(&uri)
                    .into_iter()
                    .map(|alias| CompletionItem {
                        label: alias.into(),
                        kind: Some(CompletionItemKind::MODULE),
                        ..CompletionItem::default()
                    }),
            );
        }
        Ok(Some(CompletionResponse::Array(items)))
    }

    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> Result<Option<DocumentSymbolResponse>> {
        let Some(text) = self.text(&params.text_document.uri) else {
            return Ok(None);
        };
        let analysis = self.analysis(&params.text_document.uri, &text);
        Ok(analysis
            .module
            .as_ref()
            .map(|module| DocumentSymbolResponse::Nested(document_symbols(&text, module))))
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let position = params.text_document_position_params.position;
        let uri = params.text_document_position_params.text_document.uri;
        let Some(text) = self.text(&uri) else {
            return Ok(None);
        };
        let Some(offset) = position_to_offset(&text, position) else {
            return Ok(None);
        };
        let analysis = self.analysis(&uri, &text);
        if let Some((model, symbol)) = analysis
            .semantic
            .as_ref()
            .and_then(|model| model.symbol_at(offset).map(|symbol| (model, symbol)))
        {
            return Ok(Some(GotoDefinitionResponse::Scalar(Location::new(
                uri,
                span_to_range(&text, model.symbols[symbol].declaration),
            ))));
        }
        let Some(token) = analysis.tokens.iter().find(|token| {
            token.span.start <= offset
                && offset < token.span.end
                && matches!(token.kind, TokenKind::Identifier(_))
        }) else {
            return Ok(None);
        };
        let TokenKind::Identifier(name) = &token.kind else {
            return Ok(None);
        };
        let receiver = member_receiver(&text, token.span.start);
        let workspace = self.workspace.read().expect("workspace lock poisoned");
        let Some((target_uri, target, symbol)) = workspace.definition(&uri, name, receiver) else {
            return Ok(None);
        };
        Ok(Some(GotoDefinitionResponse::Scalar(Location::new(
            target_uri.clone(),
            span_to_range(&target.text, symbol.span),
        ))))
    }

    async fn references(&self, params: ReferenceParams) -> Result<Option<Vec<Location>>> {
        let position = params.text_document_position.position;
        let uri = params.text_document_position.text_document.uri;
        let Some(text) = self.text(&uri) else {
            return Ok(None);
        };
        let Some(offset) = position_to_offset(&text, position) else {
            return Ok(None);
        };
        if let Some((_, model, symbol)) = self.symbol_at(&uri, position) {
            let mut locations = model
                .reference_spans(symbol, params.context.include_declaration)
                .into_iter()
                .map(|span| Location::new(uri.clone(), span_to_range(&text, span)))
                .collect::<Vec<_>>();
            let workspace = self.workspace.read().expect("workspace lock poisoned");
            if let Some((target_uri, target)) =
                workspace.target_at(&uri, model.symbols[symbol].declaration.start)
            {
                locations.extend(
                    workspace
                        .occurrences(target_uri, target.span, false)
                        .into_iter()
                        .map(|item| Location::new(item.uri, span_to_range(&item.text, item.span))),
                );
            }
            return Ok(Some(locations));
        }
        let workspace = self.workspace.read().expect("workspace lock poisoned");
        let Some((target_uri, target)) = workspace.target_at(&uri, offset) else {
            return Ok(None);
        };
        Ok(Some(
            workspace
                .occurrences(target_uri, target.span, params.context.include_declaration)
                .into_iter()
                .map(|item| Location::new(item.uri, span_to_range(&item.text, item.span)))
                .collect(),
        ))
    }

    async fn prepare_rename(
        &self,
        params: tower_lsp::lsp_types::TextDocumentPositionParams,
    ) -> Result<Option<PrepareRenameResponse>> {
        let uri = params.text_document.uri;
        let Some(text) = self.text(&uri) else {
            return Ok(None);
        };
        let Some(offset) = position_to_offset(&text, params.position) else {
            return Ok(None);
        };
        if let Some((_, model, symbol)) = self.symbol_at(&uri, params.position) {
            let symbol = &model.symbols[symbol];
            return Ok(Some(PrepareRenameResponse::RangeWithPlaceholder {
                range: span_to_range(&text, symbol.declaration),
                placeholder: symbol.name.clone(),
            }));
        }
        let workspace = self.workspace.read().expect("workspace lock poisoned");
        let Some((_, symbol)) = workspace.target_at(&uri, offset) else {
            return Ok(None);
        };
        let analysis = self.analysis(&uri, &text);
        let Some(token) = analysis
            .tokens
            .iter()
            .find(|token| token.span.start <= offset && offset < token.span.end)
        else {
            return Ok(None);
        };
        Ok(Some(PrepareRenameResponse::RangeWithPlaceholder {
            range: span_to_range(&text, token.span),
            placeholder: symbol.name.clone(),
        }))
    }

    async fn rename(&self, params: RenameParams) -> Result<Option<WorkspaceEdit>> {
        if !valid_identifier(&params.new_name) {
            return Err(tower_lsp::jsonrpc::Error::invalid_params(
                "new name is not a valid Pima identifier",
            ));
        }
        let uri = params.text_document_position.text_document.uri;
        let Some(text) = self.text(&uri) else {
            return Ok(None);
        };
        let Some(offset) = position_to_offset(&text, params.text_document_position.position) else {
            return Ok(None);
        };
        let local = self.symbol_at(&uri, params.text_document_position.position);
        let workspace = self.workspace.read().expect("workspace lock poisoned");
        if let Some((_, model, symbol)) = &local
            && workspace
                .target_at(&uri, model.symbols[*symbol].declaration.start)
                .is_none()
        {
            let edits = model
                .reference_spans(*symbol, true)
                .into_iter()
                .map(|span| rename_edit(&text, span, &params.new_name))
                .collect();
            return Ok(Some(WorkspaceEdit {
                changes: Some(HashMap::from([(uri, edits)])),
                ..WorkspaceEdit::default()
            }));
        }
        let target = if let Some((_, model, symbol)) = &local {
            workspace.target_at(&uri, model.symbols[*symbol].declaration.start)
        } else {
            workspace.target_at(&uri, offset)
        };
        let Some((target_uri, target)) = target else {
            return Ok(None);
        };
        let mut changes = HashMap::<Url, Vec<TextEdit>>::new();
        for item in workspace.occurrences(target_uri, target.span, true) {
            changes.entry(item.uri).or_default().push(rename_edit(
                &item.text,
                item.span,
                &params.new_name,
            ));
        }
        Ok(Some(WorkspaceEdit {
            changes: Some(changes),
            ..WorkspaceEdit::default()
        }))
    }

    async fn signature_help(&self, params: SignatureHelpParams) -> Result<Option<SignatureHelp>> {
        let uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;
        let Some(text) = self.text(&uri) else {
            return Ok(None);
        };
        let Some(offset) = position_to_offset(&text, position) else {
            return Ok(None);
        };
        let analysis = self.analysis(&uri, &text);
        Ok(
            signature_at(&text, &analysis, offset).map(|(label, parameters, active_parameter)| {
                SignatureHelp {
                    signatures: vec![SignatureInformation {
                        label,
                        documentation: None,
                        parameters: Some(
                            parameters
                                .into_iter()
                                .map(|parameter| tower_lsp::lsp_types::ParameterInformation {
                                    label: tower_lsp::lsp_types::ParameterLabel::Simple(parameter),
                                    documentation: None,
                                })
                                .collect(),
                        ),
                        active_parameter: Some(active_parameter),
                    }],
                    active_signature: Some(0),
                    active_parameter: Some(active_parameter),
                }
            }),
        )
    }

    async fn semantic_tokens_full(
        &self,
        params: SemanticTokensParams,
    ) -> Result<Option<SemanticTokensResult>> {
        let Some(text) = self.text(&params.text_document.uri) else {
            return Ok(None);
        };
        let analysis = self.analysis(&params.text_document.uri, &text);
        Ok(Some(SemanticTokensResult::Tokens(SemanticTokens {
            result_id: None,
            data: semantic_tokens(&text, &analysis),
        })))
    }

    async fn folding_range(&self, params: FoldingRangeParams) -> Result<Option<Vec<FoldingRange>>> {
        let Some(text) = self.text(&params.text_document.uri) else {
            return Ok(None);
        };
        let analysis = self.analysis(&params.text_document.uri, &text);
        Ok(analysis
            .module
            .as_ref()
            .map(|module| folding_ranges(&text, module)))
    }

    async fn selection_range(
        &self,
        params: SelectionRangeParams,
    ) -> Result<Option<Vec<SelectionRange>>> {
        let Some(text) = self.text(&params.text_document.uri) else {
            return Ok(None);
        };
        let analysis = self.analysis(&params.text_document.uri, &text);
        let Some(module) = analysis.module.as_ref() else {
            return Ok(None);
        };
        Ok(Some(
            params
                .positions
                .into_iter()
                .filter_map(|position| {
                    position_to_offset(&text, position)
                        .map(|offset| selection_at(&text, module, &analysis.tokens, offset))
                })
                .collect(),
        ))
    }

    async fn inlay_hint(&self, params: InlayHintParams) -> Result<Option<Vec<InlayHint>>> {
        let Some(text) = self.text(&params.text_document.uri) else {
            return Ok(None);
        };
        let analysis = self.analysis(&params.text_document.uri, &text);
        let Some(module) = analysis.module.as_ref() else {
            return Ok(None);
        };
        Ok(Some(inlay_hints(
            &text,
            module,
            analysis.semantic.as_ref(),
            params.range,
        )))
    }

    async fn formatting(&self, params: DocumentFormattingParams) -> Result<Option<Vec<TextEdit>>> {
        let Some(text) = self.text(&params.text_document.uri) else {
            return Ok(None);
        };
        let Some(formatted) = formatting::format(&text, params.options.tab_size as usize) else {
            return Ok(None);
        };
        if formatted == text {
            return Ok(Some(Vec::new()));
        }
        Ok(Some(vec![TextEdit::new(
            Range::new(Position::new(0, 0), offset_to_position(&text, text.len())),
            formatted,
        )]))
    }

    async fn code_action(
        &self,
        params: CodeActionParams,
    ) -> Result<Option<Vec<CodeActionOrCommand>>> {
        let uri = params.text_document.uri;
        let Some(text) = self.text(&uri) else {
            return Ok(None);
        };
        let start = position_to_offset(&text, params.range.start).unwrap_or(0);
        let end = position_to_offset(&text, params.range.end).unwrap_or(text.len());
        let analysis = self.analysis(&uri, &text);
        let Some(model) = analysis.semantic.as_ref() else {
            return Ok(None);
        };
        let mut actions = Vec::new();
        for (symbol_id, symbol) in model.symbols.iter().enumerate() {
            if symbol.declaration.end < start
                || symbol.declaration.start > end
                || !matches!(
                    symbol.kind,
                    SemanticSymbolKind::Function
                        | SemanticSymbolKind::Parameter
                        | SemanticSymbolKind::PatternCapture
                )
                || !symbol
                    .name
                    .chars()
                    .any(|character| character.is_ascii_uppercase())
            {
                continue;
            }
            let replacement_name = to_snake_case(&symbol.name);
            let edits = model
                .reference_spans(symbol_id, true)
                .into_iter()
                .map(|span| {
                    let replacement = if text
                        .get(span.start..span.end)
                        .is_some_and(|source| source.starts_with(':'))
                    {
                        format!(":{replacement_name}")
                    } else {
                        replacement_name.clone()
                    };
                    TextEdit::new(span_to_range(&text, span), replacement)
                })
                .collect();
            actions.push(CodeActionOrCommand::CodeAction(CodeAction {
                title: format!("Rename `{}` to `{replacement_name}`", symbol.name),
                kind: Some(CodeActionKind::QUICKFIX),
                diagnostics: Some(params.context.diagnostics.clone()),
                edit: Some(WorkspaceEdit {
                    changes: Some(HashMap::from([(uri.clone(), edits)])),
                    ..WorkspaceEdit::default()
                }),
                is_preferred: Some(true),
                disabled: None,
                command: None,
                data: None,
            }));
        }
        Ok(Some(actions))
    }
}

struct Analysis {
    tokens: Vec<Token>,
    module: Option<Module>,
    semantic: Option<SemanticModel>,
    diagnostics: Vec<Diagnostic>,
}

fn analyze(text: &str) -> Analysis {
    let mut sources = SourceMap::default();
    let source = sources.add("<language-server>", text);
    match lex(source, text) {
        Ok(tokens) => {
            let output = parse_recovering(&tokens);
            let semantic = SemanticModel::build(&output.module);
            let mut diagnostics = output
                .diagnostics
                .iter()
                .map(|error| diagnostic(text, error))
                .collect::<Vec<_>>();
            diagnostics.extend(
                semantic
                    .issues()
                    .cloned()
                    .chain(semantic.naming_issues())
                    .map(|issue| Diagnostic {
                        range: span_to_range(text, issue.span),
                        severity: Some(match issue.severity {
                            IssueSeverity::Error => DiagnosticSeverity::ERROR,
                            IssueSeverity::Warning => DiagnosticSeverity::WARNING,
                        }),
                        source: Some("pima".into()),
                        message: issue.message,
                        ..Diagnostic::default()
                    }),
            );
            Analysis {
                tokens,
                module: Some(output.module),
                semantic: Some(semantic),
                diagnostics,
            }
        }
        Err(errors) => Analysis {
            tokens: Vec::new(),
            module: None,
            semantic: None,
            diagnostics: errors.iter().map(|error| diagnostic(text, error)).collect(),
        },
    }
}

const KEYWORDS: &[&str] = &[
    "as", "attempt", "await", "branch", "break", "continue", "do", "function", "if", "import",
    "let", "match", "new", "pub", "remote", "return", "throw", "until", "val", "var", "while",
];

fn valid_identifier(name: &str) -> bool {
    if KEYWORDS.contains(&name) {
        return false;
    }
    let mut characters = name.chars();
    let Some(first) = characters.next() else {
        return false;
    };
    let rest = characters.collect::<String>();
    let stem = rest.strip_suffix('?').unwrap_or(&rest);
    (first.is_ascii_alphabetic() || first == '_')
        && !stem.contains('?')
        && stem
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '_')
}

fn rename_edit(text: &str, span: Span, new_name: &str) -> TextEdit {
    let replacement = if text
        .get(span.start..span.end)
        .is_some_and(|source| source.starts_with(':'))
    {
        format!(":{new_name}")
    } else {
        new_name.to_owned()
    };
    TextEdit::new(span_to_range(text, span), replacement)
}

fn to_snake_case(name: &str) -> String {
    let characters = name.chars().collect::<Vec<_>>();
    let mut result = String::new();
    for (index, character) in characters.iter().copied().enumerate() {
        if character.is_ascii_uppercase() {
            let previous_is_lower_or_digit = index > 0
                && (characters[index - 1].is_ascii_lowercase()
                    || characters[index - 1].is_ascii_digit());
            let next_is_lower = characters
                .get(index + 1)
                .is_some_and(char::is_ascii_lowercase);
            if !result.is_empty()
                && !result.ends_with('_')
                && (previous_is_lower_or_digit || next_is_lower)
            {
                result.push('_');
            }
            result.push(character.to_ascii_lowercase());
        } else {
            result.push(character);
        }
    }
    result
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
    symbols_for_statements(text, module, &module.statements)
}

#[allow(deprecated)]
fn symbols_for_statements(
    text: &str,
    module: &Module,
    statements: &[NodeId],
) -> Vec<DocumentSymbol> {
    statements
        .iter()
        .flat_map(|id| symbols_for_node(text, module, *id))
        .collect()
}

#[allow(deprecated)]
fn symbols_for_node(text: &str, module: &Module, id: NodeId) -> Vec<DocumentSymbol> {
    let node = module.node(id);
    match &node.kind {
        NodeKind::Function {
            name,
            parameter,
            body,
            ..
        } => vec![make_document_symbol(
            text,
            name.text.to_string(),
            Some(parameter_list(parameter)),
            SymbolKind::FUNCTION,
            node.span,
            name.span,
            match module.node(*body).kind {
                NodeKind::Block(block) => {
                    symbols_for_statements(text, module, &module.block(block).statements)
                }
                _ => Vec::new(),
            },
        )],
        NodeKind::Binding { pattern, value, .. } => {
            let namespace = namespace_block(module, *value);
            let children = namespace
                .map(|block| symbols_for_statements(text, module, &module.block(block).statements))
                .unwrap_or_default();
            pattern_captures(pattern)
                .into_iter()
                .map(|name| {
                    make_document_symbol(
                        text,
                        name.text.to_string(),
                        None,
                        if namespace.is_some() {
                            SymbolKind::NAMESPACE
                        } else {
                            SymbolKind::VARIABLE
                        },
                        node.span,
                        name.span,
                        children.clone(),
                    )
                })
                .collect()
        }
        _ => Vec::new(),
    }
}

#[allow(deprecated)]
fn make_document_symbol(
    text: &str,
    name: String,
    detail: Option<String>,
    kind: SymbolKind,
    span: Span,
    selection_span: Span,
    children: Vec<DocumentSymbol>,
) -> DocumentSymbol {
    DocumentSymbol {
        name,
        detail,
        kind,
        tags: None,
        deprecated: None,
        range: span_to_range(text, span),
        selection_range: span_to_range(text, selection_span),
        children: (!children.is_empty()).then_some(children),
    }
}

fn describe_token(kind: &TokenKind) -> Option<String> {
    Some(match kind {
        TokenKind::Identifier(name) => format!("Pima identifier `{name}`"),
        TokenKind::Symbol(name) => format!("Pima symbol `:{name}`"),
        TokenKind::Keyword(keyword) => match keyword.as_str() {
            "remote" => "`remote`: construct a namespace in an isolated worker VM; member requests return futures.".into(),
            "await" => "`await`: wait for a future and produce its value or throw its error.".into(),
            name => format!("Pima keyword `{name}`"),
        },
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

fn keyword_completions() -> Vec<CompletionItem> {
    KEYWORDS
        .iter()
        .map(|label| CompletionItem {
            label: (*label).into(),
            kind: Some(CompletionItemKind::KEYWORD),
            ..CompletionItem::default()
        })
        .collect()
}

fn symbol_completion(symbol: &Symbol) -> CompletionItem {
    CompletionItem {
        label: symbol.name.clone(),
        kind: Some(match symbol.kind {
            SemanticSymbolKind::Function => CompletionItemKind::FUNCTION,
            SemanticSymbolKind::Binding
            | SemanticSymbolKind::Parameter
            | SemanticSymbolKind::PatternCapture => CompletionItemKind::VARIABLE,
        }),
        detail: Some(match symbol.inferred_type {
            Some(inferred) => format!("{} : {inferred}", symbol.kind.description()),
            None => symbol.kind.description().into(),
        }),
        ..CompletionItem::default()
    }
}

fn indexed_completion(symbol: &IndexedSymbol) -> CompletionItem {
    CompletionItem {
        label: symbol.name.clone(),
        kind: Some(match symbol.kind {
            SymbolKind::FUNCTION | SymbolKind::METHOD => CompletionItemKind::FUNCTION,
            SymbolKind::NAMESPACE | SymbolKind::MODULE => CompletionItemKind::MODULE,
            SymbolKind::CONSTANT => CompletionItemKind::CONSTANT,
            SymbolKind::FIELD | SymbolKind::PROPERTY => CompletionItemKind::FIELD,
            _ => CompletionItemKind::VARIABLE,
        }),
        detail: symbol.detail.clone(),
        ..CompletionItem::default()
    }
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

fn receiver_members(
    receiver: &str,
    offset: usize,
    analysis: &Analysis,
) -> Option<&'static [catalog::Member]> {
    catalog::namespace_members(receiver).or_else(|| {
        let model = analysis.semantic.as_ref()?;
        let symbol = model
            .visible_symbols_at(offset)
            .into_iter()
            .find(|symbol| model.symbols[*symbol].name == receiver)?;
        catalog::namespace_members(model.symbols[symbol].inferred_type?)
    })
}

fn catalog_member_at(
    text: &str,
    analysis: &Analysis,
    offset: usize,
) -> Option<(&'static catalog::Member, Span)> {
    let token = analysis.tokens.iter().find(|token| {
        token.span.start <= offset
            && offset < token.span.end
            && matches!(token.kind, TokenKind::Identifier(_))
    })?;
    let TokenKind::Identifier(member_name) = &token.kind else {
        return None;
    };
    let namespace = member_receiver(text, token.span.start)?;
    let member = receiver_members(namespace, token.span.start, analysis)?
        .iter()
        .find(|member| member.name == member_name.as_ref())?;
    Some((member, token.span))
}

fn signature_at(
    text: &str,
    analysis: &Analysis,
    offset: usize,
) -> Option<(String, Vec<String>, u32)> {
    let mut brackets = Vec::new();
    for (index, token) in analysis.tokens.iter().enumerate() {
        if token.span.start >= offset {
            break;
        }
        match token.kind {
            TokenKind::LeftBracket => brackets.push(index),
            TokenKind::RightBracket => {
                brackets.pop();
            }
            _ => {}
        }
    }
    let open = *brackets.last()?;
    let tokens = analysis.tokens[open + 1..]
        .iter()
        .take_while(|token| token.span.start < offset)
        .filter(|token| !matches!(token.kind, TokenKind::Eol | TokenKind::Eof))
        .collect::<Vec<_>>();
    let first = *tokens.first()?;
    let TokenKind::Identifier(callee_name) = &first.kind else {
        return None;
    };

    let (label, parameters, consumed) =
        if matches!(tokens.get(1).map(|token| &token.kind), Some(TokenKind::Dot)) {
            let member = tokens.get(2)?;
            let TokenKind::Identifier(member_name) = &member.kind else {
                return None;
            };
            let members = catalog::namespace_members(callee_name).or_else(|| {
                let model = analysis.semantic.as_ref()?;
                let symbol = model.symbol_at(first.span.start).or_else(|| {
                    model
                        .visible_symbols_at(first.span.start)
                        .into_iter()
                        .find(|symbol| model.symbols[*symbol].name == callee_name.as_ref())
                })?;
                catalog::namespace_members(model.symbols[symbol].inferred_type?)
            })?;
            let catalog_member = members
                .iter()
                .find(|candidate| candidate.name == member_name.as_ref())?;
            (
                catalog_member.signature.to_string(),
                signature_parameters(catalog_member.signature),
                3,
            )
        } else {
            let model = analysis.semantic.as_ref()?;
            let symbol = model.symbol_at(first.span.start).or_else(|| {
                model
                    .visible_symbols_at(first.span.start)
                    .into_iter()
                    .find(|symbol| model.symbols[*symbol].name == callee_name.as_ref())
            })?;
            let symbol = &model.symbols[symbol];
            if symbol.kind != SemanticSymbolKind::Function {
                return None;
            }
            (
                format!("{}({})", symbol.name, symbol.parameters.join(", ")),
                symbol.parameters.clone(),
                1,
            )
        };

    let arguments = &tokens[consumed.min(tokens.len())..];
    let count = top_level_expression_count(arguments);
    let trailing_space = arguments.last().is_none_or(|last| {
        text.get(last.span.end..offset)
            .is_some_and(|suffix| !suffix.is_empty() && suffix.chars().all(char::is_whitespace))
    });
    let active = if count == 0 {
        0
    } else if trailing_space {
        count
    } else {
        count - 1
    };
    let active = active.min(parameters.len().saturating_sub(1)) as u32;
    Some((label, parameters, active))
}

fn signature_parameters(signature: &str) -> Vec<String> {
    signature
        .split_once('(')
        .and_then(|(_, rest)| rest.strip_suffix(')'))
        .map(|parameters| {
            parameters
                .split(',')
                .map(str::trim)
                .filter(|parameter| !parameter.is_empty())
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

fn top_level_expression_count(tokens: &[&Token]) -> usize {
    let mut count = 0;
    let mut depth = 0_u32;
    let mut member_name = false;
    for token in tokens {
        match token.kind {
            TokenKind::RightParen | TokenKind::RightBracket | TokenKind::RightBrace => {
                depth = depth.saturating_sub(1);
            }
            TokenKind::LeftParen | TokenKind::LeftBracket | TokenKind::LeftBrace => {
                if depth == 0 {
                    count += 1;
                }
                depth += 1;
            }
            TokenKind::Dot if depth == 0 => member_name = true,
            TokenKind::Eol | TokenKind::Eof => {}
            _ if depth == 0 && member_name => member_name = false,
            _ if depth == 0 => count += 1,
            _ => {}
        }
    }
    count
}

fn semantic_legend() -> SemanticTokensLegend {
    SemanticTokensLegend {
        token_types: vec![
            SemanticTokenType::NAMESPACE,
            SemanticTokenType::FUNCTION,
            SemanticTokenType::PARAMETER,
            SemanticTokenType::VARIABLE,
            SemanticTokenType::PROPERTY,
            SemanticTokenType::KEYWORD,
            SemanticTokenType::STRING,
            SemanticTokenType::NUMBER,
            SemanticTokenType::ENUM_MEMBER,
            SemanticTokenType::OPERATOR,
            SemanticTokenType::new("boolean"),
        ],
        token_modifiers: vec![
            SemanticTokenModifier::DECLARATION,
            SemanticTokenModifier::READONLY,
        ],
    }
}

fn semantic_tokens(text: &str, analysis: &Analysis) -> Vec<SemanticToken> {
    let mut entries = Vec::<(Span, u32, u32)>::new();
    for token in &analysis.tokens {
        let token_type = match token.kind {
            TokenKind::Keyword(_) => Some(5),
            TokenKind::String(_) => Some(6),
            TokenKind::Integer(_) | TokenKind::Float(_) => Some(7),
            TokenKind::Symbol(_)
                if analysis.semantic.as_ref().is_some_and(|model| {
                    model
                        .symbols
                        .iter()
                        .any(|symbol| symbol.declaration == token.span)
                }) =>
            {
                None
            }
            TokenKind::Symbol(_) => Some(8),
            TokenKind::Boolean(_) => Some(10),
            _ => None,
        };
        if let Some(token_type) = token_type {
            entries.push((token.span, token_type, 0));
        } else if catalog_member_at(text, analysis, token.span.start).is_some() {
            entries.push((token.span, 4, 0));
        }
    }
    if let Some(model) = &analysis.semantic {
        for symbol in &model.symbols {
            entries.push((
                symbol.declaration,
                semantic_symbol_token_type(symbol.kind),
                1 | u32::from(!symbol.mutable) << 1,
            ));
        }
        for reference in &model.references {
            let symbol = &model.symbols[reference.symbol];
            entries.push((
                reference.span,
                semantic_symbol_token_type(symbol.kind),
                u32::from(!symbol.mutable) << 1,
            ));
        }
    }
    entries.sort_by_key(|(span, _, _)| (span.start, span.end));
    entries.dedup_by_key(|(span, _, _)| (span.start, span.end));

    let mut previous_line = 0;
    let mut previous_start = 0;
    entries
        .into_iter()
        .map(|(span, token_type, modifiers)| {
            let start = offset_to_position(text, span.start);
            let end = offset_to_position(text, span.end);
            let delta_line = start.line - previous_line;
            let delta_start = if delta_line == 0 {
                start.character - previous_start
            } else {
                start.character
            };
            previous_line = start.line;
            previous_start = start.character;
            SemanticToken {
                delta_line,
                delta_start,
                length: end.character.saturating_sub(start.character),
                token_type,
                token_modifiers_bitset: modifiers,
            }
        })
        .collect()
}

fn semantic_symbol_token_type(kind: SemanticSymbolKind) -> u32 {
    match kind {
        SemanticSymbolKind::Function => 1,
        SemanticSymbolKind::Parameter => 2,
        SemanticSymbolKind::Binding | SemanticSymbolKind::PatternCapture => 3,
    }
}

fn folding_ranges(text: &str, module: &Module) -> Vec<FoldingRange> {
    let mut spans = module
        .blocks
        .iter()
        .map(|block| block.span)
        .collect::<Vec<_>>();
    spans.extend(module.nodes.iter().filter_map(|node| match node.kind {
        NodeKind::List(_) | NodeKind::Match { .. } | NodeKind::Branch(_) => Some(node.span),
        _ => None,
    }));
    spans.sort_by_key(|span| (span.start, span.end));
    spans.dedup_by_key(|span| (span.start, span.end));
    spans
        .into_iter()
        .filter_map(|span| {
            let start = offset_to_position(text, span.start);
            let end = offset_to_position(text, span.end);
            (start.line < end.line).then_some(FoldingRange {
                start_line: start.line,
                start_character: Some(start.character),
                end_line: end.line,
                end_character: Some(end.character),
                kind: None,
                collapsed_text: None,
            })
        })
        .collect()
}

fn selection_at(text: &str, module: &Module, tokens: &[Token], offset: usize) -> SelectionRange {
    let mut spans = tokens
        .iter()
        .filter(|token| token.span.start <= offset && offset < token.span.end)
        .map(|token| token.span)
        .collect::<Vec<_>>();
    spans.extend(
        module
            .nodes
            .iter()
            .filter(|node| node.span.start <= offset && offset <= node.span.end)
            .map(|node| node.span),
    );
    spans.extend(
        module
            .blocks
            .iter()
            .filter(|block| block.span.start <= offset && offset <= block.span.end)
            .map(|block| block.span),
    );
    spans.push(Span::new(module.source, 0, text.len()));
    spans.sort_by_key(|span| span.end.saturating_sub(span.start));
    spans.dedup_by_key(|span| (span.start, span.end));

    let mut selection = None;
    for span in spans.into_iter().rev() {
        selection = Some(SelectionRange {
            range: span_to_range(text, span),
            parent: selection.map(Box::new),
        });
    }
    selection.expect("the document range is always present")
}

fn inlay_hints(
    text: &str,
    module: &Module,
    semantic: Option<&SemanticModel>,
    requested_range: Range,
) -> Vec<InlayHint> {
    let start = position_to_offset(text, requested_range.start).unwrap_or(0);
    let end = position_to_offset(text, requested_range.end).unwrap_or(text.len());
    let mut hints = Vec::new();
    for node in &module.nodes {
        let NodeKind::Call {
            callee, argument, ..
        } = &node.kind
        else {
            continue;
        };
        if node.span.end < start || node.span.start > end {
            continue;
        }
        let parameters = call_parameters(module, *callee, semantic);
        let arguments = match &module.node(*argument).kind {
            NodeKind::List(elements) => elements.as_slice(),
            _ => std::slice::from_ref(argument),
        };
        for (argument, parameter) in arguments.iter().zip(parameters) {
            let argument = module.node(*argument);
            if matches!(&argument.kind, NodeKind::Identifier(name) if name.as_ref() == parameter) {
                continue;
            }
            hints.push(InlayHint {
                position: offset_to_position(text, argument.span.start),
                label: InlayHintLabel::String(format!("{parameter}:")),
                kind: Some(InlayHintKind::PARAMETER),
                text_edits: None,
                tooltip: None,
                padding_left: None,
                padding_right: Some(true),
                data: None,
            });
        }
    }
    hints
}

fn call_parameters(
    module: &Module,
    callee: NodeId,
    semantic: Option<&SemanticModel>,
) -> Vec<String> {
    match &module.node(callee).kind {
        NodeKind::Identifier(_) => {
            let Some(model) = semantic else {
                return Vec::new();
            };
            let Some(symbol) = model.symbol_at(module.node(callee).span.start) else {
                return Vec::new();
            };
            model_parameters(model, symbol)
        }
        NodeKind::Member { object, member } => {
            let NodeKind::Identifier(namespace) = &module.node(*object).kind else {
                return Vec::new();
            };
            catalog::namespace_members(namespace)
                .and_then(|members| {
                    members
                        .iter()
                        .find(|candidate| candidate.name == member.text.as_ref())
                })
                .map(|member| signature_parameters(member.signature))
                .unwrap_or_default()
        }
        _ => Vec::new(),
    }
}

fn model_parameters(model: &SemanticModel, symbol: crate::semantic::SymbolId) -> Vec<String> {
    let symbol = &model.symbols[symbol];
    if symbol.kind == SemanticSymbolKind::Function {
        symbol.parameters.clone()
    } else {
        Vec::new()
    }
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
        let result = analyze("val :value\n");
        assert_eq!(result.diagnostics.len(), 1);
        assert_eq!(result.diagnostics[0].source.as_deref(), Some("pima"));
        assert!(result.module.is_some());
    }

    #[test]
    fn retains_symbols_around_an_incomplete_statement() {
        let text = "val :before 1\nval :incomplete\nval :after 2\n";
        let result = analyze(text);
        let module = result.module.expect("recoverable module");
        let symbols = document_symbols(text, &module);
        assert_eq!(
            symbols
                .iter()
                .map(|symbol| symbol.name.as_str())
                .collect::<Vec<_>>(),
            ["before", "after"]
        );
    }

    #[test]
    fn extracts_top_level_symbols() {
        let result = analyze("val :answer 42\nfunction :double (x) {\n  * x 2\n}\n");
        let module = result.module.expect("valid module");
        let symbols = document_symbols(
            "val :answer 42\nfunction :double (x) {\n  * x 2\n}\n",
            &module,
        );
        assert_eq!(symbols.len(), 2);
        assert_eq!(symbols[0].name, "answer");
        assert_eq!(symbols[1].name, "double");
    }

    #[test]
    fn positions_use_utf16_code_units() {
        let text = "\"\u{1F600}\" value";
        assert_eq!(offset_to_position(text, 6), Position::new(0, 4));
        assert_eq!(position_to_offset(text, Position::new(0, 4)), Some(6));
    }

    #[test]
    fn rename_names_follow_pima_identifier_rules() {
        assert!(valid_identifier("parse_value"));
        assert!(valid_identifier("empty?"));
        assert!(!valid_identifier("empty?now"));
        assert!(!valid_identifier("val"));
        assert!(!valid_identifier(":value"));
        assert_eq!(to_snake_case("parseValue"), "parse_value");
        assert_eq!(to_snake_case("HTTPValue"), "http_value");
        assert_eq!(to_snake_case("empty?"), "empty?");
    }

    #[test]
    fn recognizes_standard_namespace_member_completion_context() {
        assert_eq!(member_receiver("Math.", 5), Some("Math"));
        let text = "val :result String.";
        assert_eq!(member_receiver(text, text.len()), Some("String"));
        assert_eq!(member_receiver("value", 5), None);
    }

    #[test]
    fn completes_and_describes_inferred_future_members() {
        let text = "val :Worker { pub val :value 1 }\nval :worker [remote Worker]\nval :pending worker.value\npending.";
        let analysis = analyze(text);
        let members = receiver_members("pending", text.len(), &analysis).expect("future members");
        assert_eq!(
            members.iter().map(|member| member.name).collect::<Vec<_>>(),
            ["complete?"]
        );

        let call = "val :Worker { pub val :value 1 }\nval :worker [remote Worker]\nval :pending worker.value\n[pending.complete?]";
        let analysis = analyze(call);
        let offset = call.find("complete?").expect("complete token");
        let (member, _) = catalog_member_at(call, &analysis, offset).expect("future member hover");
        assert_eq!(member.signature, "future.complete?()");
        let signature = signature_at(call, &analysis, call.len() - 1).expect("future signature");
        assert_eq!(signature.0, "future.complete?()");
    }

    #[test]
    fn current_keywords_are_completed_and_reserved_for_rename() {
        let labels = keyword_completions()
            .into_iter()
            .map(|item| item.label)
            .collect::<Vec<_>>();
        for keyword in ["await", "branch", "remote"] {
            assert!(labels.iter().any(|label| label == keyword));
            assert!(!valid_identifier(keyword));
        }
    }

    #[test]
    fn document_symbols_include_namespace_members() {
        let text = "val :Point {\n    pub val :x 0\n    pub function :move (amount) {\n        x\n    }\n}\n";
        let result = analyze(text);
        let symbols = document_symbols(text, &result.module.expect("module"));
        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "Point");
        assert_eq!(
            symbols[0]
                .children
                .as_ref()
                .expect("namespace children")
                .iter()
                .map(|symbol| symbol.name.as_str())
                .collect::<Vec<_>>(),
            ["x", "move"]
        );
    }

    #[test]
    fn standard_member_signature_help_tracks_arguments() {
        let text = "[Math.pow 2 ";
        let analysis = analyze(text);
        let (label, parameters, active) =
            signature_at(text, &analysis, text.len()).expect("signature");
        assert_eq!(label, "Math.pow(base, exponent)");
        assert_eq!(parameters, ["base", "exponent"]);
        assert_eq!(active, 1);
    }

    #[test]
    fn user_function_signature_help_uses_declared_parameters() {
        let text = "function :add (left right) {\n    + left right\n}\n[add 1 ";
        let analysis = analyze(text);
        let (label, parameters, active) =
            signature_at(text, &analysis, text.len()).expect("signature");
        assert_eq!(label, "add(left, right)");
        assert_eq!(parameters, ["left", "right"]);
        assert_eq!(active, 1);
    }

    #[test]
    fn semantic_tokens_classify_functions_parameters_and_references() {
        let text = "function :identity (value) {\n    value\n}\n";
        let analysis = analyze(text);
        let tokens = semantic_tokens(text, &analysis);
        assert!(tokens.iter().any(|token| token.token_type == 1));
        assert!(tokens.iter().any(|token| token.token_type == 2));
        assert!(
            tokens
                .iter()
                .any(|token| token.token_type == 2 && token.token_modifiers_bitset == 2)
        );
    }

    #[test]
    fn folding_ranges_include_multiline_blocks_and_lists() {
        let text = "val :values (\n    1\n    2\n)\nval :selected branch (\n    true {\n        values\n    }\n)\nfunction :read () {\n    values\n}\n";
        let analysis = analyze(text);
        let ranges = folding_ranges(text, analysis.module.as_ref().expect("module"));
        assert!(ranges.len() >= 4);
        assert!(ranges.iter().all(|range| range.start_line < range.end_line));
    }

    #[test]
    fn selection_ranges_expand_from_token_to_document() {
        let text = "function :read (value) {\n    value\n}\n";
        let analysis = analyze(text);
        let offset = text.rfind("value").expect("reference");
        let selection = selection_at(
            text,
            analysis.module.as_ref().expect("module"),
            &analysis.tokens,
            offset,
        );
        assert_eq!(
            selection.range,
            span_to_range(
                text,
                Span::new(
                    analysis.module.as_ref().expect("module").source,
                    offset,
                    offset + "value".len()
                )
            )
        );
        assert!(selection.parent.is_some());
    }

    #[test]
    fn inlay_hints_use_known_parameter_names() {
        let text = "function :add (left right) {\n    + left right\n}\n[add 1 2]\n";
        let analysis = analyze(text);
        let hints = inlay_hints(
            text,
            analysis.module.as_ref().expect("module"),
            analysis.semantic.as_ref(),
            Range::new(Position::new(0, 0), offset_to_position(text, text.len())),
        );
        let labels = hints
            .iter()
            .map(|hint| match &hint.label {
                InlayHintLabel::String(label) => label.as_str(),
                InlayHintLabel::LabelParts(_) => panic!("unexpected label parts"),
            })
            .collect::<Vec<_>>();
        assert!(labels.contains(&"left:"));
        assert!(labels.contains(&"right:"));
    }

    #[test]
    fn large_document_analysis_stays_interactive() {
        let mut text = String::new();
        for index in 0..1_500 {
            text.push_str(&format!(
                "function :function_{index} (value) {{\n    + value {index}\n}}\n"
            ));
        }
        let started = std::time::Instant::now();
        let analysis = analyze(&text);
        assert!(analysis.diagnostics.is_empty());
        assert_eq!(
            analysis
                .semantic
                .as_ref()
                .expect("semantic model")
                .symbols
                .len(),
            3_000
        );
        assert!(
            started.elapsed() < std::time::Duration::from_secs(5),
            "large document analysis took {:?}",
            started.elapsed()
        );
    }
}
