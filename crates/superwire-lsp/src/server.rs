use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::fmt;
use std::path::PathBuf;

use lsp_server::{Connection, ErrorCode, Message, Notification, Request, RequestId, Response, ResponseError};
use lsp_types::notification::{DidChangeTextDocument, DidCloseTextDocument, DidOpenTextDocument, Exit, Initialized, Notification as _};
use lsp_types::request::{
    CodeActionRequest, CodeLensRequest, Completion, DocumentSymbolRequest, ExecuteCommand, FoldingRangeRequest, Formatting, GotoDefinition,
    HoverRequest, Request as _, SemanticTokensFullRequest, Shutdown, WorkspaceSymbolRequest,
};
use lsp_types::{
    CodeAction, CodeActionKind, CodeActionOptions, CodeActionOrCommand, CodeActionProviderCapability, CodeLens, CodeLensOptions, Command,
    CompletionItem, CompletionList, CompletionOptions, CompletionParams, CompletionResponse, CompletionTextEdit, Diagnostic,
    DiagnosticRelatedInformation, DocumentChanges, DocumentSymbol, DocumentSymbolResponse, ExecuteCommandOptions, FoldingRange,
    FoldingRangeKind, FoldingRangeProviderCapability, Hover, HoverContents, HoverProviderCapability, InitializeParams, InitializeResult,
    InsertTextFormat, Location, MarkupContent, MarkupKind, OneOf, OptionalVersionedTextDocumentIdentifier, PublishDiagnosticsParams,
    SemanticToken, SemanticTokenType, SemanticTokens, SemanticTokensFullOptions, SemanticTokensLegend, SemanticTokensOptions,
    SemanticTokensParams, SemanticTokensResult, SemanticTokensServerCapabilities, ServerCapabilities, ServerInfo, SymbolInformation,
    TextDocumentEdit, TextDocumentPositionParams, TextDocumentSyncCapability, TextDocumentSyncKind, TextEdit, Uri, WorkDoneProgressOptions,
    WorkspaceEdit,
};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::sync::{mpsc, Arc, Condvar, Mutex};
use std::time::{Duration, Instant};
use superwire_dsl::{parse_workflow, DeclarationKeyword, ImportKeyword, Workflow};
use superwire_mcp::{
    HttpMcpClientFactory, McpClientFactory, McpClientRequestScope, McpLock, McpLockResolutionContext, McpNetworkPolicy, McpServerConfig,
    McpServerLock, PolicyMcpClientFactory, ProjectMcpLock,
};
use thiserror::Error;

use crate::document::{
    CodeActionSuggestion, CodeLensHint, CompletionSuggestion, DocumentState, DocumentSymbolNode, FoldingRangeBlock, PositionEncoding,
    WorkspaceSymbolMatch,
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

const MCP_DISCOVERY_WORKER_COUNT: usize = 4;
const MCP_DISCOVERY_QUEUE_CAPACITY: usize = 64;
const MCP_DISCOVERY_RESULT_CAPACITY: usize = MCP_DISCOVERY_WORKER_COUNT * 2;
const MCP_DISCOVERY_CACHE_CAPACITY_PER_WORKER: usize = 32;
const MCP_DISCOVERY_CACHE_TTL: Duration = Duration::from_secs(15 * 60);

#[derive(Debug, Default)]
pub struct LanguageServer {
    documents: HashMap<String, DocumentState>,
    mcp_discovery_worker: McpDiscoveryWorker,
    pending_mcp_discoveries: HashMap<String, PendingMcpDiscovery>,
    next_mcp_discovery_request_id: u64,
    runtime_values_by_document_uri: HashMap<String, RuntimeValues>,
    network_mcp_discovery_trust: NetworkMcpDiscoveryTrust,
    position_encoding: PositionEncoding,
    completion_snippet_support: bool,
    hierarchical_document_symbol_support: bool,
}

#[derive(Debug)]
struct McpDiscoveryWorker {
    scheduler: Arc<McpDiscoveryScheduler>,
    result_receiver: mpsc::Receiver<McpDiscoveryResult>,
    #[cfg(test)]
    synchronous_cache: Mutex<McpDiscoveryCache>,
}

struct McpDiscoveryCache {
    server_locks_by_config_key: HashMap<McpDiscoveryCacheKey, CachedMcpServerLock>,
    client_factory: Arc<dyn McpClientFactory>,
    capacity: usize,
    time_to_live: Duration,
    next_access_sequence: u64,
}

#[derive(Clone, PartialEq)]
struct PendingMcpDiscovery {
    request_id: u64,
    document_version: i32,
    source_text: Arc<str>,
    runtime_values: Option<RuntimeValues>,
}

struct McpDiscoveryRequest {
    pending_discovery: PendingMcpDiscovery,
    document_uri: String,
    previous_mcp_lock: Option<McpLock>,
}

#[derive(Debug)]
struct McpDiscoveryResult {
    request_id: u64,
    document_uri: String,
    document_version: i32,
    source_text: Arc<str>,
    mcp_lock: Option<McpLock>,
}

#[derive(Debug)]
struct McpDiscoveryScheduler {
    state: Mutex<McpDiscoverySchedulerState>,
    request_available: Condvar,
    capacity: usize,
}

#[derive(Debug, Default)]
struct McpDiscoverySchedulerState {
    pending_requests_by_document_uri: HashMap<String, McpDiscoveryRequest>,
    ready_document_uris: VecDeque<String>,
    active_document_uris: HashSet<String>,
    closed: bool,
}

#[derive(Debug)]
struct McpDiscoveryScheduleOutcome {
    accepted: bool,
    evicted_document_uri: Option<String>,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct McpDiscoveryCacheKey([u8; 32]);

#[derive(Debug, Clone)]
struct CachedMcpServerLock {
    server_lock: McpServerLock,
    last_accessed_at: Instant,
    access_sequence: u64,
}

/// Custom initialization options under `InitializeParams.initializationOptions`.
///
/// Network MCP discovery is disabled unless the client explicitly sends
/// `{ "workspaceTrust": { "networkMcpDiscovery": "trusted" } }`.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SuperwireInitializationOptions {
    #[serde(default)]
    workspace_trust: WorkspaceTrustOptions,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WorkspaceTrustOptions {
    #[serde(default)]
    network_mcp_discovery: NetworkMcpDiscoveryTrust,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
enum NetworkMcpDiscoveryTrust {
    #[default]
    Disabled,
    Trusted,
}

#[derive(Clone, Default, PartialEq, serde::Deserialize)]
struct RuntimeValues {
    #[serde(default)]
    input: Value,
    #[serde(default)]
    secrets: Value,
}

impl NetworkMcpDiscoveryTrust {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Disabled => "disabled",
            Self::Trusted => "trusted",
        }
    }
}

impl fmt::Debug for RuntimeValues {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RuntimeValues")
            .field("input", &"<redacted>")
            .field("secrets", &"<redacted>")
            .finish()
    }
}

