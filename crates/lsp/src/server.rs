use std::collections::HashMap;
use std::path::PathBuf;

use serde_json::{json, Value};
use superwire_core::mcp::{McpLock, ProjectMcpLock};
use thiserror::Error;
use tokio::io::{stdin, stdout, AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader, BufWriter, Stdin, Stdout};

use crate::document::{
    CodeActionSuggestion, CodeLensHint, CompletionSuggestion, DocumentState, DocumentSymbolNode, FoldingRangeBlock, WorkspaceSymbolMatch,
};
use crate::protocol::{
    error_response, publish_diagnostics_notification, success_response, CodeActionParams, CodeLens, CodeLensParams, Command, Diagnostic,
    DidChangeTextDocumentParams, DidCloseTextDocumentParams, DidOpenTextDocumentParams, DocumentFormattingParams,
    DocumentSymbol as ProtocolDocumentSymbol, DocumentSymbolParams, ExecuteCommandParams, FoldingRange, FoldingRangeParams, JsonRpcRequest,
    Location, Range, SymbolInformation, TextDocumentPositionParams, TextEdit, WorkspaceSymbolParams,
};

#[derive(Debug, Error)]
pub enum ServerError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

#[derive(Debug)]
struct RequestOutcome {
    response: Option<Value>,
    notifications: Vec<Value>,
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
        let request: JsonRpcRequest = serde_json::from_slice(raw_message)?;
        let outcome = self.handle_request(request)?;
        let mut messages = Vec::new();

        if let Some(response) = outcome.response {
            messages.push(response);
        }

        messages.extend(outcome.notifications);

