use std::collections::HashMap;

use serde_json::{json, Value};
use thiserror::Error;
use tokio::io::{stdin, stdout, AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader, BufWriter, Stdin, Stdout};

use crate::document::{CompletionSuggestion, DocumentState};
use crate::protocol::{
    error_response, publish_diagnostics_notification, success_response, Diagnostic, DidChangeTextDocumentParams,
    DidCloseTextDocumentParams, DidOpenTextDocumentParams, JsonRpcRequest, TextDocumentPositionParams,
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
            "completionProvider": {
                "resolveProvider": false,
                "triggerCharacters": [".", ":", "\""]
            }
        },
        "serverInfo": {
            "name": "engine-ai-lsp",
            "version": "0.2.0"
        }
    })
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
