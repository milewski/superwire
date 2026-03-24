use std::collections::HashMap;

use engine_ai_dsl::{builtin_symbols, lookup_symbol, SymbolCategory, SymbolDoc};
use serde_json::{json, Value};
use thiserror::Error;
use tokio::io::{stdin, stdout, AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader, BufWriter, Stdin, Stdout};

use crate::document::DocumentIndex;
use crate::protocol::{
    error_response, success_response, DidChangeTextDocumentParams, DidOpenTextDocumentParams, JsonRpcRequest, TextDocumentPositionParams,
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
    should_exit: bool,
}

#[derive(Debug, Default)]
pub struct LanguageServer {
    documents: HashMap<String, DocumentIndex>,
    shutdown_requested: bool,
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

            if outcome.should_exit {
                break;
            }
        }

        Ok(())
    }

    fn handle_request(&mut self, request: JsonRpcRequest) -> Result<RequestOutcome, ServerError> {
        log::debug!("handling LSP method {}", request.method);

        let outcome = match request.method.as_str() {
            "initialize" => RequestOutcome {
                response: request.id.map(|request_id| success_response(request_id, initialize_result())),
                should_exit: false,
            },
            "initialized" => RequestOutcome {
                response: None,
                should_exit: false,
            },
            "shutdown" => {
                self.shutdown_requested = true;

                RequestOutcome {
                    response: request.id.map(|request_id| success_response(request_id, Value::Null)),
                    should_exit: false,
                }
            }
            "exit" => RequestOutcome {
                response: None,
                should_exit: true,
            },
            "textDocument/didOpen" => {
                let open_params: DidOpenTextDocumentParams = serde_json::from_value(request.params)?;

                self.documents
                    .insert(open_params.text_document.uri, DocumentIndex::new(open_params.text_document.text));

                RequestOutcome {
                    response: None,
                    should_exit: false,
                }
            }
            "textDocument/didChange" => {
                let change_params: DidChangeTextDocumentParams = serde_json::from_value(request.params)?;

                if let Some(last_change) = change_params.content_changes.last() {
                    self.documents
                        .insert(change_params.text_document.uri, DocumentIndex::new(last_change.text.clone()));
                }

                RequestOutcome {
                    response: None,
                    should_exit: false,
                }
            }
            "textDocument/completion" => {
                let request_id = request.id.unwrap_or(Value::Null);
                let completion_params: TextDocumentPositionParams = serde_json::from_value(request.params)?;
                let completion_result = self.completion_result(&completion_params);

                RequestOutcome {
                    response: Some(success_response(request_id, completion_result)),
                    should_exit: false,
                }
            }
            "textDocument/hover" => {
                let request_id = request.id.unwrap_or(Value::Null);
                let hover_params: TextDocumentPositionParams = serde_json::from_value(request.params)?;

                let response = match self.hover_result(&hover_params) {
                    Some(hover_result) => success_response(request_id, hover_result),
                    None => success_response(request_id, Value::Null),
                };

                RequestOutcome {
                    response: Some(response),
                    should_exit: false,
                }
            }
            _ => RequestOutcome {
                response: request.id.map(|request_id| error_response(request_id, -32601, "Method not found")),
                should_exit: false,
            },
        };

        Ok(outcome)
    }

    fn completion_result(&self, completion_params: &TextDocumentPositionParams) -> Value {
        let Some(document_index) = self.documents.get(&completion_params.text_document.uri) else {
            return json!({
                "isIncomplete": false,
                "items": [],
            });
        };

        let line_prefix = document_index.line_prefix(completion_params.position).unwrap_or_default();

        let completion_items = if line_prefix.ends_with("schema.") {
            named_completion_items(document_index.schema_names(), 7, "Named schema")
        } else if line_prefix.ends_with("agent.") {
            named_completion_items(document_index.agent_names(), 6, "Declared agent")
        } else if line_prefix.ends_with("input.") {
            named_completion_items(document_index.input_fields(), 5, "Input field")
        } else if line_prefix.ends_with("secrets.") {
            named_completion_items(document_index.secret_fields(), 5, "Secret field")
        } else if looks_like_function_context(&line_prefix) {
            builtin_symbols()
                .iter()
                .filter(|symbol| matches!(symbol.category, SymbolCategory::Function))
                .map(symbol_completion_item)
                .collect()
        } else {
            builtin_symbols().iter().map(symbol_completion_item).collect()
        };

        json!({
            "isIncomplete": false,
            "items": completion_items,
        })
    }

    fn hover_result(&self, hover_params: &TextDocumentPositionParams) -> Option<Value> {
        let document_index = self.documents.get(&hover_params.text_document.uri)?;
        let hovered_symbol = document_index.symbol_at(hover_params.position)?;

        if let Some(hover_markdown) = dynamic_symbol_hover(document_index, &hovered_symbol) {
            return Some(markdown_hover(&hover_markdown));
        }

        let exact_lookup = lookup_symbol(&hovered_symbol).or_else(|| hovered_symbol.rsplit('.').next().and_then(lookup_symbol));

        exact_lookup.map(|symbol_doc| markdown_hover(&format_symbol_doc(symbol_doc)))
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
            "textDocumentSync": 1,
            "hoverProvider": true,
            "completionProvider": {
                "resolveProvider": false,
                "triggerCharacters": [".", ":"]
            }
        },
        "serverInfo": {
            "name": "engine-ai-lsp",
            "version": "0.1.0"
        }
    })
}