impl fmt::Debug for McpDiscoveryCache {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("McpDiscoveryCache")
            .field("server_locks_by_config_key", &self.server_locks_by_config_key)
            .field("client_factory", &"<redacted>")
            .field("capacity", &self.capacity)
            .field("time_to_live", &self.time_to_live)
            .field("next_access_sequence", &self.next_access_sequence)
            .finish()
    }
}

impl fmt::Debug for PendingMcpDiscovery {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PendingMcpDiscovery")
            .field("request_id", &self.request_id)
            .field("document_version", &self.document_version)
            .field("source_text", &"<redacted>")
            .field("runtime_values", &self.runtime_values)
            .finish()
    }
}

impl fmt::Debug for McpDiscoveryRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("McpDiscoveryRequest")
            .field("pending_discovery", &self.pending_discovery)
            .field("document_uri", &self.document_uri)
            .field("has_previous_mcp_lock", &self.previous_mcp_lock.is_some())
            .finish()
    }
}

impl fmt::Debug for McpDiscoveryCacheKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("sha256:")?;

        for digest_byte in self.0 {
            write!(formatter, "{digest_byte:02x}")?;
        }

        Ok(())
    }
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct RuntimeValuesParams {
    text_document: lsp_types::TextDocumentIdentifier,
    #[serde(default)]
    input: Value,
    #[serde(default)]
    secrets: Value,
}

impl RuntimeValues {
    fn lock_resolution_context(&self) -> McpLockResolutionContext {
        McpLockResolutionContext {
            input: Self::object_value_map(&self.input),
            secrets: Self::object_value_map(&self.secrets),
            dynamic: BTreeMap::new(),
            agent_outputs: BTreeMap::new(),
            agent_contexts: BTreeMap::new(),
        }
    }

    fn object_value_map(value: &Value) -> BTreeMap<String, Value> {
        value
            .as_object()
            .map(|object| {
                object
                    .iter()
                    .map(|(field_name, field_value)| (field_name.clone(), field_value.clone()))
                    .collect()
            })
            .unwrap_or_default()
    }
}

impl LanguageServer {
    fn with_discovery_client_factory(client_factory: Arc<dyn McpClientFactory>) -> Self {
        Self {
            documents: HashMap::new(),
            mcp_discovery_worker: McpDiscoveryWorker::new(client_factory),
            pending_mcp_discoveries: HashMap::new(),
            next_mcp_discovery_request_id: 0,
            runtime_values_by_document_uri: HashMap::new(),
            network_mcp_discovery_trust: NetworkMcpDiscoveryTrust::Disabled,
            position_encoding: PositionEncoding::default(),
            completion_snippet_support: false,
            hierarchical_document_symbol_support: false,
        }
    }

    /// Builds the stdio/editor server with a trusted-capable MCP transport.
    ///
    /// This constructor does not itself grant network access. Discovery remains
    /// disabled until initialization explicitly sets
    /// `workspaceTrust.networkMcpDiscovery` to `trusted`.
    #[must_use]
    pub fn for_stdio_editor() -> Self {
        Self::with_discovery_client_factory(Arc::new(PolicyMcpClientFactory::new(McpNetworkPolicy::Trusted)))
    }

    #[cfg(test)]
    fn with_mcp_client_factory(client_factory: Arc<dyn McpClientFactory>) -> Self {
        Self::with_discovery_client_factory(client_factory)
    }

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
        let mut language_server = Self::for_stdio_editor();

        loop {
            for diagnostics_notification in language_server.apply_ready_mcp_discovery_results() {
                connection
                    .sender
                    .send(Message::Notification(diagnostics_notification))
                    .map_err(|_| ServerError::ChannelClosed)?;
            }

            let message = match connection.receiver.recv_timeout(Duration::from_millis(10)) {
                Ok(message) => message,
                Err(receive_error) if receive_error.is_timeout() => continue,
                Err(_) => break,
            };
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
        let discovery_notifications = self.apply_ready_mcp_discovery_results();
        let mut server_messages = match message {
            Message::Request(request) => self.handle_request(request)?,
            Message::Notification(notification) => self.handle_notification(notification)?,
            Message::Response(_) => ServerMessageBatch::continue_without_response(),
        };

        server_messages
            .messages
            .extend(discovery_notifications.into_iter().map(Message::Notification));

        Ok(server_messages)
    }

    fn handle_request(&mut self, request: Request) -> Result<ServerMessageBatch, ServerError> {
        log::debug!("handling LSP request method {}", request.method);

        let outcome = match request.method.as_str() {
            lsp_types::request::Initialize::METHOD => self.initialize_outcome(request.id, request.params)?,
            Shutdown::METHOD => self.shutdown_outcome(request.id),
            Completion::METHOD => self.handle_completion(request.id, request.params)?,
            HoverRequest::METHOD => self.handle_hover(request.id, request.params)?,
            GotoDefinition::METHOD => self.handle_definition(request.id, request.params)?,
            DocumentSymbolRequest::METHOD => self.handle_document_symbols(request.id, request.params)?,
            WorkspaceSymbolRequest::METHOD => self.handle_workspace_symbols(request.id, request.params)?,
            SemanticTokensFullRequest::METHOD => self.handle_semantic_tokens(request.id, request.params)?,
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
            "superwire/runtimeValues" => self.handle_runtime_values(notification.params)?,
            _ => RequestOutcome::continue_without_response(),
        };

        Ok(outcome.into_message_batch())
    }

