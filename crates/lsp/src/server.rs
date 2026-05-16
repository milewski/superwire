use std::collections::HashMap;
use std::path::PathBuf;

use lsp_server::{Connection, ErrorCode, Message, Notification, Request, RequestId, Response, ResponseError};
use lsp_types::notification::{DidChangeTextDocument, DidCloseTextDocument, DidOpenTextDocument, Exit, Initialized, Notification as _};
use lsp_types::request::{
    CodeActionRequest, CodeLensRequest, Completion, DocumentSymbolRequest, ExecuteCommand, FoldingRangeRequest, Formatting, GotoDefinition,
    HoverRequest, Request as _, Shutdown, WorkspaceSymbolRequest,
};
use lsp_types::{
    CodeAction, CodeActionKind, CodeActionOptions, CodeActionOrCommand, CodeActionProviderCapability, CodeLens, CodeLensOptions, Command,
    CompletionItem, CompletionList, CompletionOptions, CompletionResponse, CompletionTextEdit, Diagnostic, DocumentChanges, DocumentSymbol,
    ExecuteCommandOptions, FoldingRange, FoldingRangeKind, FoldingRangeProviderCapability, Hover, HoverContents, HoverProviderCapability,
    InitializeResult, InsertTextFormat, Location, MarkupContent, MarkupKind, OneOf, OptionalVersionedTextDocumentIdentifier,
    PublishDiagnosticsParams, ServerCapabilities, ServerInfo, SymbolInformation, TextDocumentEdit, TextDocumentPositionParams,
    TextDocumentSyncCapability, TextDocumentSyncKind, TextEdit, Uri, WorkspaceEdit,
};
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::Value;
use superwire_core::dsl::parse_workflow;
use superwire_core::mcp::{McpLock, ProjectMcpLock};
use thiserror::Error;

use crate::document::{
    CodeActionSuggestion, CodeLensHint, CompletionSuggestion, DocumentState, DocumentSymbolNode, FoldingRangeBlock, WorkspaceSymbolMatch,
};

#[derive(Debug, Error)]
pub enum ServerError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("language server channel closed")]
    ChannelClosed,
}

#[derive(Debug)]
struct RequestOutcome {
    response: Option<Response>,
    notifications: Vec<Notification>,
    should_exit: bool,
}

#[derive(Debug)]
pub struct ServerMessages {
    pub messages: Vec<Value>,
    pub should_exit: bool,
}

#[derive(Debug, Default)]
pub struct LanguageServer {
    documents: HashMap<String, DocumentState>,
}

impl LanguageServer {
    pub fn handle_json_rpc_message(&mut self, raw_message: &[u8]) -> Result<ServerMessages, ServerError> {
        let message: Message = serde_json::from_slice(raw_message)?;
        let server_messages = self.handle_message(message)?;
        let messages = server_messages
            .messages
            .into_iter()
            .map(serde_json::to_value)
            .collect::<Result<Vec<_>, _>>()?;

        Ok(ServerMessages {
            messages,
            should_exit: server_messages.should_exit,
        })
    }

    pub fn run_stdio() -> Result<(), ServerError> {
        let (connection, io_threads) = Connection::stdio();
        let mut language_server = Self::default();

        for message in &connection.receiver {
            let server_messages = language_server.handle_message(message)?;

            for message in server_messages.messages {
                connection.sender.send(message).map_err(|_| ServerError::ChannelClosed)?;
            }

            if server_messages.should_exit {
                break;
            }
        }

        drop(connection);
        io_threads.join()?;

        Ok(())
    }

    fn handle_message(&mut self, message: Message) -> Result<ServerMessageBatch, ServerError> {
        match message {
            Message::Request(request) => self.handle_request(request),
            Message::Notification(notification) => self.handle_notification(notification),
            Message::Response(_) => Ok(ServerMessageBatch::continue_without_response()),
        }
    }

