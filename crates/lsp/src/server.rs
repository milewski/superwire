use std::collections::HashMap;

use serde_json::{json, Value};
use thiserror::Error;
use tokio::io::{stdin, stdout, AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader, BufWriter, Stdin, Stdout};

use crate::document::{CodeLensHint, CompletionSuggestion, DocumentState, DocumentSymbolNode, FoldingRangeBlock, WorkspaceSymbolMatch};
use crate::protocol::{
    error_response, publish_diagnostics_notification, success_response, CodeLens, CodeLensParams, Command, Diagnostic,
    DidChangeTextDocumentParams, DidCloseTextDocumentParams, DidOpenTextDocumentParams, DocumentFormattingParams,
    DocumentSymbol as ProtocolDocumentSymbol, DocumentSymbolParams, ExecuteCommandParams, FoldingRange, FoldingRangeParams, JsonRpcRequest,
    Location, SymbolInformation, TextDocumentPositionParams, TextEdit, WorkspaceSymbolParams,
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

#[derive(Debug, Default)]
pub struct LanguageServer {
    documents: HashMap<String, DocumentState>,
}

impl LanguageServer {
    pub async fn run_stdio() -> Result<(), ServerError> {
        let input_reader = BufReader::new(stdin());
        let output_writer = BufWriter::new(stdout());
        let mut message_reader = MessageReader::new(input_reader);
        let mut message_writer = MessageWriter::new(output_writer);
        let mut language_server = Self::default();

        while let Some(raw_message) = message_reader.read_message().await? {
            let request: JsonRpcRequest = serde_json::from_slice(&raw_message)?;
            let outcome = language_server.handle_request(request)?;

            if let Some(response) = outcome.response {
                message_writer.write_message(&response).await?;
            }

            for notification in outcome.notifications {
                message_writer.write_message(&notification).await?;
            }

            if outcome.should_exit {
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

        self.documents.insert(
            open_params.text_document.uri.clone(),
            DocumentState::new(open_params.text_document.text),
        );

        let diagnostics_notification = self.publish_document_diagnostics(open_params.text_document.uri.as_str());

        Ok(RequestOutcome::without_response(diagnostics_notification))
    }

    fn handle_did_change(&mut self, params: Value) -> Result<RequestOutcome, ServerError> {
        let change_params: DidChangeTextDocumentParams = serde_json::from_value(params)?;

        if let Some(last_change) = change_params.content_changes.last() {
            if let Some(document_state) = self.documents.get_mut(&change_params.text_document.uri) {
                document_state.replace_text(last_change.text.clone());
            } else {
                self.documents.insert(
                    change_params.text_document.uri.clone(),
                    DocumentState::new(last_change.text.clone()),
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
                source: "engine-ai-lsp".to_string(),
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

        let completion_items = document_state
            .completion_suggestions(completion_params.position)
            .into_iter()
            .map(completion_item_to_value)
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
            "completionProvider": {
                "resolveProvider": false,
                "triggerCharacters": [".", ":", "\""]
            },
            "codeLensProvider": {
                "resolveProvider": false
            },
            "executeCommandProvider": {
                "commands": ["engine-ai.generated.output"]
            }
        },
        "serverInfo": {
            "name": "engine-ai-lsp",
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

fn completion_item_to_value(completion_suggestion: CompletionSuggestion) -> Value {
    json!({
        "label": completion_suggestion.label,
        "kind": completion_suggestion.kind.as_lsp_kind(),
        "detail": completion_suggestion.detail,
        "documentation": completion_suggestion.documentation,
        "insertText": completion_suggestion.insert_text,
    })
}

fn markdown_hover(markdown: &str) -> Value {
    json!({
        "contents": {
            "kind": "markdown",
            "value": markdown,
        }
    })
}