    fn initialize_outcome(&mut self, request_id: RequestId, params: Value) -> Result<RequestOutcome, ServerError> {
        let initialize_params: InitializeParams = deserialize_params(params)?;
        let initialization_options = match initialize_params.initialization_options.as_ref() {
            Some(options_value) => match serde_json::from_value::<SuperwireInitializationOptions>(options_value.clone()) {
                Ok(initialization_options) => initialization_options,
                Err(error) => {
                    self.set_network_mcp_discovery_trust(NetworkMcpDiscoveryTrust::Disabled);

                    return Ok(RequestOutcome::with_response(Response {
                        id: request_id,
                        result: None,
                        error: Some(ResponseError {
                            code: ErrorCode::InvalidParams as i32,
                            message: format!(
                                "invalid initializationOptions.workspaceTrust.networkMcpDiscovery: expected `{}` or `{}`; {error}",
                                NetworkMcpDiscoveryTrust::Disabled.as_str(),
                                NetworkMcpDiscoveryTrust::Trusted.as_str(),
                            ),
                            data: Some(serde_json::json!({
                                "path": "initializationOptions.workspaceTrust.networkMcpDiscovery",
                                "supportedValues": [
                                    NetworkMcpDiscoveryTrust::Disabled.as_str(),
                                    NetworkMcpDiscoveryTrust::Trusted.as_str()
                                ]
                            })),
                        }),
                    }));
                }
            },
            None => SuperwireInitializationOptions::default(),
        };

        self.set_network_mcp_discovery_trust(initialization_options.workspace_trust.network_mcp_discovery);

        self.position_encoding = initialize_params
            .capabilities
            .general
            .as_ref()
            .and_then(|general_capabilities| general_capabilities.position_encodings.as_ref())
            .and_then(|position_encodings| position_encodings.iter().find_map(PositionEncoding::from_kind))
            .unwrap_or_default();
        self.completion_snippet_support = initialize_params
            .capabilities
            .text_document
            .as_ref()
            .and_then(|text_document_capabilities| text_document_capabilities.completion.as_ref())
            .and_then(|completion_capabilities| completion_capabilities.completion_item.as_ref())
            .and_then(|completion_item_capabilities| completion_item_capabilities.snippet_support)
            .unwrap_or(false);
        self.hierarchical_document_symbol_support = initialize_params
            .capabilities
            .text_document
            .as_ref()
            .and_then(|text_document_capabilities| text_document_capabilities.document_symbol.as_ref())
            .and_then(|document_symbol_capabilities| document_symbol_capabilities.hierarchical_document_symbol_support)
            .unwrap_or(false);

        Ok(RequestOutcome::with_response(success_response(
            request_id,
            initialize_result(self.position_encoding),
        )))
    }

    fn set_network_mcp_discovery_trust(&mut self, network_mcp_discovery_trust: NetworkMcpDiscoveryTrust) {
        if self.network_mcp_discovery_trust == network_mcp_discovery_trust {
            return;
        }

        self.mcp_discovery_worker.cancel_all();
        self.pending_mcp_discoveries.clear();

        if network_mcp_discovery_trust == NetworkMcpDiscoveryTrust::Disabled {
            self.runtime_values_by_document_uri.clear();
        }

        self.network_mcp_discovery_trust = network_mcp_discovery_trust;
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
        let document_version = open_params.text_document.version;
        let source_text = open_params.text_document.text;
        let discovery_source_text = Arc::<str>::from(source_text.as_str());
        let mcp_lock = read_project_mcp_lock(&document_uri);

        self.documents.insert(
            document_uri.clone(),
            DocumentState::from_versioned_text(source_text, mcp_lock.clone(), Some(document_version), self.position_encoding),
        );
        self.schedule_mcp_discovery(document_uri.clone(), document_version, discovery_source_text, mcp_lock);

        let diagnostics_notification = self.publish_document_diagnostics(&document_uri);

        Ok(RequestOutcome::without_response(diagnostics_notification))
    }

    fn handle_did_change(&mut self, params: Value) -> Result<RequestOutcome, ServerError> {
        let change_params: lsp_types::DidChangeTextDocumentParams = deserialize_params(params)?;

        let document_uri = change_params.text_document.uri.to_string();
        if self
            .documents
            .get(&document_uri)
            .and_then(DocumentState::version)
            .is_some_and(|document_version| document_version >= change_params.text_document.version)
        {
            return Ok(RequestOutcome::continue_without_response());
        }

        if let Some(last_change) = change_params.content_changes.last() {
            let previous_mcp_lock = self.documents.get(&document_uri).and_then(DocumentState::mcp_lock);
            let mcp_lock = previous_mcp_lock.or_else(|| read_project_mcp_lock(&document_uri));
            let discovery_source_text = Arc::<str>::from(last_change.text.as_str());

            if let Some(document_state) = self.documents.get_mut(&document_uri) {
                document_state.replace_versioned_text(
                    last_change.text.clone(),
                    mcp_lock.clone(),
                    Some(change_params.text_document.version),
                );
            } else {
                self.documents.insert(
                    document_uri.clone(),
                    DocumentState::from_versioned_text(
                        last_change.text.clone(),
                        mcp_lock.clone(),
                        Some(change_params.text_document.version),
                        self.position_encoding,
                    ),
                );
            }

            self.schedule_mcp_discovery(
                document_uri.clone(),
                change_params.text_document.version,
                discovery_source_text,
                mcp_lock,
            );
        }

        let diagnostics_notification = self.publish_document_diagnostics(&document_uri);

        Ok(RequestOutcome::without_response(diagnostics_notification))
    }

    fn handle_did_close(&mut self, params: Value) -> Result<RequestOutcome, ServerError> {
        let close_params: lsp_types::DidCloseTextDocumentParams = deserialize_params(params)?;
        let document_uri = close_params.text_document.uri.to_string();

        self.documents.remove(&document_uri);
        self.runtime_values_by_document_uri.remove(&document_uri);
        self.mcp_discovery_worker.cancel(&document_uri);
        self.pending_mcp_discoveries.remove(&document_uri);

        let diagnostics_notification = publish_diagnostics_notification(close_params.text_document.uri, Vec::new(), None);

        Ok(RequestOutcome::without_response(Some(diagnostics_notification)))
    }

    fn handle_runtime_values(&mut self, params: Value) -> Result<RequestOutcome, ServerError> {
        let runtime_values_params: RuntimeValuesParams = deserialize_params(params)?;
        let document_uri = runtime_values_params.text_document.uri.to_string();
        let runtime_values = RuntimeValues {
            input: runtime_values_params.input,
            secrets: runtime_values_params.secrets,
        };

        self.runtime_values_by_document_uri.insert(document_uri.clone(), runtime_values);

        let Some((source_text, document_version, previous_mcp_lock)) = self.documents.get(&document_uri).and_then(|document_state| {
            document_state.version().map(|document_version| {
                (
                    Arc::<str>::from(document_state.source_text()),
                    document_version,
                    document_state.mcp_lock(),
                )
            })
        }) else {
            return Ok(RequestOutcome::continue_without_response());
        };

        self.schedule_mcp_discovery(document_uri, document_version, source_text, previous_mcp_lock);

        Ok(RequestOutcome::continue_without_response())
    }