    fn handle_request(&mut self, request: Request) -> Result<ServerMessageBatch, ServerError> {
        log::debug!("handling LSP request method {}", request.method);

        let outcome = match request.method.as_str() {
            lsp_types::request::Initialize::METHOD => self.initialize_outcome(request.id),
            Shutdown::METHOD => self.shutdown_outcome(request.id),
            Completion::METHOD => self.handle_completion(request.id, request.params)?,
            HoverRequest::METHOD => self.handle_hover(request.id, request.params)?,
            GotoDefinition::METHOD => self.handle_definition(request.id, request.params)?,
            DocumentSymbolRequest::METHOD => self.handle_document_symbols(request.id, request.params)?,
            WorkspaceSymbolRequest::METHOD => self.handle_workspace_symbols(request.id, request.params)?,
            FoldingRangeRequest::METHOD => self.handle_folding_ranges(request.id, request.params)?,
            Formatting::METHOD => self.handle_formatting(request.id, request.params)?,
            CodeActionRequest::METHOD => self.handle_code_action(request.id, request.params)?,
            CodeLensRequest::METHOD => self.handle_code_lens(request.id, request.params)?,
            ExecuteCommand::METHOD => self.handle_execute_command(request.id, request.params)?,
            _ => self.method_not_found_outcome(request.id),
        };

        Ok(outcome.into_message_batch())
    }

    fn handle_notification(&mut self, notification: Notification) -> Result<ServerMessageBatch, ServerError> {
        log::debug!("handling LSP notification method {}", notification.method);

        let outcome = match notification.method.as_str() {
            Initialized::METHOD => RequestOutcome::continue_without_response(),
            Exit::METHOD => RequestOutcome::exit_without_response(),
            DidOpenTextDocument::METHOD => self.handle_did_open(notification.params)?,
            DidChangeTextDocument::METHOD => self.handle_did_change(notification.params)?,
            DidCloseTextDocument::METHOD => self.handle_did_close(notification.params)?,
            _ => RequestOutcome::continue_without_response(),
        };

        Ok(outcome.into_message_batch())
    }

    fn initialize_outcome(&self, request_id: RequestId) -> RequestOutcome {
        RequestOutcome::with_response(success_response(request_id, initialize_result()))
    }

    fn shutdown_outcome(&self, request_id: RequestId) -> RequestOutcome {
        RequestOutcome::with_response(success_response(request_id, ()))
    }

    fn method_not_found_outcome(&self, request_id: RequestId) -> RequestOutcome {
        RequestOutcome::with_response(Response {
            id: request_id,
            result: None,
            error: Some(ResponseError {
                code: ErrorCode::MethodNotFound as i32,
                message: "Method not found".to_string(),
                data: None,
            }),
        })
    }

    fn handle_did_open(&mut self, params: Value) -> Result<RequestOutcome, ServerError> {
        let open_params: lsp_types::DidOpenTextDocumentParams = deserialize_params(params)?;
        let document_uri = open_params.text_document.uri.to_string();
        let mcp_lock = resolve_mcp_lock(&document_uri, &open_params.text_document.text, None);

        self.documents
            .insert(document_uri.clone(), DocumentState::new(open_params.text_document.text, mcp_lock));

        let diagnostics_notification = self.publish_document_diagnostics(&document_uri);

        Ok(RequestOutcome::without_response(diagnostics_notification))
    }

    fn handle_did_change(&mut self, params: Value) -> Result<RequestOutcome, ServerError> {
        let change_params: lsp_types::DidChangeTextDocumentParams = deserialize_params(params)?;
        let document_uri = change_params.text_document.uri.to_string();

        if let Some(last_change) = change_params.content_changes.last() {
            let previous_mcp_lock = self.documents.get(&document_uri).and_then(DocumentState::mcp_lock);
            let mcp_lock = resolve_mcp_lock(&document_uri, &last_change.text, previous_mcp_lock);

            if let Some(document_state) = self.documents.get_mut(&document_uri) {
                document_state.replace_text(last_change.text.clone(), mcp_lock);
            } else {
                self.documents
                    .insert(document_uri.clone(), DocumentState::new(last_change.text.clone(), mcp_lock));
            }
        }

        let diagnostics_notification = self.publish_document_diagnostics(&document_uri);

        Ok(RequestOutcome::without_response(diagnostics_notification))
    }