        Ok(ServerMessages {
            messages,
            should_exit: outcome.should_exit,
        })
    }

    pub async fn run_stdio() -> Result<(), ServerError> {
        let input_reader = BufReader::new(stdin());
        let output_writer = BufWriter::new(stdout());
        let mut message_reader = MessageReader::new(input_reader);
        let mut message_writer = MessageWriter::new(output_writer);
        let mut language_server = Self::default();

        while let Some(raw_message) = message_reader.read_message().await? {
            let server_messages = language_server.handle_json_rpc_message(&raw_message)?;

            for message in server_messages.messages {
                message_writer.write_message(&message).await?;
            }

            if server_messages.should_exit {
                break;
            }
        }

        Ok(())
    }

    fn handle_request(&mut self, request: JsonRpcRequest) -> Result<RequestOutcome, ServerError> {
        log::debug!("handling LSP method {}", request.method);

        match request.method.as_str() {
            "initialize" => Ok(self.initialize_outcome(request.id)),
            "initialized" => Ok(RequestOutcome::continue_without_response()),
            "shutdown" => Ok(self.shutdown_outcome(request.id)),
            "exit" => Ok(RequestOutcome::exit_without_response()),
            "textDocument/didOpen" => self.handle_did_open(request.params),
            "textDocument/didChange" => self.handle_did_change(request.params),
            "textDocument/didClose" => self.handle_did_close(request.params),
            "textDocument/completion" => self.handle_completion(request.id, request.params),
            "textDocument/hover" => self.handle_hover(request.id, request.params),
            "textDocument/definition" => self.handle_definition(request.id, request.params),
            "textDocument/documentSymbol" => self.handle_document_symbols(request.id, request.params),
            "workspace/symbol" => self.handle_workspace_symbols(request.id, request.params),
            "textDocument/foldingRange" => self.handle_folding_ranges(request.id, request.params),
            "textDocument/formatting" => self.handle_formatting(request.id, request.params),
            "textDocument/codeAction" => self.handle_code_action(request.id, request.params),
            "textDocument/codeLens" => self.handle_code_lens(request.id, request.params),
            "workspace/executeCommand" => self.handle_execute_command(request.id, request.params),
            _ => Ok(self.method_not_found_outcome(request.id)),
        }
    }

    fn initialize_outcome(&self, request_id: Option<Value>) -> RequestOutcome {
        RequestOutcome {
            response: request_id.map(|request_id| success_response(request_id, initialize_result())),
            notifications: Vec::new(),
            should_exit: false,
        }
    }

    fn shutdown_outcome(&self, request_id: Option<Value>) -> RequestOutcome {
        RequestOutcome {
            response: request_id.map(|request_id| success_response(request_id, Value::Null)),
            notifications: Vec::new(),
            should_exit: false,
        }
    }

    fn method_not_found_outcome(&self, request_id: Option<Value>) -> RequestOutcome {
        RequestOutcome {
            response: request_id.map(|request_id| error_response(request_id, -32601, "Method not found")),
            notifications: Vec::new(),
            should_exit: false,
        }
    }

    fn handle_did_open(&mut self, params: Value) -> Result<RequestOutcome, ServerError> {
        let open_params: DidOpenTextDocumentParams = serde_json::from_value(params)?;

        let mcp_lock = read_project_mcp_lock(&open_params.text_document.uri);

        self.documents.insert(
            open_params.text_document.uri.clone(),
            DocumentState::new(open_params.text_document.text, mcp_lock),
        );

        let diagnostics_notification = self.publish_document_diagnostics(open_params.text_document.uri.as_str());

        Ok(RequestOutcome::without_response(diagnostics_notification))
    }

    fn handle_did_change(&mut self, params: Value) -> Result<RequestOutcome, ServerError> {
        let change_params: DidChangeTextDocumentParams = serde_json::from_value(params)?;

        if let Some(last_change) = change_params.content_changes.last() {
            let mcp_lock = read_project_mcp_lock(&change_params.text_document.uri).or_else(|| {
                self.documents
                    .get(&change_params.text_document.uri)
                    .and_then(DocumentState::mcp_lock)
            });

            if let Some(document_state) = self.documents.get_mut(&change_params.text_document.uri) {
                document_state.replace_text(last_change.text.clone(), mcp_lock);
            } else {
                self.documents.insert(
                    change_params.text_document.uri.clone(),
                    DocumentState::new(last_change.text.clone(), mcp_lock),
                );
            }
        }

        let diagnostics_notification = self.publish_document_diagnostics(change_params.text_document.uri.as_str());

        Ok(RequestOutcome::without_response(diagnostics_notification))
    }

    fn handle_did_close(&mut self, params: Value) -> Result<RequestOutcome, ServerError> {
        let close_params: DidCloseTextDocumentParams = serde_json::from_value(params)?;

        self.documents.remove(&close_params.text_document.uri);

        let diagnostics_notification = publish_diagnostics_notification(&close_params.text_document.uri, Vec::new());

        Ok(RequestOutcome::without_response(Some(diagnostics_notification)))
    }

    fn handle_completion(&self, request_id: Option<Value>, params: Value) -> Result<RequestOutcome, ServerError> {
        let completion_params: TextDocumentPositionParams = serde_json::from_value(params)?;

        Ok(RequestOutcome {
            response: Some(success_response(
                request_id.unwrap_or(Value::Null),
                self.completion_result(&completion_params),
            )),
            notifications: Vec::new(),
            should_exit: false,
        })
    }

    fn handle_hover(&self, request_id: Option<Value>, params: Value) -> Result<RequestOutcome, ServerError> {
        let hover_params: TextDocumentPositionParams = serde_json::from_value(params)?;

        Ok(RequestOutcome {
            response: Some(success_response(
                request_id.unwrap_or(Value::Null),
                self.hover_result(&hover_params).unwrap_or(Value::Null),
            )),
            notifications: Vec::new(),
            should_exit: false,
        })
    }

    fn handle_definition(&self, request_id: Option<Value>, params: Value) -> Result<RequestOutcome, ServerError> {
        let definition_params: TextDocumentPositionParams = serde_json::from_value(params)?;

        Ok(RequestOutcome {
            response: Some(success_response(
                request_id.unwrap_or(Value::Null),
                self.definition_result(&definition_params).unwrap_or(Value::Null),
            )),
            notifications: Vec::new(),
            should_exit: false,
        })
    }

    fn handle_document_symbols(&self, request_id: Option<Value>, params: Value) -> Result<RequestOutcome, ServerError> {
        let symbol_params: DocumentSymbolParams = serde_json::from_value(params)?;

        Ok(RequestOutcome {
            response: Some(success_response(
                request_id.unwrap_or(Value::Null),
                self.document_symbols_result(&symbol_params),
            )),
            notifications: Vec::new(),
            should_exit: false,
        })
    }

    fn handle_workspace_symbols(&self, request_id: Option<Value>, params: Value) -> Result<RequestOutcome, ServerError> {
        let workspace_symbol_params: WorkspaceSymbolParams = serde_json::from_value(params)?;

        Ok(RequestOutcome {
            response: Some(success_response(
                request_id.unwrap_or(Value::Null),
                self.workspace_symbols_result(&workspace_symbol_params),
            )),
            notifications: Vec::new(),
            should_exit: false,
        })
    }

    fn handle_folding_ranges(&self, request_id: Option<Value>, params: Value) -> Result<RequestOutcome, ServerError> {
        let folding_range_params: FoldingRangeParams = serde_json::from_value(params)?;

        Ok(RequestOutcome {
            response: Some(success_response(
                request_id.unwrap_or(Value::Null),
                self.folding_ranges_result(&folding_range_params),
            )),
            notifications: Vec::new(),
            should_exit: false,
        })
    }

    fn handle_formatting(&self, request_id: Option<Value>, params: Value) -> Result<RequestOutcome, ServerError> {
        let formatting_params: DocumentFormattingParams = serde_json::from_value(params)?;

        Ok(RequestOutcome {
            response: Some(success_response(
                request_id.unwrap_or(Value::Null),
                self.formatting_result(&formatting_params),
            )),
            notifications: Vec::new(),
            should_exit: false,
        })
    }

    fn handle_code_lens(&self, request_id: Option<Value>, params: Value) -> Result<RequestOutcome, ServerError> {
        let code_lens_params: CodeLensParams = serde_json::from_value(params)?;

        Ok(RequestOutcome {
            response: Some(success_response(
                request_id.unwrap_or(Value::Null),
                self.code_lens_result(&code_lens_params),
            )),
            notifications: Vec::new(),
            should_exit: false,
        })
    }

    fn handle_code_action(&self, request_id: Option<Value>, params: Value) -> Result<RequestOutcome, ServerError> {
        let code_action_params: CodeActionParams = serde_json::from_value(params)?;

        Ok(RequestOutcome {
            response: Some(success_response(
                request_id.unwrap_or(Value::Null),
                self.code_action_result(&code_action_params),
            )),
            notifications: Vec::new(),
            should_exit: false,
        })
    }

    fn handle_execute_command(&self, request_id: Option<Value>, params: Value) -> Result<RequestOutcome, ServerError> {
        let _execute_command_params: ExecuteCommandParams = serde_json::from_value(params)?;

        Ok(RequestOutcome {
            response: Some(success_response(request_id.unwrap_or(Value::Null), Value::Null)),
            notifications: Vec::new(),
            should_exit: false,
        })
    }

    fn publish_document_diagnostics(&self, document_uri: &str) -> Option<Value> {
        let document_state = self.documents.get(document_uri)?;
        let diagnostics = document_state
            .diagnostics()
            .into_iter()
            .map(|document_diagnostic| Diagnostic {
                range: document_diagnostic.range,
                severity: document_diagnostic.severity.as_lsp_severity(),
                code: document_diagnostic.code,
                source: "superwire-lsp".to_string(),
                message: document_diagnostic.message,
            })
            .collect::<Vec<_>>();

        Some(publish_diagnostics_notification(document_uri, diagnostics))
    }

    fn completion_result(&self, completion_params: &TextDocumentPositionParams) -> Value {
        let Some(document_state) = self.documents.get(&completion_params.text_document.uri) else {
            return json!({
                "isIncomplete": false,
                "items": [],
            });
        };

        let completion_text_edit_range = document_state.completion_text_edit_range(completion_params.position);

        let completion_items = document_state
            .completion_suggestions(completion_params.position)
            .into_iter()
            .map(|completion_suggestion| completion_item_to_value(completion_suggestion, completion_text_edit_range))
            .collect::<Vec<_>>();

        json!({
            "isIncomplete": false,
            "items": completion_items,
        })
    }

    fn hover_result(&self, hover_params: &TextDocumentPositionParams) -> Option<Value> {
        let document_state = self.documents.get(&hover_params.text_document.uri)?;
        let hover_markdown = document_state.hover_markdown(hover_params.position)?;

        Some(markdown_hover(&hover_markdown))
    }

    fn definition_result(&self, definition_params: &TextDocumentPositionParams) -> Option<Value> {
        let document_state = self.documents.get(&definition_params.text_document.uri)?;
        let definition_range = document_state.definition_range(definition_params.position)?;

        let location = Location {
            uri: definition_params.text_document.uri.clone(),
            range: definition_range,
        };

        Some(json!([location]))
    }

    fn document_symbols_result(&self, symbol_params: &DocumentSymbolParams) -> Value {
        let Some(document_state) = self.documents.get(&symbol_params.text_document.uri) else {
            return json!([]);
        };

        let symbol_nodes = document_state.document_symbols();
        let document_symbols = symbol_nodes.into_iter().map(document_symbol_node_to_protocol).collect::<Vec<_>>();

        json!(document_symbols)
    }

    fn workspace_symbols_result(&self, workspace_symbol_params: &WorkspaceSymbolParams) -> Value {
        let mut workspace_symbols = self
            .documents
            .iter()
            .flat_map(|(document_uri, document_state)| {
                document_state.workspace_symbols(document_uri, workspace_symbol_params.query.as_str())
            })
            .collect::<Vec<_>>();

        workspace_symbols.sort_by(|left_symbol, right_symbol| left_symbol.name.cmp(&right_symbol.name));

        let symbol_information = workspace_symbols
            .into_iter()
            .map(workspace_symbol_match_to_information)
            .collect::<Vec<_>>();

        json!(symbol_information)
    }

    fn folding_ranges_result(&self, folding_range_params: &FoldingRangeParams) -> Value {
        let Some(document_state) = self.documents.get(&folding_range_params.text_document.uri) else {
            return json!([]);
        };

        let folding_ranges = document_state
            .folding_ranges()
            .into_iter()
            .map(folding_range_block_to_protocol)
            .collect::<Vec<_>>();

        json!(folding_ranges)
    }

    fn formatting_result(&self, formatting_params: &DocumentFormattingParams) -> Value {
        let Some(document_state) = self.documents.get(&formatting_params.text_document.uri) else {
            return json!([]);
        };

        let Some(formatting_edit) = document_state.formatting_edit() else {
            return json!([]);
        };

        let text_edit = TextEdit {
            range: formatting_edit.range,
            new_text: formatting_edit.new_text,
        };

        json!([text_edit])
    }

    fn code_lens_result(&self, code_lens_params: &CodeLensParams) -> Value {
        let Some(document_state) = self.documents.get(&code_lens_params.text_document.uri) else {
            return json!([]);
        };

        let code_lenses = document_state
            .generated_output_marks()
            .into_iter()
            .map(code_lens_hint_to_protocol)
            .collect::<Vec<_>>();

        json!(code_lenses)
    }

    fn code_action_result(&self, code_action_params: &CodeActionParams) -> Value {
        let Some(document_state) = self.documents.get(&code_action_params.text_document.uri) else {
            return json!([]);
        };

        let code_actions = document_state
            .code_actions(code_action_params.range.start)
            .into_iter()
            .map(|code_action| code_action_to_protocol(code_action, &code_action_params.text_document.uri))
            .collect::<Vec<_>>();

        json!(code_actions)
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

    fn without_response(optional_notification: Option<Value>) -> Self {
        Self {
            response: None,
            notifications: optional_notification.into_iter().collect(),
            should_exit: false,
        }
    }
}