    fn schedule_mcp_discovery(
        &mut self,
        document_uri: String,
        document_version: i32,
        source_text: Arc<str>,
        previous_mcp_lock: Option<McpLock>,
    ) -> bool {
        if self.network_mcp_discovery_trust != NetworkMcpDiscoveryTrust::Trusted {
            self.mcp_discovery_worker.cancel(&document_uri);
            self.pending_mcp_discoveries.remove(&document_uri);

            return false;
        }

        if !self
            .documents
            .get(&document_uri)
            .is_some_and(DocumentState::has_mcp_server_declarations)
        {
            self.mcp_discovery_worker.cancel(&document_uri);
            self.pending_mcp_discoveries.remove(&document_uri);

            return false;
        }

        let runtime_values = self.runtime_values_by_document_uri.get(&document_uri).cloned();

        if self.pending_mcp_discoveries.get(&document_uri).is_some_and(|pending_discovery| {
            pending_discovery.document_version == document_version
                && pending_discovery.source_text == source_text
                && pending_discovery.runtime_values == runtime_values
        }) {
            return false;
        }

        let request_id = self.next_mcp_discovery_request_id;
        self.next_mcp_discovery_request_id = self
            .next_mcp_discovery_request_id
            .checked_add(1)
            .expect("MCP discovery request identifier overflowed");
        let pending_discovery = PendingMcpDiscovery {
            request_id,
            document_version,
            source_text,
            runtime_values,
        };
        let discovery_request = McpDiscoveryRequest {
            pending_discovery: pending_discovery.clone(),
            document_uri: document_uri.clone(),
            previous_mcp_lock,
        };

        let schedule_outcome = self.mcp_discovery_worker.schedule(discovery_request);

        if let Some(evicted_document_uri) = schedule_outcome.evicted_document_uri {
            self.pending_mcp_discoveries.remove(&evicted_document_uri);
        }

        if !schedule_outcome.accepted {
            return false;
        }

        self.pending_mcp_discoveries.insert(document_uri, pending_discovery);

        true
    }

    fn apply_ready_mcp_discovery_results(&mut self) -> Vec<Notification> {
        let discovery_results = self.mcp_discovery_worker.result_receiver.try_iter().collect::<Vec<_>>();

        discovery_results
            .into_iter()
            .filter_map(|discovery_result| self.apply_mcp_discovery_result(discovery_result))
            .collect()
    }

    fn apply_mcp_discovery_result(&mut self, discovery_result: McpDiscoveryResult) -> Option<Notification> {
        let pending_discovery = self.pending_mcp_discoveries.get(&discovery_result.document_uri)?;

        if pending_discovery.request_id != discovery_result.request_id {
            return None;
        }

        let pending_discovery = self.pending_mcp_discoveries.remove(&discovery_result.document_uri)?;

        if pending_discovery.document_version != discovery_result.document_version
            || pending_discovery.source_text != discovery_result.source_text
        {
            return None;
        }

        let document_state = self.documents.get_mut(&discovery_result.document_uri)?;

        if document_state.version() != Some(discovery_result.document_version)
            || document_state.source_text() != discovery_result.source_text.as_ref()
        {
            return None;
        }

        document_state.replace_mcp_lock_if_version_and_source(
            discovery_result.mcp_lock,
            discovery_result.document_version,
            &discovery_result.source_text,
        );

        self.publish_document_diagnostics(&discovery_result.document_uri)
    }

    #[cfg(test)]
    fn receive_and_apply_mcp_discovery_result(&mut self, timeout: Duration) -> Option<Notification> {
        let discovery_result = self.mcp_discovery_worker.receive_result_timeout(timeout)?;

        self.apply_mcp_discovery_result(discovery_result)
    }