    fn handle_did_close(&mut self, params: Value) -> Result<RequestOutcome, ServerError> {
        let close_params: lsp_types::DidCloseTextDocumentParams = deserialize_params(params)?;
        let document_uri = close_params.text_document.uri.to_string();

        self.documents.remove(&document_uri);

        let diagnostics_notification = publish_diagnostics_notification(close_params.text_document.uri, Vec::new());

        Ok(RequestOutcome::without_response(Some(diagnostics_notification)))
    }

    fn handle_completion(&self, request_id: RequestId, params: Value) -> Result<RequestOutcome, ServerError> {
        let completion_params: TextDocumentPositionParams = deserialize_params(params)?;

        Ok(RequestOutcome::with_response(success_response(
            request_id,
            self.completion_result(&completion_params),
        )))
    }

    fn handle_hover(&self, request_id: RequestId, params: Value) -> Result<RequestOutcome, ServerError> {
        let hover_params: TextDocumentPositionParams = deserialize_params(params)?;

        Ok(RequestOutcome::with_response(success_response(
            request_id,
            self.hover_result(&hover_params),
        )))
    }

    fn handle_definition(&self, request_id: RequestId, params: Value) -> Result<RequestOutcome, ServerError> {
        let definition_params: TextDocumentPositionParams = deserialize_params(params)?;

        Ok(RequestOutcome::with_response(success_response(
            request_id,
            self.definition_result(&definition_params),
        )))
    }

    fn handle_document_symbols(&self, request_id: RequestId, params: Value) -> Result<RequestOutcome, ServerError> {
        let symbol_params: lsp_types::DocumentSymbolParams = deserialize_params(params)?;

        Ok(RequestOutcome::with_response(success_response(
            request_id,
            self.document_symbols_result(&symbol_params),
        )))
    }

    fn handle_workspace_symbols(&self, request_id: RequestId, params: Value) -> Result<RequestOutcome, ServerError> {
        let workspace_symbol_params: lsp_types::WorkspaceSymbolParams = deserialize_params(params)?;

        Ok(RequestOutcome::with_response(success_response(
            request_id,
            self.workspace_symbols_result(&workspace_symbol_params),
        )))
    }

    fn handle_folding_ranges(&self, request_id: RequestId, params: Value) -> Result<RequestOutcome, ServerError> {
        let folding_range_params: lsp_types::FoldingRangeParams = deserialize_params(params)?;

        Ok(RequestOutcome::with_response(success_response(
            request_id,
            self.folding_ranges_result(&folding_range_params),
        )))
    }

    fn handle_formatting(&self, request_id: RequestId, params: Value) -> Result<RequestOutcome, ServerError> {
        let formatting_params: lsp_types::DocumentFormattingParams = deserialize_params(params)?;

        Ok(RequestOutcome::with_response(success_response(
            request_id,
            self.formatting_result(&formatting_params),
        )))
    }

    fn handle_code_lens(&self, request_id: RequestId, params: Value) -> Result<RequestOutcome, ServerError> {
        let code_lens_params: lsp_types::CodeLensParams = deserialize_params(params)?;

        Ok(RequestOutcome::with_response(success_response(
            request_id,
            self.code_lens_result(&code_lens_params),
        )))
    }

    fn handle_code_action(&self, request_id: RequestId, params: Value) -> Result<RequestOutcome, ServerError> {
        let code_action_params: lsp_types::CodeActionParams = deserialize_params(params)?;

        Ok(RequestOutcome::with_response(success_response(
            request_id,
            self.code_action_result(&code_action_params),
        )))
    }