struct MessageReader {
    reader: BufReader<Stdin>,
}

impl MessageReader {
    fn new(reader: BufReader<Stdin>) -> Self {
        Self { reader }
    }

    async fn read_message(&mut self) -> Result<Option<Vec<u8>>, ServerError> {
        let mut content_length = None;

        loop {
            let mut header_line = String::new();
            let read_count = self.reader.read_line(&mut header_line).await?;

            if read_count == 0 {
                return Ok(None);
            }

            if header_line == "\r\n" {
                break;
            }

            if let Some(header_value) = header_line.strip_prefix("Content-Length:") {
                let parsed_length = header_value
                    .trim()
                    .parse::<usize>()
                    .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;

                content_length = Some(parsed_length);
            }
        }

        let Some(message_length) = content_length else {
            return Ok(None);
        };

        let mut message_buffer = vec![0_u8; message_length];
        self.reader.read_exact(&mut message_buffer).await?;

        Ok(Some(message_buffer))
    }
}

struct MessageWriter {
    writer: BufWriter<Stdout>,
}

impl MessageWriter {
    fn new(writer: BufWriter<Stdout>) -> Self {
        Self { writer }
    }

    async fn write_message(&mut self, message: &Value) -> Result<(), ServerError> {
        let encoded_message = serde_json::to_vec(message)?;
        let header = format!("Content-Length: {}\r\n\r\n", encoded_message.len());

        self.writer.write_all(header.as_bytes()).await?;
        self.writer.write_all(&encoded_message).await?;
        self.writer.flush().await?;

        Ok(())
    }
}