    fn handle_completion(&self, request_id: RequestId, params: Value) -> Result<RequestOutcome, ServerError> {
        let completion_params: CompletionParams = deserialize_params(params)?;

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

    fn handle_semantic_tokens(&self, request_id: RequestId, params: Value) -> Result<RequestOutcome, ServerError> {
        let semantic_tokens_params: SemanticTokensParams = deserialize_params(params)?;

        Ok(RequestOutcome::with_response(success_response(
            request_id,
            self.semantic_tokens_result(&semantic_tokens_params),
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
        let version = document_state.version();
        let diagnostics = document_state
            .diagnostics()
            .into_iter()
            .map(|document_diagnostic| {
                let related_information = (!document_diagnostic.related.is_empty()).then(|| {
                    document_diagnostic
                        .related
                        .into_iter()
                        .map(|related_diagnostic| DiagnosticRelatedInformation {
                            location: Location {
                                uri: document_uri.clone(),
                                range: related_diagnostic.range,
                            },
                            message: related_diagnostic.message,
                        })
                        .collect()
                });
                let data = (!document_diagnostic.notes.is_empty() || document_diagnostic.help.is_some()).then(|| {
                    serde_json::json!({
                        "notes": document_diagnostic.notes,
                        "help": document_diagnostic.help,
                    })
                });

                Diagnostic {
                    range: document_diagnostic.range,
                    severity: Some(document_diagnostic.severity),
                    code: Some(document_diagnostic.code.as_lsp_code()),
                    code_description: None,
                    source: Some("superwire-lsp".to_string()),
                    message: document_diagnostic.message,
                    related_information,
                    tags: None,
                    data,
                }
            })
            .collect::<Vec<_>>();

        Some(publish_diagnostics_notification(document_uri, diagnostics, version))
    }

    fn completion_result(&self, completion_params: &CompletionParams) -> CompletionResponse {
        let text_document_position = &completion_params.text_document_position;
        let document_uri = text_document_position.text_document.uri.to_string();
        let Some(document_state) = self.documents.get(&document_uri) else {
            return CompletionResponse::List(CompletionList {
                is_incomplete: false,
                items: Vec::new(),
            });
        };
        let position = text_document_position.position;
        let completion_text_edit_range = document_state.completion_text_edit_range(position);
        let completion_items = document_state
            .completion_suggestions(position)
            .into_iter()
            .map(|completion_suggestion| {
                completion_suggestion.into_lsp_completion_item(completion_text_edit_range, self.completion_snippet_support)
            })
            .collect::<Vec<_>>();

        CompletionResponse::List(CompletionList {
            is_incomplete: document_state.completion_is_incomplete(),
            items: completion_items,
        })
    }

    fn hover_result(&self, hover_params: &TextDocumentPositionParams) -> Option<Hover> {
        let document_uri = hover_params.text_document.uri.to_string();
        let document_state = self.documents.get(&document_uri)?;
        let hover_markdown = document_state.hover_markdown(hover_params.position)?;
        let hover_range = document_state.hover_range(hover_params.position);

        Some(markdown_hover(hover_markdown, hover_range))
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

    fn document_symbols_result(&self, symbol_params: &lsp_types::DocumentSymbolParams) -> DocumentSymbolResponse {
        let document_uri = symbol_params.text_document.uri.to_string();
        let Some(document_state) = self.documents.get(&document_uri) else {
            return if self.hierarchical_document_symbol_support {
                DocumentSymbolResponse::Nested(Vec::new())
            } else {
                DocumentSymbolResponse::Flat(Vec::new())
            };
        };

        if self.hierarchical_document_symbol_support {
            return DocumentSymbolResponse::Nested(
                document_state
                    .document_symbols()
                    .into_iter()
                    .map(DocumentSymbolNode::into_lsp_document_symbol)
                    .collect(),
            );
        }

        DocumentSymbolResponse::Flat(
            document_state
                .workspace_symbols(&document_uri, "")
                .into_iter()
                .filter_map(WorkspaceSymbolMatch::into_lsp_symbol_information)
                .collect(),
        )
    }

    fn semantic_tokens_result(&self, semantic_tokens_params: &SemanticTokensParams) -> Option<SemanticTokensResult> {
        let document_uri = semantic_tokens_params.text_document.uri.to_string();
        let document_state = self.documents.get(&document_uri)?;
        let mut previous_line = 0_u32;
        let mut previous_start = 0_u32;
        let semantic_tokens = document_state
            .semantic_highlights()
            .into_iter()
            .filter_map(|semantic_highlight| {
                if semantic_highlight.range.start.line != semantic_highlight.range.end.line {
                    return None;
                }

                let line = semantic_highlight.range.start.line;
                let start = semantic_highlight.range.start.character;
                let delta_line = line.saturating_sub(previous_line);
                let delta_start = if delta_line == 0 {
                    start.saturating_sub(previous_start)
                } else {
                    start
                };
                let length = semantic_highlight
                    .range
                    .end
                    .character
                    .saturating_sub(semantic_highlight.range.start.character);
                previous_line = line;
                previous_start = start;

                Some(SemanticToken {
                    delta_line,
                    delta_start,
                    length,
                    token_type: semantic_highlight.kind.legend_index(),
                    token_modifiers_bitset: 0,
                })
            })
            .collect();

        Some(SemanticTokensResult::Tokens(SemanticTokens {
            result_id: document_state.version().map(|version| version.to_string()),
            data: semantic_tokens,
        }))
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

    #[cfg(test)]
    fn resolve_mcp_lock(&self, document_uri: &str, source_text: &str, previous_mcp_lock: Option<McpLock>) -> Option<McpLock> {
        self.mcp_discovery_worker.resolve_synchronously(
            document_uri,
            source_text,
            previous_mcp_lock,
            self.runtime_values_by_document_uri.get(document_uri),
        )
    }
}

impl McpDiscoveryWorker {
    fn new(client_factory: Arc<dyn McpClientFactory>) -> Self {
        let scheduler = Arc::new(McpDiscoveryScheduler::new(MCP_DISCOVERY_QUEUE_CAPACITY));
        let (result_sender, result_receiver) = mpsc::sync_channel(MCP_DISCOVERY_RESULT_CAPACITY);

        for worker_index in 0..MCP_DISCOVERY_WORKER_COUNT {
            let worker_scheduler = Arc::clone(&scheduler);
            let worker_result_sender = result_sender.clone();
            let worker_client_factory = Arc::clone(&client_factory);

            std::thread::Builder::new()
                .name(format!("superwire-lsp-mcp-discovery-{worker_index}"))
                .spawn(move || {
                    let mut discovery_cache = McpDiscoveryCache::new(worker_client_factory);

                    while let Some(request) = worker_scheduler.next_request() {
                        let McpDiscoveryRequest {
                            pending_discovery,
                            document_uri,
                            previous_mcp_lock,
                        } = request;
                        let mcp_lock = discovery_cache.resolve_mcp_lock(
                            &document_uri,
                            &pending_discovery.source_text,
                            previous_mcp_lock,
                            pending_discovery.runtime_values.as_ref(),
                        );
                        let discovery_result = McpDiscoveryResult {
                            request_id: pending_discovery.request_id,
                            document_uri: document_uri.clone(),
                            document_version: pending_discovery.document_version,
                            source_text: pending_discovery.source_text,
                            mcp_lock,
                        };
                        let result_sent = worker_result_sender.send(discovery_result).is_ok();

                        worker_scheduler.complete(&document_uri);

                        if !result_sent {
                            break;
                        }
                    }
                })
                .expect("MCP discovery worker thread should start");
        }

        Self {
            scheduler,
            result_receiver,
            #[cfg(test)]
            synchronous_cache: Mutex::new(McpDiscoveryCache::new(client_factory)),
        }
    }

    fn schedule(&self, discovery_request: McpDiscoveryRequest) -> McpDiscoveryScheduleOutcome {
        self.scheduler.schedule(discovery_request)
    }

    fn cancel(&self, document_uri: &str) {
        self.scheduler.cancel(document_uri);
    }

    fn cancel_all(&self) {
        self.scheduler.cancel_all();
    }

    #[cfg(test)]
    fn receive_result_timeout(&self, timeout: Duration) -> Option<McpDiscoveryResult> {
        self.result_receiver.recv_timeout(timeout).ok()
    }

    #[cfg(test)]
    fn resolve_synchronously(
        &self,
        document_uri: &str,
        source_text: &str,
        previous_mcp_lock: Option<McpLock>,
        runtime_values: Option<&RuntimeValues>,
    ) -> Option<McpLock> {
        self.synchronous_cache
            .lock()
            .ok()?
            .resolve_mcp_lock(document_uri, source_text, previous_mcp_lock, runtime_values)
    }
}

impl Drop for McpDiscoveryWorker {
    fn drop(&mut self) {
        self.scheduler.close();
    }
}

impl McpDiscoveryScheduler {
    fn new(capacity: usize) -> Self {
        Self {
            state: Mutex::new(McpDiscoverySchedulerState::default()),
            request_available: Condvar::new(),
            capacity,
        }
    }

    fn schedule(&self, discovery_request: McpDiscoveryRequest) -> McpDiscoveryScheduleOutcome {
        let document_uri = discovery_request.document_uri.clone();
        let Ok(mut scheduler_state) = self.state.lock() else {
            return McpDiscoveryScheduleOutcome {
                accepted: false,
                evicted_document_uri: None,
            };
        };

        if scheduler_state.closed || self.capacity == 0 {
            return McpDiscoveryScheduleOutcome {
                accepted: false,
                evicted_document_uri: None,
            };
        }

        if let Some(pending_request) = scheduler_state.pending_requests_by_document_uri.get_mut(&document_uri) {
            *pending_request = discovery_request;

            return McpDiscoveryScheduleOutcome {
                accepted: true,
                evicted_document_uri: None,
            };
        }

        let mut evicted_document_uri = None;

        while scheduler_state.pending_requests_by_document_uri.len() >= self.capacity {
            let Some(oldest_document_uri) = scheduler_state.ready_document_uris.pop_front() else {
                return McpDiscoveryScheduleOutcome {
                    accepted: false,
                    evicted_document_uri: None,
                };
            };

            if scheduler_state
                .pending_requests_by_document_uri
                .remove(&oldest_document_uri)
                .is_some()
            {
                evicted_document_uri = Some(oldest_document_uri);

                break;
            }
        }

        if !scheduler_state.active_document_uris.contains(&document_uri) {
            scheduler_state.ready_document_uris.push_back(document_uri.clone());
        }

        scheduler_state
            .pending_requests_by_document_uri
            .insert(document_uri, discovery_request);
        self.request_available.notify_one();

        McpDiscoveryScheduleOutcome {
            accepted: true,
            evicted_document_uri,
        }
    }

    fn cancel(&self, document_uri: &str) {
        let Ok(mut scheduler_state) = self.state.lock() else {
            return;
        };

        scheduler_state.pending_requests_by_document_uri.remove(document_uri);
        scheduler_state
            .ready_document_uris
            .retain(|ready_document_uri| ready_document_uri != document_uri);
    }

    fn cancel_all(&self) {
        let Ok(mut scheduler_state) = self.state.lock() else {
            return;
        };

        scheduler_state.pending_requests_by_document_uri.clear();
        scheduler_state.ready_document_uris.clear();
    }

    fn next_request(&self) -> Option<McpDiscoveryRequest> {
        let mut scheduler_state = self.state.lock().ok()?;

        loop {
            if scheduler_state.closed {
                return None;
            }

            while let Some(document_uri) = scheduler_state.ready_document_uris.pop_front() {
                let Some(discovery_request) = scheduler_state.pending_requests_by_document_uri.remove(&document_uri) else {
                    continue;
                };

                scheduler_state.active_document_uris.insert(document_uri);

                return Some(discovery_request);
            }

            scheduler_state = self.request_available.wait(scheduler_state).ok()?;
        }
    }

    fn complete(&self, document_uri: &str) {
        let Ok(mut scheduler_state) = self.state.lock() else {
            return;
        };

        scheduler_state.active_document_uris.remove(document_uri);

        if scheduler_state.pending_requests_by_document_uri.contains_key(document_uri) {
            scheduler_state.ready_document_uris.push_back(document_uri.to_string());
            self.request_available.notify_one();
        }
    }

    fn close(&self) {
        let Ok(mut scheduler_state) = self.state.lock() else {
            return;
        };

        scheduler_state.closed = true;
        scheduler_state.pending_requests_by_document_uri.clear();
        scheduler_state.ready_document_uris.clear();
        self.request_available.notify_all();
    }
}

impl Default for McpDiscoveryWorker {
    fn default() -> Self {
        Self::new(Arc::new(HttpMcpClientFactory))
    }
}

impl McpDiscoveryCache {
    fn new(client_factory: Arc<dyn McpClientFactory>) -> Self {
        Self {
            server_locks_by_config_key: HashMap::new(),
            client_factory,
            capacity: MCP_DISCOVERY_CACHE_CAPACITY_PER_WORKER,
            time_to_live: MCP_DISCOVERY_CACHE_TTL,
            next_access_sequence: 0,
        }
    }

    #[cfg(test)]
    fn with_limits(client_factory: Arc<dyn McpClientFactory>, capacity: usize, time_to_live: Duration) -> Self {
        Self {
            server_locks_by_config_key: HashMap::new(),
            client_factory,
            capacity,
            time_to_live,
            next_access_sequence: 0,
        }
    }

    fn resolve_mcp_lock(
        &mut self,
        document_uri: &str,
        source_text: &str,
        previous_mcp_lock: Option<McpLock>,
        runtime_values: Option<&RuntimeValues>,
    ) -> Option<McpLock> {
        let project_mcp_lock = read_project_mcp_lock(document_uri);

        if let Some(mcp_lock) = self.lock_with_discovered_missing_servers(source_text, project_mcp_lock, runtime_values) {
            return Some(mcp_lock);
        }

        previous_mcp_lock.or_else(|| self.discover_mcp_lock_from_source(source_text, runtime_values))
    }

    fn discover_mcp_lock_from_source(&mut self, source_text: &str, runtime_values: Option<&RuntimeValues>) -> Option<McpLock> {
        let workflow = Self::workflow_for_mcp_discovery(source_text)?;

        if let Some(runtime_values) = runtime_values {
            let lock_resolution_context = runtime_values.lock_resolution_context();

            if let Ok(mcp_lock) = self.discover_from_workflow_with_context(&workflow, &lock_resolution_context) {
                return Some(mcp_lock);
            }
        }

        self.discover_from_workflow(&workflow).ok()
    }

    fn workflow_for_mcp_discovery(source_text: &str) -> Option<Workflow> {
        if let Ok(workflow) = parse_workflow(source_text) {
            return Some(workflow);
        }

        let batch_import_prefix = format!("{} {}", ImportKeyword::From.as_str(), DeclarationKeyword::Mcp.as_str());
        let batch_import_start_indexes = source_text
            .match_indices(&batch_import_prefix)
            .map(|(batch_import_start_index, _)| batch_import_start_index)
            .collect::<Vec<_>>();

        batch_import_start_indexes
            .iter()
            .rev()
            .find_map(|batch_import_start_index| parse_workflow(source_text[..*batch_import_start_index].trim_end()).ok())
    }

    fn lock_with_discovered_missing_servers(
        &mut self,
        source_text: &str,
        project_mcp_lock: Option<McpLock>,
        runtime_values: Option<&RuntimeValues>,
    ) -> Option<McpLock> {
        let Some(mut mcp_lock) = project_mcp_lock else {
            return self.discover_mcp_lock_from_source(source_text, runtime_values);
        };
        let Some(workflow) = Self::workflow_for_mcp_discovery(source_text) else {
            return Some(mcp_lock);
        };
        let missing_server_names = workflow
            .declarations()
            .iter()
            .filter_map(|declaration| match declaration {
                superwire_dsl::Declaration::McpServer(mcp_server_declaration) => Some(mcp_server_declaration.name.as_str()),
                _ => None,
            })
            .filter(|server_name| !mcp_lock.servers.contains_key(*server_name))
            .collect::<Vec<_>>();

        if missing_server_names.is_empty() {
            return Some(mcp_lock);
        }

        let discovered_mcp_lock = self.discover_mcp_lock_from_source(source_text, runtime_values)?;

        for missing_server_name in missing_server_names {
            if let Some(server_lock) = discovered_mcp_lock.servers.get(missing_server_name) {
                mcp_lock.servers.insert(missing_server_name.to_string(), server_lock.clone());
            }
        }

        Some(mcp_lock)
    }

    fn discover_from_workflow(&mut self, workflow: &Workflow) -> Result<McpLock, superwire_mcp::McpError> {
        let evaluation_context = McpLockResolutionContext::default().to_evaluation_context();
        let client_factory = Arc::clone(&self.client_factory);
        let request_scope = McpClientRequestScope::from_workflow(client_factory.as_ref(), workflow, &evaluation_context)?;
        let mut mcp_lock = McpLock::empty();

        for server_config in McpServerConfig::from_workflow(workflow)? {
            let server_lock = self.server_lock_for_config(&server_config, &request_scope)?;

            mcp_lock.servers.insert(server_config.name, server_lock);
        }

        Ok(mcp_lock)
    }

    fn discover_from_workflow_with_context(
        &mut self,
        workflow: &Workflow,
        lock_resolution_context: &McpLockResolutionContext,
    ) -> Result<McpLock, superwire_mcp::McpError> {
        let evaluation_context = lock_resolution_context.to_evaluation_context();
        let client_factory = Arc::clone(&self.client_factory);
        let request_scope = McpClientRequestScope::from_workflow(client_factory.as_ref(), workflow, &evaluation_context)?;
        let mut mcp_lock = McpLock::empty();

        for declaration in workflow.declarations() {
            let superwire_dsl::Declaration::McpServer(mcp_server_declaration) = declaration else {
                continue;
            };

            let server_config = McpServerConfig::resolve_from_declaration(mcp_server_declaration, &evaluation_context)?;
            let server_lock = self.server_lock_for_config(&server_config, &request_scope)?;

            mcp_lock.servers.insert(server_config.name, server_lock);
        }

        Ok(mcp_lock)
    }

    fn server_lock_for_config(
        &mut self,
        server_config: &McpServerConfig,
        client_factory: &dyn McpClientFactory,
    ) -> Result<McpServerLock, superwire_mcp::McpError> {
        let config_key = McpDiscoveryCacheKey::from_server_config(server_config);
        let current_time = Instant::now();

        self.evict_expired(current_time);

        let access_sequence = self.next_access_sequence();

        if let Some(cached_server_lock) = self.server_locks_by_config_key.get_mut(&config_key) {
            cached_server_lock.last_accessed_at = current_time;
            cached_server_lock.access_sequence = access_sequence;

            return Ok(cached_server_lock.server_lock.clone());
        }

        let server_lock = client_factory.client_for_config(server_config.clone())?.list_tools()?;

        if self.capacity > 0 {
            self.evict_least_recently_used_if_full();
            self.server_locks_by_config_key.insert(
                config_key,
                CachedMcpServerLock {
                    server_lock: server_lock.clone(),
                    last_accessed_at: current_time,
                    access_sequence,
                },
            );
        }

        Ok(server_lock)
    }

    fn evict_expired(&mut self, current_time: Instant) {
        let time_to_live = self.time_to_live;

        self.server_locks_by_config_key
            .retain(|_, cached_server_lock| current_time.saturating_duration_since(cached_server_lock.last_accessed_at) <= time_to_live);
    }

    fn evict_least_recently_used_if_full(&mut self) {
        if self.server_locks_by_config_key.len() < self.capacity {
            return;
        }

        let least_recently_used_key = self
            .server_locks_by_config_key
            .iter()
            .min_by_key(|(_, cached_server_lock)| cached_server_lock.access_sequence)
            .map(|(config_key, _)| *config_key);

        if let Some(least_recently_used_key) = least_recently_used_key {
            self.server_locks_by_config_key.remove(&least_recently_used_key);
        }
    }

    fn next_access_sequence(&mut self) -> u64 {
        let access_sequence = self.next_access_sequence;
        self.next_access_sequence = self
            .next_access_sequence
            .checked_add(1)
            .expect("MCP discovery cache access sequence overflowed");

        access_sequence
    }
}

impl McpDiscoveryCacheKey {
    fn from_server_config(server_config: &McpServerConfig) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(b"superwire-mcp-server-config-v1");

        let mut hash_component = |component: &[u8]| {
            hasher.update(u64::try_from(component.len()).unwrap_or(u64::MAX).to_be_bytes());
            hasher.update(component);
        };

        hash_component(server_config.name.as_bytes());
        hash_component(server_config.endpoint.as_bytes());
        hash_component(&u64::try_from(server_config.headers.len()).unwrap_or(u64::MAX).to_be_bytes());

        for (header_name, header_value) in &server_config.headers {
            hash_component(header_name.as_bytes());
            hash_component(header_value.as_bytes());
        }

        Self(hasher.finalize().into())
    }
}

impl Default for McpDiscoveryCache {
    fn default() -> Self {
        Self::new(Arc::new(HttpMcpClientFactory))
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
    fn into_lsp_completion_item(self, completion_text_edit_range: Option<lsp_types::Range>, snippet_support: bool) -> CompletionItem {
        let insert_text_uses_snippet_format = Self::uses_snippet_format(&self.insert_text);
        let insert_text = if insert_text_uses_snippet_format && !snippet_support {
            Self::plain_insert_text(&self.insert_text)
        } else {
            self.insert_text
        };
        let mut completion_item = CompletionItem {
            label: self.label,
            kind: Some(self.kind),
            detail: Some(self.detail),
            documentation: Some(lsp_types::Documentation::MarkupContent(MarkupContent {
                kind: MarkupKind::Markdown,
                value: self.documentation,
            })),
            ..Default::default()
        };

        if let Some(text_edit_range) = completion_text_edit_range {
            completion_item.text_edit = Some(CompletionTextEdit::Edit(TextEdit {
                range: text_edit_range,
                new_text: insert_text,
            }));
        } else {
            completion_item.insert_text = Some(insert_text);
        }

        if insert_text_uses_snippet_format && snippet_support {
            completion_item.insert_text_format = Some(InsertTextFormat::SNIPPET);
        }

        completion_item
    }

    fn uses_snippet_format(insert_text: &str) -> bool {
        let mut remaining_text = insert_text;

        while let Some(dollar_offset) = remaining_text.find('$') {
            let text_after_dollar = &remaining_text[dollar_offset + 1..];
            let starts_unbraced_placeholder = text_after_dollar.chars().next().is_some_and(|character| character.is_ascii_digit());
            let starts_braced_placeholder = text_after_dollar
                .strip_prefix('{')
                .and_then(|placeholder_text| placeholder_text.chars().next())
                .is_some_and(|character| character.is_ascii_digit());

            if starts_unbraced_placeholder || starts_braced_placeholder {
                return true;
            }

            remaining_text = text_after_dollar;
        }

        false
    }

    fn plain_insert_text(snippet_text: &str) -> String {
        let mut plain_text = String::with_capacity(snippet_text.len());
        let mut remaining_text = snippet_text;

        while !remaining_text.is_empty() {
            if let Some(text_after_dollar) = remaining_text.strip_prefix('$') {
                let digit_count = text_after_dollar.bytes().take_while(u8::is_ascii_digit).count();

                if digit_count > 0 {
                    remaining_text = &text_after_dollar[digit_count..];

                    continue;
                }

                if let Some(braced_placeholder_text) = text_after_dollar.strip_prefix('{') {
                    let digit_count = braced_placeholder_text.bytes().take_while(u8::is_ascii_digit).count();
                    let placeholder_suffix = &braced_placeholder_text[digit_count..];

                    if digit_count > 0 {
                        if let Some(default_and_closing_text) = placeholder_suffix.strip_prefix(':') {
                            if let Some(closing_brace_offset) = default_and_closing_text.find('}') {
                                plain_text.push_str(&default_and_closing_text[..closing_brace_offset]);
                                remaining_text = &default_and_closing_text[closing_brace_offset + 1..];

                                continue;
                            }
                        } else if let Some(text_after_closing_brace) = placeholder_suffix.strip_prefix('}') {
                            remaining_text = text_after_closing_brace;

                            continue;
                        }
                    }
                }
            }

            let next_character = remaining_text
                .chars()
                .next()
                .expect("non-empty completion text should contain a character");
            plain_text.push(next_character);
            remaining_text = &remaining_text[next_character.len_utf8()..];
        }

        plain_text
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

fn publish_diagnostics_notification(uri: Uri, diagnostics: Vec<Diagnostic>, version: Option<i32>) -> Notification {
    Notification {
        method: lsp_types::notification::PublishDiagnostics::METHOD.to_string(),
        params: serde_json::to_value(PublishDiagnosticsParams { uri, diagnostics, version })
            .expect("publish diagnostics params should serialize"),
    }
}

fn initialize_result(position_encoding: PositionEncoding) -> InitializeResult {
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
                trigger_characters: Some([".", ":", "\"", "{", "(", ",", "?"].into_iter().map(ToOwned::to_owned).collect()),
                ..Default::default()
            }),
            code_lens_provider: Some(CodeLensOptions {
                resolve_provider: Some(false),
            }),
            execute_command_provider: Some(ExecuteCommandOptions {
                commands: vec!["superwire.generated.output".to_string()],
                ..Default::default()
            }),
            semantic_tokens_provider: Some(SemanticTokensServerCapabilities::SemanticTokensOptions(SemanticTokensOptions {
                work_done_progress_options: WorkDoneProgressOptions::default(),
                legend: SemanticTokensLegend {
                    token_types: vec![
                        SemanticTokenType::KEYWORD,
                        SemanticTokenType::TYPE,
                        SemanticTokenType::CLASS,
                        SemanticTokenType::PROPERTY,
                        SemanticTokenType::FUNCTION,
                        SemanticTokenType::VARIABLE,
                        SemanticTokenType::STRING,
                        SemanticTokenType::NUMBER,
                        SemanticTokenType::COMMENT,
                        SemanticTokenType::OPERATOR,
                        SemanticTokenType::ENUM_MEMBER,
                        SemanticTokenType::NAMESPACE,
                    ],
                    token_modifiers: Vec::new(),
                },
                range: Some(false),
                full: Some(SemanticTokensFullOptions::Bool(true)),
            })),
            position_encoding: Some(position_encoding.as_kind()),
            experimental: Some(serde_json::json!({
                "superwire": {
                    "initializationOptions": {
                        "workspaceTrust": {
                            "networkMcpDiscovery": {
                                "default": NetworkMcpDiscoveryTrust::Disabled.as_str(),
                                "supportedValues": [
                                    NetworkMcpDiscoveryTrust::Disabled.as_str(),
                                    NetworkMcpDiscoveryTrust::Trusted.as_str()
                                ],
                                "description": "Allows workflow MCP endpoint and runtime header network access only for explicitly trusted workspaces."
                            }
                        }
                    }
                }
            })),
            ..Default::default()
        },
        server_info: Some(ServerInfo {
            name: "superwire-lsp".to_string(),
            version: Some(env!("CARGO_PKG_VERSION").to_string()),
        }),
    }
}

fn markdown_hover(markdown: String, range: Option<lsp_types::Range>) -> Hover {
    Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value: markdown,
        }),
        range,
    }
}

fn read_project_mcp_lock(document_uri: &str) -> Option<McpLock> {
    let workflow_path = path_for_document_uri(document_uri)?;
    let lock_path = ProjectMcpLock::discover_lock_path_for_workflow(&workflow_path)?;
    let lock_root = lock_path.parent()?;
    let project_lock = ProjectMcpLock::read_from_path(&lock_path).ok()?;

    project_lock.workflow_lock(lock_root, &workflow_path).cloned()
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