    fn handle_execute_command(&self, request_id: RequestId, params: Value) -> Result<RequestOutcome, ServerError> {
        let _execute_command_params: lsp_types::ExecuteCommandParams = deserialize_params(params)?;

        Ok(RequestOutcome::with_response(success_response(request_id, ())))
    }

    fn publish_document_diagnostics(&self, document_uri: &str) -> Option<Notification> {
        let document_state = self.documents.get(document_uri)?;
        let document_uri = document_uri.parse::<Uri>().ok()?;
        let diagnostics = document_state
            .diagnostics()
            .into_iter()
            .map(|document_diagnostic| Diagnostic {
                range: document_diagnostic.range,
                severity: Some(document_diagnostic.severity),
                code: Some(document_diagnostic.code.as_lsp_code()),
                code_description: None,
                source: Some("superwire-lsp".to_string()),
                message: document_diagnostic.message,
                related_information: None,
                tags: None,
                data: None,
            })
            .collect::<Vec<_>>();

        Some(publish_diagnostics_notification(document_uri, diagnostics))
    }

    fn completion_result(&self, completion_params: &TextDocumentPositionParams) -> CompletionResponse {
        let document_uri = completion_params.text_document.uri.to_string();
        let Some(document_state) = self.documents.get(&document_uri) else {
            return CompletionResponse::List(CompletionList {
                is_incomplete: false,
                items: Vec::new(),
            });
        };

        let completion_text_edit_range = document_state.completion_text_edit_range(completion_params.position);
        let completion_items = document_state
            .completion_suggestions(completion_params.position)
            .into_iter()
            .map(|completion_suggestion| completion_suggestion.into_lsp_completion_item(completion_text_edit_range))
            .collect::<Vec<_>>();

        CompletionResponse::List(CompletionList {
            is_incomplete: false,
            items: completion_items,
        })
    }

    fn hover_result(&self, hover_params: &TextDocumentPositionParams) -> Option<Hover> {
        let document_uri = hover_params.text_document.uri.to_string();
        let document_state = self.documents.get(&document_uri)?;
        let hover_markdown = document_state.hover_markdown(hover_params.position)?;

        Some(markdown_hover(hover_markdown))
    }

    fn definition_result(&self, definition_params: &TextDocumentPositionParams) -> Option<Vec<Location>> {
        let document_uri = definition_params.text_document.uri.to_string();
        let document_state = self.documents.get(&document_uri)?;
        let definition_range = document_state.definition_range(definition_params.position)?;

        Some(vec![Location {
            uri: definition_params.text_document.uri.clone(),
            range: definition_range,
        }])
    }

    fn document_symbols_result(&self, symbol_params: &lsp_types::DocumentSymbolParams) -> Vec<DocumentSymbol> {
        let document_uri = symbol_params.text_document.uri.to_string();
        let Some(document_state) = self.documents.get(&document_uri) else {
            return Vec::new();
        };

        document_state
            .document_symbols()
            .into_iter()
            .map(DocumentSymbolNode::into_lsp_document_symbol)
            .collect()
    }

    fn workspace_symbols_result(&self, workspace_symbol_params: &lsp_types::WorkspaceSymbolParams) -> Vec<SymbolInformation> {
        let mut workspace_symbols = self
            .documents
            .iter()
            .flat_map(|(document_uri, document_state)| {
                document_state.workspace_symbols(document_uri, workspace_symbol_params.query.as_str())
            })
            .collect::<Vec<_>>();

        workspace_symbols.sort_by(|left_symbol, right_symbol| left_symbol.name.cmp(&right_symbol.name));

        workspace_symbols
            .into_iter()
            .filter_map(WorkspaceSymbolMatch::into_lsp_symbol_information)
            .collect()
    }