fn initialize_result() -> Value {
    json!({
        "capabilities": {
            "textDocumentSync": {
                "openClose": true,
                "change": 1
            },
            "hoverProvider": true,
            "definitionProvider": true,
            "documentSymbolProvider": true,
            "workspaceSymbolProvider": true,
            "foldingRangeProvider": true,
            "documentFormattingProvider": true,
            "codeActionProvider": true,
            "completionProvider": {
                "resolveProvider": false,
                "triggerCharacters": [".", ":", "\""]
            },
            "codeLensProvider": {
                "resolveProvider": false
            },
            "executeCommandProvider": {
                "commands": ["superwire.generated.output"]
            }
        },
        "serverInfo": {
            "name": "superwire-lsp",
            "version": "0.2.0"
        }
    })
}

fn document_symbol_node_to_protocol(document_symbol_node: DocumentSymbolNode) -> ProtocolDocumentSymbol {
    ProtocolDocumentSymbol {
        name: document_symbol_node.name,
        detail: document_symbol_node.detail,
        kind: document_symbol_node.kind.as_lsp_kind(),
        range: document_symbol_node.range,
        selection_range: document_symbol_node.selection_range,
        children: document_symbol_node
            .children
            .into_iter()
            .map(document_symbol_node_to_protocol)
            .collect(),
    }
}