fn symbol_completion_item(symbol_doc: &SymbolDoc) -> Value {
    json!({
        "label": symbol_doc.label,
        "kind": completion_kind(symbol_doc.category),
        "detail": symbol_doc.detail,
        "documentation": symbol_doc.documentation,
        "insertText": symbol_doc.label,
    })
}

fn named_completion_items(entries: Vec<String>, kind: u32, detail: &str) -> Vec<Value> {
    entries
        .into_iter()
        .map(|entry| {
            json!({
                "label": entry,
                "kind": kind,
                "detail": detail,
                "insertText": entry,
            })
        })
        .collect()
}

fn completion_kind(symbol_category: SymbolCategory) -> u32 {
    match symbol_category {
        SymbolCategory::Keyword => 14,
        SymbolCategory::Function => 3,
        SymbolCategory::Namespace => 9,
        SymbolCategory::Property => 10,
        SymbolCategory::Type => 13,
    }
}

fn format_symbol_doc(symbol_doc: &SymbolDoc) -> String {
    format!("**{}**\n\n{}\n\n{}", symbol_doc.label, symbol_doc.detail, symbol_doc.documentation)
}

fn markdown_hover(markdown: &str) -> Value {
    json!({
        "contents": {
            "kind": "markdown",
            "value": markdown,
        }
    })
}

fn dynamic_symbol_hover(document_index: &DocumentIndex, symbol: &str) -> Option<String> {
    if let Some(schema_name) = symbol.strip_prefix("schema.") {
        if document_index
            .schema_names()
            .iter()
            .any(|candidate_name| candidate_name == schema_name)
        {
            return Some(format!("**schema.{schema_name}**\n\nNamed schema declared in this document."));
        }
    }

    if let Some(agent_reference) = symbol.strip_prefix("agent.") {
        let agent_name = agent_reference.split('.').next()?;

        if document_index
            .agent_names()
            .iter()
            .any(|candidate_name| candidate_name == agent_name)
        {
            return Some(format!("**agent.{agent_name}**\n\nReference to the `{agent_name}` agent output."));
        }
    }

    if let Some(input_field) = symbol.strip_prefix("input.") {
        let field_name = input_field.split('.').next()?;

        if document_index
            .input_fields()
            .iter()
            .any(|candidate_name| candidate_name == field_name)
        {
            return Some(format!("**input.{field_name}**\n\nWorkflow input field declared in this document."));
        }
    }

    if document_index
        .provider_names()
        .iter()
        .any(|candidate_name| candidate_name == symbol)
    {
        return Some(format!(
            "**{symbol}(...)**\n\nProvider model call bound to the `{symbol}` provider declared in this document."
        ));
    }

    None
}

fn looks_like_function_context(line_prefix: &str) -> bool {
    let trimmed_prefix = line_prefix.trim_end();

    trimmed_prefix.ends_with(':') || trimmed_prefix.ends_with('(') || trimmed_prefix.ends_with(',')
}