    fn folding_ranges_result(&self, folding_range_params: &lsp_types::FoldingRangeParams) -> Vec<FoldingRange> {
        let document_uri = folding_range_params.text_document.uri.to_string();
        let Some(document_state) = self.documents.get(&document_uri) else {
            return Vec::new();
        };

        document_state
            .folding_ranges()
            .into_iter()
            .map(FoldingRangeBlock::into_lsp_folding_range)
            .collect()
    }

    fn formatting_result(&self, formatting_params: &lsp_types::DocumentFormattingParams) -> Vec<TextEdit> {
        let document_uri = formatting_params.text_document.uri.to_string();
        let Some(document_state) = self.documents.get(&document_uri) else {
            return Vec::new();
        };
        let Some(formatting_edit) = document_state.formatting_edit() else {
            return Vec::new();
        };

        vec![TextEdit {
            range: formatting_edit.range,
            new_text: formatting_edit.new_text,
        }]
    }

    fn code_lens_result(&self, code_lens_params: &lsp_types::CodeLensParams) -> Vec<CodeLens> {
        let document_uri = code_lens_params.text_document.uri.to_string();
        let Some(document_state) = self.documents.get(&document_uri) else {
            return Vec::new();
        };

        document_state
            .generated_output_marks()
            .into_iter()
            .map(CodeLensHint::into_lsp_code_lens)
            .collect()
    }

    fn code_action_result(&self, code_action_params: &lsp_types::CodeActionParams) -> Vec<CodeActionOrCommand> {
        let document_uri = code_action_params.text_document.uri.to_string();
        let Some(document_state) = self.documents.get(&document_uri) else {
            return Vec::new();
        };

        document_state
            .code_actions(code_action_params.range.start)
            .into_iter()
            .map(|code_action| code_action.into_lsp_code_action(code_action_params.text_document.uri.clone()))
            .collect()
    }
}

#[derive(Debug)]
struct ServerMessageBatch {
    messages: Vec<Message>,
    should_exit: bool,
}

impl ServerMessageBatch {
    fn continue_without_response() -> Self {
        Self {
            messages: Vec::new(),
            should_exit: false,
        }
    }
}

impl RequestOutcome {
    fn continue_without_response() -> Self {
        Self {
            response: None,
            notifications: Vec::new(),
            should_exit: false,
        }
    }

    fn exit_without_response() -> Self {
        Self {
            response: None,
            notifications: Vec::new(),
            should_exit: true,
        }
    }

    fn with_response(response: Response) -> Self {
        Self {
            response: Some(response),
            notifications: Vec::new(),
            should_exit: false,
        }
    }

    fn without_response(optional_notification: Option<Notification>) -> Self {
        Self {
            response: None,
            notifications: optional_notification.into_iter().collect(),
            should_exit: false,
        }
    }

    fn into_message_batch(self) -> ServerMessageBatch {
        let mut messages = Vec::new();

        if let Some(response) = self.response {
            messages.push(Message::Response(response));
        }

        messages.extend(self.notifications.into_iter().map(Message::Notification));

        ServerMessageBatch {
            messages,
            should_exit: self.should_exit,
        }
    }
}

impl CompletionSuggestion {
    fn into_lsp_completion_item(self, completion_text_edit_range: Option<lsp_types::Range>) -> CompletionItem {
        let insert_text_uses_snippet_format = self.insert_text.contains("$1");
        let mut completion_item = CompletionItem {
            label: self.label,
            kind: Some(self.kind),
            detail: Some(self.detail),
            documentation: Some(lsp_types::Documentation::String(self.documentation)),
            ..Default::default()
        };

        if let Some(text_edit_range) = completion_text_edit_range {
            completion_item.text_edit = Some(CompletionTextEdit::Edit(TextEdit {
                range: text_edit_range,
                new_text: self.insert_text,
            }));
        } else {
            completion_item.insert_text = Some(self.insert_text);
        }

        if insert_text_uses_snippet_format {
            completion_item.insert_text_format = Some(InsertTextFormat::SNIPPET);
        }

        completion_item
    }
}