fn workspace_symbol_match_to_information(workspace_symbol_match: WorkspaceSymbolMatch) -> SymbolInformation {
    SymbolInformation {
        name: workspace_symbol_match.name,
        kind: workspace_symbol_match.kind.as_lsp_kind(),
        location: Location {
            uri: workspace_symbol_match.document_uri,
            range: workspace_symbol_match.range,
        },
        container_name: workspace_symbol_match.container_name,
    }
}

fn folding_range_block_to_protocol(folding_range_block: FoldingRangeBlock) -> FoldingRange {
    FoldingRange {
        start_line: folding_range_block.start_line,
        start_character: Some(folding_range_block.start_character),
        end_line: folding_range_block.end_line,
        end_character: Some(folding_range_block.end_character),
        kind: Some("region".to_string()),
    }
}

fn code_lens_hint_to_protocol(code_lens_hint: CodeLensHint) -> CodeLens {
    CodeLens {
        range: code_lens_hint.range,
        command: Command {
            title: code_lens_hint.title,
            command: code_lens_hint.command,
            arguments: Vec::new(),
        },
    }
}

fn code_action_to_protocol(code_action: CodeActionSuggestion, document_uri: &str) -> Value {
    let mut changes = serde_json::Map::new();
    changes.insert(
        document_uri.to_string(),
        json!([{
            "range": code_action.edit.range,
            "newText": code_action.edit.new_text,
        }]),
    );

    json!({
        "title": code_action.title,
        "kind": "quickfix",
        "edit": {
            "changes": changes
        }
    })
}

fn completion_item_to_value(completion_suggestion: CompletionSuggestion, completion_text_edit_range: Option<Range>) -> Value {
    let CompletionSuggestion {
        label,
        kind,
        detail,
        documentation,
        insert_text,
    } = completion_suggestion;

    let insert_text_uses_snippet_format = insert_text.contains("$1");
    let mut completion_item = json!({
        "label": label,
        "kind": kind.as_lsp_kind(),
        "detail": detail,
        "documentation": documentation,
    });

    if let Some(completion_item_object) = completion_item.as_object_mut() {
        if let Some(text_edit_range) = completion_text_edit_range {
            completion_item_object.insert(
                "textEdit".to_string(),
                json!({
                    "range": text_edit_range,
                    "newText": insert_text,
                }),
            );
        } else {
            completion_item_object.insert("insertText".to_string(), json!(insert_text));
        }

        if insert_text_uses_snippet_format {
            completion_item_object.insert("insertTextFormat".to_string(), json!(2));
        }
    }

    completion_item
}