impl DocumentSymbolNode {
    #[allow(deprecated)]
    fn into_lsp_document_symbol(self) -> DocumentSymbol {
        DocumentSymbol {
            name: self.name,
            detail: self.detail,
            kind: self.kind,
            tags: None,
            deprecated: None,
            range: self.range,
            selection_range: self.selection_range,
            children: Some(
                self.children
                    .into_iter()
                    .map(DocumentSymbolNode::into_lsp_document_symbol)
                    .collect(),
            ),
        }
    }
}

impl WorkspaceSymbolMatch {
    #[allow(deprecated)]
    fn into_lsp_symbol_information(self) -> Option<SymbolInformation> {
        Some(SymbolInformation {
            name: self.name,
            kind: self.kind,
            tags: None,
            deprecated: None,
            location: Location {
                uri: self.document_uri.parse().ok()?,
                range: self.range,
            },
            container_name: self.container_name,
        })
    }
}

impl FoldingRangeBlock {
    fn into_lsp_folding_range(self) -> FoldingRange {
        FoldingRange {
            start_line: self.start_line,
            start_character: Some(self.start_character),
            end_line: self.end_line,
            end_character: Some(self.end_character),
            kind: Some(FoldingRangeKind::Region),
            collapsed_text: None,
        }
    }
}

impl CodeLensHint {
    fn into_lsp_code_lens(self) -> CodeLens {
        CodeLens {
            range: self.range,
            command: Some(Command {
                title: self.title,
                command: self.command,
                arguments: Some(Vec::new()),
            }),
            data: None,
        }
    }
}

impl CodeActionSuggestion {
    fn into_lsp_code_action(self, document_uri: Uri) -> CodeActionOrCommand {
        CodeActionOrCommand::CodeAction(CodeAction {
            title: self.title,
            kind: Some(CodeActionKind::QUICKFIX),
            edit: Some(WorkspaceEdit {
                document_changes: Some(DocumentChanges::Edits(vec![TextDocumentEdit {
                    text_document: OptionalVersionedTextDocumentIdentifier {
                        uri: document_uri,
                        version: None,
                    },
                    edits: vec![OneOf::Left(TextEdit {
                        range: self.edit.range,
                        new_text: self.edit.new_text,
                    })],
                }])),
                ..Default::default()
            }),
            ..Default::default()
        })
    }
}

fn deserialize_params<ParamsType>(params: Value) -> Result<ParamsType, ServerError>
where
    ParamsType: DeserializeOwned,
{
    Ok(serde_json::from_value(params)?)
}

fn success_response<ResultType>(request_id: RequestId, result: ResultType) -> Response
where
    ResultType: Serialize,
{
    Response {
        id: request_id,
        result: Some(serde_json::to_value(result).expect("LSP response should serialize")),
        error: None,
    }
}

fn publish_diagnostics_notification(uri: Uri, diagnostics: Vec<Diagnostic>) -> Notification {
    Notification {
        method: lsp_types::notification::PublishDiagnostics::METHOD.to_string(),
        params: serde_json::to_value(PublishDiagnosticsParams {
            uri,
            diagnostics,
            version: None,
        })
        .expect("publish diagnostics params should serialize"),
    }
}

fn initialize_result() -> InitializeResult {
    InitializeResult {
        capabilities: ServerCapabilities {
            text_document_sync: Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL)),
            hover_provider: Some(HoverProviderCapability::Simple(true)),
            definition_provider: Some(OneOf::Left(true)),
            document_symbol_provider: Some(OneOf::Left(true)),
            workspace_symbol_provider: Some(OneOf::Left(true)),
            folding_range_provider: Some(FoldingRangeProviderCapability::Simple(true)),
            document_formatting_provider: Some(OneOf::Left(true)),
            code_action_provider: Some(CodeActionProviderCapability::Options(CodeActionOptions {
                code_action_kinds: Some(vec![CodeActionKind::QUICKFIX]),
                ..Default::default()
            })),
            completion_provider: Some(CompletionOptions {
                resolve_provider: Some(false),
                trigger_characters: Some(vec![".".to_string(), ":".to_string(), "\"".to_string()]),
                ..Default::default()
            }),
            code_lens_provider: Some(CodeLensOptions {
                resolve_provider: Some(false),
            }),
            execute_command_provider: Some(ExecuteCommandOptions {
                commands: vec!["superwire.generated.output".to_string()],
                ..Default::default()
            }),
            ..Default::default()
        },
        server_info: Some(ServerInfo {
            name: "superwire-lsp".to_string(),
            version: Some("0.2.0".to_string()),
        }),
    }
}

fn markdown_hover(markdown: String) -> Hover {
    Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value: markdown,
        }),
        range: None,
    }
}

fn read_project_mcp_lock(document_uri: &str) -> Option<McpLock> {
    let workflow_path = path_for_document_uri(document_uri)?;
    let lock_path = ProjectMcpLock::discover_lock_path_for_workflow(&workflow_path)?;
    let lock_root = lock_path.parent()?;
    let project_lock = ProjectMcpLock::read_from_path(&lock_path).ok()?;

    project_lock.workflow_lock(lock_root, &workflow_path).cloned()
}

fn resolve_mcp_lock(document_uri: &str, source_text: &str, previous_mcp_lock: Option<McpLock>) -> Option<McpLock> {
    let project_mcp_lock = read_project_mcp_lock(document_uri);

    if let Some(mcp_lock) = lock_with_discovered_missing_servers(source_text, project_mcp_lock) {
        return Some(mcp_lock);
    }

    previous_mcp_lock.or_else(|| discover_mcp_lock_from_source(source_text))
}

fn discover_mcp_lock_from_source(source_text: &str) -> Option<McpLock> {
    let workflow = parse_workflow(source_text).ok()?;

    McpLock::discover_from_workflow(&workflow).ok()
}

fn lock_with_discovered_missing_servers(source_text: &str, project_mcp_lock: Option<McpLock>) -> Option<McpLock> {
    let Some(mut mcp_lock) = project_mcp_lock else {
        return discover_mcp_lock_from_source(source_text);
    };
    let workflow = parse_workflow(source_text).ok()?;
    let missing_server_names = workflow
        .declarations()
        .iter()
        .filter_map(|declaration| match declaration {
            superwire_core::dsl::Declaration::McpServer(mcp_server_declaration) => Some(mcp_server_declaration.name.as_str()),
            _ => None,
        })
        .filter(|server_name| !mcp_lock.servers.contains_key(*server_name))
        .collect::<Vec<_>>();

    if missing_server_names.is_empty() {
        return Some(mcp_lock);
    }

    let discovered_mcp_lock = McpLock::discover_from_workflow(&workflow).ok()?;

    for missing_server_name in missing_server_names {
        if let Some(server_lock) = discovered_mcp_lock.servers.get(missing_server_name) {
            mcp_lock.servers.insert(missing_server_name.to_string(), server_lock.clone());
        }
    }

    Some(mcp_lock)
}

fn path_for_document_uri(document_uri: &str) -> Option<PathBuf> {
    let file_path = document_uri.strip_prefix("file://")?;
    let decoded_file_path = percent_decode_file_uri_path(file_path);

    Some(PathBuf::from(decoded_file_path))
}

fn percent_decode_file_uri_path(path: &str) -> String {
    let mut decoded = String::new();
    let bytes = path.as_bytes();
    let mut byte_index = 0;

    while byte_index < bytes.len() {
        if bytes[byte_index] == b'%' && byte_index + 2 < bytes.len() {
            if let Ok(hex_value) = u8::from_str_radix(&path[byte_index + 1..byte_index + 3], 16) {
                decoded.push(char::from(hex_value));
                byte_index += 3;

                continue;
            }
        }

        decoded.push(char::from(bytes[byte_index]));
        byte_index += 1;
    }

    decoded
}

#[cfg(test)]
mod tests;