fn markdown_hover(markdown: &str) -> Value {
    json!({
        "contents": {
            "kind": "markdown",
            "value": markdown,
        }
    })
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
mod tests {
    use super::read_project_mcp_lock;
    use serde_json::Value;
    use std::collections::BTreeMap;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::thread;
    use superwire_core::mcp::{McpLock, McpLockResolutionContext, ProjectMcpLock};
    use superwire_core::workflow_source;

    #[test]
    fn reads_mcp_lock_from_project_lock_without_refreshing() {
        let server = TestMcpHttpServer::spawn();
        let workflow_source = workflow_source! {
            secrets {
                mcp_endpoint: string
            }

            mcp local {
                endpoint: secrets.mcp_endpoint
                headers: {
                    Accept: "application/json"
                }
            }

            tool update_user_name from mcp.local.tool.update_user_name
        };
        let temp_directory_path = std::env::temp_dir().join(format!(
            "superwire_lsp_lock_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("current time should be after unix epoch")
                .as_nanos()
        ));
        std::fs::create_dir_all(&temp_directory_path).expect("temporary directory should be created");
        let temp_file_path = temp_directory_path.join("dynamic.wire");
        std::fs::write(&temp_file_path, workflow_source).expect("temporary workflow should write");
        let document_uri = format!("file://{}", temp_file_path.display());
        let lock_path = temp_directory_path.join("superwire.lock");
        let lock_context = McpLockResolutionContext {
            input: BTreeMap::new(),
            secrets: [("mcp_endpoint".to_string(), Value::String(server.endpoint()))]
                .into_iter()
                .collect(),
            dynamic: BTreeMap::new(),
            agent_outputs: BTreeMap::new(),
            agent_contexts: BTreeMap::new(),
        };
        let discovered_lock = McpLock::discover_from_workflow_with_lock_context(
            &superwire_core::dsl::parse_workflow(workflow_source).expect("workflow should parse"),
            Some(&lock_context),
        )
        .expect("MCP metadata should discover using lock context");
        let mut project_lock = ProjectMcpLock::empty();

        project_lock.insert_workflow_lock(
            temp_file_path.parent().expect("temporary workflow should have parent"),
            &temp_file_path,
            discovered_lock,
        );
        project_lock.write_to_path(&lock_path).expect("project lock should write");

        let read_lock = read_project_mcp_lock(&document_uri).expect("project lock should read");

        assert!(read_lock.servers.contains_key("local"));
        assert!(!temp_file_path.with_extension("wire.lock").exists());

        let _ = std::fs::remove_dir_all(&temp_directory_path);
    }

    struct TestMcpHttpServer {
        endpoint: String,
    }

    impl TestMcpHttpServer {
        fn spawn() -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").expect("test MCP listener should bind");
            let endpoint = format!("http://{}", listener.local_addr().expect("local address should exist"));

            thread::spawn(move || {
                for incoming_stream in listener.incoming().take(12) {
                    let stream = incoming_stream.expect("test MCP stream should open");
                    handle_mcp_request(stream);
                }
            });

            Self { endpoint }
        }

        fn endpoint(&self) -> String {
            self.endpoint.clone()
        }
    }

    fn handle_mcp_request(mut stream: TcpStream) {
        let mut reader = BufReader::new(stream.try_clone().expect("stream clone should succeed"));
        let mut content_length = 0_usize;
        let mut header_line = String::new();

        loop {
            header_line.clear();
            reader.read_line(&mut header_line).expect("header line should read");

            if header_line == "\r\n" || header_line.is_empty() {
                break;
            }

            if let Some(value) = header_line.to_ascii_lowercase().strip_prefix("content-length:") {
                content_length = value.trim().parse().expect("content length should parse");
            }
        }

        let mut request_body = vec![0_u8; content_length];
        reader.read_exact(&mut request_body).expect("request body should read");
        let request: Value = serde_json::from_slice(&request_body).expect("request body should be JSON");
        let response = if let Some(response_body) = response_for_method(request.get("method").and_then(Value::as_str)) {
            let response_body = response_body.to_string();

            format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{}",
                response_body.len(),
                response_body
            )
        } else {
            "HTTP/1.1 202 Accepted\r\ncontent-length: 0\r\n\r\n".to_string()
        };

        stream.write_all(response.as_bytes()).expect("response should write");
    }

    fn response_for_method(method: Option<&str>) -> Option<Value> {
        match method {
            Some("notifications/initialized") => None,
            Some("tools/list") => Some(serde_json::json!({
                "jsonrpc": "2.0",
                "id": 2,
                "result": {
                    "tools": [{
                        "name": "update-user-name",
                        "description": "Update user name",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "user_name": { "type": "string" }
                            },
                            "required": ["user_name"]
                        }
                    }]
                }
            })),
            _ => Some(serde_json::json!({ "jsonrpc": "2.0", "id": 1, "result": {} })),
        }
    }
}
