use std::collections::HashMap;
use std::fmt::Write;

use serde_json::{json, Value};
use thiserror::Error;
use tokio::io::{stdin, stdout, AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader, BufWriter, Stdin, Stdout};

use engine_ai_core::dsl::{AgentProperty, Expression};

use crate::document::{parse_reference_chain, DiagnosticSeverity, LspDiagnostic, ParsedDocument};
use crate::protocol::{
    error_response, publish_diagnostics_notification, success_response, DidChangeTextDocumentParams, DidOpenTextDocumentParams,
    JsonRpcRequest, TextDocumentPositionParams,
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
    documents: HashMap<String, ParsedDocument>,
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

            for notification in &outcome.notifications {
                message_writer.write_message(notification).await?;
            }

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
                notifications: Vec::new(),
                should_exit: false,
            },
            "initialized" => RequestOutcome {
                response: None,
                notifications: Vec::new(),
                should_exit: false,
            },
            "shutdown" => {
                self.shutdown_requested = true;

                RequestOutcome {
                    response: request.id.map(|request_id| success_response(request_id, Value::Null)),
                    notifications: Vec::new(),
                    should_exit: false,
                }
            }
            "exit" => RequestOutcome {
                response: None,
                notifications: Vec::new(),
                should_exit: true,
            },
            "textDocument/didOpen" => {
                let open_params: DidOpenTextDocumentParams = serde_json::from_value(request.params)?;
                let document = ParsedDocument::parse(open_params.text_document.text);
                let diagnostics = convert_diagnostics_to_lsp(&document.diagnostics());
                let notification = publish_diagnostics_notification(&open_params.text_document.uri, &diagnostics);

                self.documents.insert(open_params.text_document.uri, document);

                RequestOutcome {
                    response: None,
                    notifications: vec![notification],
                    should_exit: false,
                }
            }
            "textDocument/didChange" => {
                let change_params: DidChangeTextDocumentParams = serde_json::from_value(request.params)?;

                let mut notifications = Vec::new();

                if let Some(last_change) = change_params.content_changes.last() {
                    let document = ParsedDocument::parse(last_change.text.clone());
                    let diagnostics = convert_diagnostics_to_lsp(&document.diagnostics());
                    let notification = publish_diagnostics_notification(&change_params.text_document.uri, &diagnostics);

                    notifications.push(notification);
                    self.documents.insert(change_params.text_document.uri, document);
                }

                RequestOutcome {
                    response: None,
                    notifications,
                    should_exit: false,
                }
            }
            "textDocument/completion" => {
                let request_id = request.id.unwrap_or(Value::Null);
                let completion_params: TextDocumentPositionParams = serde_json::from_value(request.params)?;
                let completion_result = self.completion_result(&completion_params);

                RequestOutcome {
                    response: Some(success_response(request_id, completion_result)),
                    notifications: Vec::new(),
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
                    notifications: Vec::new(),
                    should_exit: false,
                }
            }
            _ => RequestOutcome {
                response: request.id.map(|request_id| error_response(request_id, -32601, "Method not found")),
                notifications: Vec::new(),
                should_exit: false,
            },
        };

        Ok(outcome)
    }

    fn completion_result(&self, completion_params: &TextDocumentPositionParams) -> Value {
        let Some(parsed_document) = self.documents.get(&completion_params.text_document.uri) else {
            return json!({
                "isIncomplete": false,
                "items": [],
            });
        };

        let line_prefix = parsed_document.line_prefix(completion_params.position).unwrap_or_default();

        let completion_items = if line_prefix.trim_end().ends_with('.') {
            self.complete_after_dot(parsed_document, &line_prefix)
        } else if line_prefix.trim_end().ends_with(':') {
            self.complete_after_colon(&line_prefix)
        } else if line_prefix.trim_end().ends_with('(') {
            self.complete_after_open_paren(parsed_document, &line_prefix)
        } else {
            self.complete_default(parsed_document, &line_prefix)
        };

        json!({
            "isIncomplete": false,
            "items": completion_items,
        })
    }

    fn complete_after_dot(&self, parsed_document: &ParsedDocument, line_prefix: &str) -> Vec<Value> {
        let reference_chain = parse_reference_chain(line_prefix);

        if reference_chain.is_empty() {
            return Vec::new();
        }

        let completions = parsed_document.resolve_reference_chain_fields(&reference_chain);

        completions
            .into_iter()
            .map(|field_name| {
                json!({
                    "label": field_name,
                    "kind": 5,
                    "detail": "Field",
                    "insertText": field_name,
                })
            })
            .collect()
    }

    fn complete_after_colon(&self, line_prefix: &str) -> Vec<Value> {
        if is_inside_agent_block(line_prefix) {
            return agent_property_completions();
        }

        scalar_type_completions()
    }

    fn complete_after_open_paren(&self, parsed_document: &ParsedDocument, line_prefix: &str) -> Vec<Value> {
        let trimmed = line_prefix.trim_end();
        let paren_position = trimmed.rfind('(');

        let Some(paren_position) = paren_position else {
            return expression_completions();
        };

        let before_paren = trimmed[..paren_position].trim_end();

        if let Some(last_dot) = before_paren.rfind('.') {
            let provider_name = before_paren[last_dot + 1..].trim();

            if !provider_name.is_empty() && parsed_document.provider_names().contains(&provider_name.to_owned()) {
                return model_name_completions(parsed_document, provider_name);
            }
        } else if !before_paren.is_empty()
            && before_paren
                .chars()
                .last()
                .is_some_and(|character| character.is_ascii_alphanumeric() || character == '_')
        {
            let provider_name = before_paren;

            if parsed_document.provider_names().contains(&provider_name.to_owned()) {
                return model_name_completions(parsed_document, provider_name);
            }
        }

        expression_completions()
    }

    fn complete_default(&self, _parsed_document: &ParsedDocument, line_prefix: &str) -> Vec<Value> {
        let trimmed = line_prefix.trim();

        if trimmed.is_empty() || is_on_fresh_line(line_prefix) {
            if is_inside_agent_block(line_prefix) {
                return agent_property_completions();
            }

            if is_inside_typed_block(line_prefix) {
                return field_name_completions();
            }

            return keyword_completions();
        }

        expression_completions()
    }

    fn hover_result(&self, hover_params: &TextDocumentPositionParams) -> Option<Value> {
        let parsed_document = self.documents.get(&hover_params.text_document.uri)?;
        let hovered_symbol = symbol_at_position(parsed_document.source(), hover_params.position)?;

        if let Some(hover_markdown) = ast_symbol_hover(parsed_document, &hovered_symbol) {
            return Some(markdown_hover(&hover_markdown));
        }

        builtin_symbol_hover(&hovered_symbol).map(|hover_markdown| markdown_hover(&hover_markdown))
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
                "triggerCharacters": [".", ":", "("]
            },
            "diagnosticProvider": {
                "interFileDependencies": false,
                "workspaceDiagnostics": false
            }
        },
        "serverInfo": {
            "name": "engine-ai-lsp",
            "version": "0.1.0"
        }
    })
}

fn convert_diagnostics_to_lsp(diagnostics: &[LspDiagnostic]) -> Vec<Value> {
    diagnostics
        .iter()
        .map(|diagnostic| {
            json!({
                "range": {
                    "start": {
                        "line": diagnostic.range.start.line,
                        "character": diagnostic.range.start.character,
                    },
                    "end": {
                        "line": diagnostic.range.end.line,
                        "character": diagnostic.range.end.character,
                    },
                },
                "severity": match diagnostic.severity {
                    DiagnosticSeverity::Error => 1,
                    DiagnosticSeverity::Warning => 2,
                    DiagnosticSeverity::Information => 3,
                    DiagnosticSeverity::Hint => 4,
                },
                "message": diagnostic.message,
            })
        })
        .collect()
}

fn keyword_completions() -> Vec<Value> {
    let keywords = vec![
        ("provider", "Provider declaration", "provider ${1:name} {\n\t${2:properties}\n}"),
        ("secrets", "Secrets declaration", "secrets {\n\t${1:fields}\n}"),
        ("input", "Input declaration", "input {\n\t${1:fields}\n}"),
        ("schema", "Schema declaration", "schema ${1:Name} {\n\t${2:fields}\n}"),
        ("agent", "Agent declaration", "agent ${1:name} {\n\t${2:properties}\n}"),
        ("output", "Output declaration", "output {\n\t${1:fields}\n}"),
    ];

    keywords
        .into_iter()
        .map(|(label, detail, insert_text)| {
            json!({
                "label": label,
                "kind": 14,
                "detail": detail,
                "insertText": insert_text,
                "insertTextFormat": 2,
            })
        })
        .collect()
}

fn agent_property_completions() -> Vec<Value> {
    let properties = vec![
        ("model:", "Model binding", "model: ${1:provider(\"model\")}"),
        ("prompt:", "Prompt expression", "prompt: \"${1:text}\""),
        ("output:", "Output type", "output: ${1:string}"),
        ("context:", "Context expression", "context: ${1:expression}"),
        ("inference:", "Inference configuration", "inference: ${1:expression}"),
        ("tools:", "Tools list", "tools: [${1:tool.name}]"),
    ];

    properties
        .into_iter()
        .map(|(label, detail, insert_text)| {
            json!({
                "label": label,
                "kind": 10,
                "detail": detail,
                "insertText": insert_text,
                "insertTextFormat": 2,
            })
        })
        .collect()
}

fn expression_completions() -> Vec<Value> {
    let mut items = Vec::new();

    let keywords = vec![("true", "Boolean literal"), ("false", "Boolean literal"), ("null", "Null literal")];

    for (label, detail) in keywords {
        items.push(json!({
            "label": label,
            "kind": 14,
            "detail": detail,
            "insertText": label,
        }));
    }

    items.push(json!({
        "label": "\"\"",
        "kind": 6,
        "detail": "String literal",
        "insertText": "\"${1:text}\"",
        "insertTextFormat": 2,
    }));

    items.push(json!({
        "label": "\"\"\"\"\"\"",
        "kind": 6,
        "detail": "Multiline string",
        "insertText": "\"\"\"\n${1:text}\n\"\"\"",
        "insertTextFormat": 2,
    }));

    items.push(json!({
        "label": "[]",
        "kind": 6,
        "detail": "Array literal",
        "insertText": "[${1:}]",
        "insertTextFormat": 2,
    }));

    items.push(json!({
        "label": "{}",
        "kind": 6,
        "detail": "Object literal",
        "insertText": "{\n\t${1:}\n}",
        "insertTextFormat": 2,
    }));

    let reference_keywords = vec![
        ("agent.", "Agent reference"),
        ("input.", "Input reference"),
        ("secrets.", "Secrets reference"),
        ("tool.", "Tool reference"),
    ];

    for (label, detail) in reference_keywords {
        items.push(json!({
            "label": label,
            "kind": 6,
            "detail": detail,
            "insertText": label,
        }));
    }

    items
}

fn scalar_type_completions() -> Vec<Value> {
    let types = vec![
        ("string", "String type"),
        ("number", "Number type"),
        ("float", "Float type"),
        ("boolean", "Boolean type"),
        ("null", "Null type"),
    ];

    let mut items: Vec<Value> = types
        .into_iter()
        .map(|(label, detail)| {
            json!({
                "label": label,
                "kind": 13,
                "detail": detail,
                "insertText": label,
            })
        })
        .collect();

    items.push(json!({
        "label": "schema.",
        "kind": 7,
        "detail": "Schema reference",
        "insertText": "schema.",
    }));

    items
}

fn field_name_completions() -> Vec<Value> {
    vec![json!({
        "label": "identifier",
        "kind": 6,
        "detail": "Field name",
        "insertText": "${1:name}: ${2:type}",
        "insertTextFormat": 2,
    })]
}

fn model_name_completions(parsed_document: &ParsedDocument, provider_name: &str) -> Vec<Value> {
    let Some(workflow) = parsed_document.workflow() else {
        return Vec::new();
    };

    let Some(provider_declaration) = workflow.find_provider(provider_name) else {
        return Vec::new();
    };

    let models_property = provider_declaration.properties.iter().find(|field| field.name == "models");

    let Some(models_property) = models_property else {
        return Vec::new();
    };

    let Expression::ArrayLiteral(model_entries) = &models_property.value else {
        return Vec::new();
    };

    model_entries
        .iter()
        .filter_map(|model_entry| {
            if let Expression::StringLiteral(model_name) = model_entry {
                Some(json!({
                    "label": model_name,
                    "kind": 6,
                    "detail": format!("Model for {provider_name}"),
                    "insertText": format!("\"{model_name}\""),
                }))
            } else {
                None
            }
        })
        .collect()
}

fn symbol_at_position(source: &str, position: crate::protocol::Position) -> Option<String> {
    let line_text = source.lines().nth(position.line as usize)?;
    let line_characters: Vec<char> = line_text.chars().collect();

    if line_characters.is_empty() {
        return None;
    }

    let mut cursor_index = usize::min(position.character as usize, line_characters.len().saturating_sub(1));

    if !is_symbol_character(line_characters[cursor_index]) {
        if cursor_index == 0 || !is_symbol_character(line_characters[cursor_index - 1]) {
            return None;
        }

        cursor_index -= 1;
    }

    let mut start_index = cursor_index;

    while start_index > 0 && is_symbol_character(line_characters[start_index - 1]) {
        start_index -= 1;
    }

    let mut end_index = cursor_index + 1;

    while end_index < line_characters.len() && is_symbol_character(line_characters[end_index]) {
        end_index += 1;
    }

    Some(line_characters[start_index..end_index].iter().collect())
}

fn is_symbol_character(character: char) -> bool {
    character.is_ascii_alphanumeric() || character == '_' || character == '.'
}

fn ast_symbol_hover(parsed_document: &ParsedDocument, symbol: &str) -> Option<String> {
    if symbol.contains('.') {
        let chain: Vec<String> = symbol.split('.').map(str::to_owned).collect();

        if chain.len() >= 2 {
            let mut type_cursor = None;

            if chain[0] == "agent" {
                let agent_name = &chain[1];
                let fields_after_name = &chain[2..];

                let agent_declaration = parsed_document.workflow()?.find_agent(agent_name)?;

                type_cursor = agent_declaration.properties.iter().find_map(|property| {
                    if let AgentProperty::Output(type_expression) = property {
                        Some(type_expression.clone())
                    } else {
                        None
                    }
                });

                for field_name in fields_after_name {
                    let current_type = type_cursor?;
                    type_cursor = parsed_document.find_field_type(&current_type, field_name);
                }
            } else if chain[0] == "input" {
                let fields_after_root = &chain[1..];

                let input_declaration = parsed_document.workflow()?.find_input()?;
                let mut current_type = engine_ai_core::dsl::TypeExpression::Object(input_declaration.fields.clone());

                for field_name in fields_after_root {
                    current_type = parsed_document.find_field_type(&current_type, field_name)?;
                }

                type_cursor = Some(current_type);
            } else if chain[0] == "secrets" {
                let fields_after_root = &chain[1..];

                let secrets_declaration = parsed_document.workflow()?.find_secrets()?;
                let mut current_type = engine_ai_core::dsl::TypeExpression::Object(secrets_declaration.fields.clone());

                for field_name in fields_after_root {
                    current_type = parsed_document.find_field_type(&current_type, field_name)?;
                }

                type_cursor = Some(current_type);
            }

            if let Some(resolved_type) = type_cursor {
                return Some(format!("**{symbol}**\n\nType: `{}`", format_type_expression(&resolved_type)));
            }
        }
    }

    if let Some(schema_name) = symbol.strip_prefix("schema.") {
        let schema_declaration = parsed_document.workflow()?.find_schema(schema_name)?;

        let field_descriptions: Vec<String> = schema_declaration
            .fields
            .iter()
            .map(|field| format!("- `{}`: {}", field.name, format_type_expression(&field.field_type)))
            .collect();

        return Some(format!(
            "**schema.{schema_name}**\n\nSchema declaration.\n\n{}",
            field_descriptions.join("\n")
        ));
    }

    if let Some(agent_declaration) = parsed_document.workflow()?.find_agent(symbol) {
        let mut description = format!("**{symbol}**\n\nAgent declaration");

        if let Some(for_loop) = &agent_declaration.for_loop {
            let _ = write!(description, "\n\nLoop: `for {} in ...`", for_loop.iterator_name);
        }

        let property_descriptions: Vec<String> = agent_declaration
            .properties
            .iter()
            .map(|property| match property {
                AgentProperty::Model(_) => "- `model:` Model binding".to_owned(),
                AgentProperty::Prompt(_) => "- `prompt:` Prompt configuration".to_owned(),
                AgentProperty::Output(type_expr) => {
                    format!("- `output:` {}", format_type_expression(type_expr))
                }
                AgentProperty::Context(_) => "- `context:` Context configuration".to_owned(),
                AgentProperty::Inference(_) => "- `inference:` Inference configuration".to_owned(),
                AgentProperty::Tools(_) => "- `tools:` Tools configuration".to_owned(),
                AgentProperty::Custom { name, .. } => format!("- `{name}:` Custom property"),
            })
            .collect();

        let _ = write!(description, "\n\n{}", property_descriptions.join("\n"));

        return Some(description);
    }

    if parsed_document.provider_names().contains(&symbol.to_owned()) {
        return Some(format!("**{symbol}(...)**\n\nProvider configured in this document."));
    }

    None
}

fn format_type_expression(type_expression: &engine_ai_core::dsl::TypeExpression) -> String {
    match type_expression {
        engine_ai_core::dsl::TypeExpression::String => "string".to_owned(),
        engine_ai_core::dsl::TypeExpression::Number => "number".to_owned(),
        engine_ai_core::dsl::TypeExpression::Float => "float".to_owned(),
        engine_ai_core::dsl::TypeExpression::Boolean => "boolean".to_owned(),
        engine_ai_core::dsl::TypeExpression::Null => "null".to_owned(),
        engine_ai_core::dsl::TypeExpression::SchemaReference(name) => format!("schema.{name}"),
        engine_ai_core::dsl::TypeExpression::StringEnum(value) => format!("\"{value}\""),
        engine_ai_core::dsl::TypeExpression::Array { item_type, fixed_length } => {
            let item_description = format_type_expression(item_type);

            match fixed_length {
                Some(length) => format!("[{item_description}; {length}]"),
                None => format!("[{item_description}]"),
            }
        }
        engine_ai_core::dsl::TypeExpression::Tuple(types) => {
            let type_descriptions: Vec<String> = types.iter().map(format_type_expression).collect();

            format!("({})", type_descriptions.join(", "))
        }
        engine_ai_core::dsl::TypeExpression::Object(fields) => {
            let field_descriptions: Vec<String> = fields
                .iter()
                .map(|field| format!("{}: {}", field.name, format_type_expression(&field.field_type)))
                .collect();

            format!("{{ {} }}", field_descriptions.join("; "))
        }
        engine_ai_core::dsl::TypeExpression::Union(types) => {
            let type_descriptions: Vec<String> = types.iter().map(format_type_expression).collect();

            type_descriptions.join(" | ")
        }
    }
}

fn builtin_symbol_hover(symbol: &str) -> Option<String> {
    let builtin_documentation: &[(&str, &str)] = &[
        ("provider", "**provider**\n\nDeclares an LLM provider configuration."),
        ("secrets", "**secrets**\n\nDeclares secret values for the workflow."),
        ("input", "**input**\n\nDeclares input fields for the workflow."),
        ("schema", "**schema**\n\nDeclares a named data schema."),
        (
            "agent",
            "**agent**\n\nDeclares an AI agent with model, prompt, and output configuration.",
        ),
        ("output", "**output**\n\nDeclares the workflow output mapping."),
        ("model", "**model**\n\nAgent property that binds the agent to a provider model."),
        ("prompt", "**prompt**\n\nAgent property that defines the prompt sent to the model."),
        (
            "context",
            "**context**\n\nAgent property that provides additional context to the agent.",
        ),
        ("inference", "**inference**\n\nAgent property that configures inference parameters."),
        ("tools", "**tools**\n\nAgent property that provides tool references for the agent."),
        ("string", "**string**\n\nScalar type for text values."),
        ("number", "**number**\n\nScalar type for integer values."),
        ("float", "**float**\n\nScalar type for floating-point values."),
        ("boolean", "**boolean**\n\nScalar type for true/false values."),
        ("null", "**null**\n\nScalar type representing absence of value."),
        ("true", "**true**\n\nBoolean literal."),
        ("false", "**false**\n\nBoolean literal."),
        (
            "for",
            "**for**\n\nIterates over an expression, creating agent instances for each element.",
        ),
        ("in", "**in**\n\nUsed with `for` to specify the iterable expression."),
    ];

    builtin_documentation
        .iter()
        .find(|(label, _)| *label == symbol)
        .map(|(_, documentation)| (*documentation).to_owned())
}

fn markdown_hover(markdown: &str) -> Value {
    json!({
        "contents": {
            "kind": "markdown",
            "value": markdown,
        }
    })
}

#[allow(clippy::cast_possible_wrap)]
fn is_inside_agent_block(line_prefix: &str) -> bool {
    let trimmed_prefix = line_prefix.trim();

    if trimmed_prefix.ends_with('{') {
        return false;
    }

    let mut brace_depth: isize = 0;

    for line in line_prefix.lines().rev() {
        let trimmed_line = line.trim();

        let opening_braces = trimmed_line.chars().filter(|character| *character == '{').count() as isize;
        let closing_braces = trimmed_line.chars().filter(|character| *character == '}').count() as isize;

        brace_depth += opening_braces;
        brace_depth -= closing_braces;

        if brace_depth >= 1 && trimmed_line.starts_with("agent ") {
            return true;
        }

        if brace_depth <= 0 && !trimmed_line.is_empty() {
            break;
        }
    }

    false
}

#[allow(clippy::cast_possible_wrap)]
fn is_inside_typed_block(line_prefix: &str) -> bool {
    let mut brace_depth: isize = 0;

    for line in line_prefix.lines().rev() {
        let trimmed_line = line.trim();

        let opening_braces = trimmed_line.chars().filter(|character| *character == '{').count() as isize;
        let closing_braces = trimmed_line.chars().filter(|character| *character == '}').count() as isize;

        brace_depth += opening_braces;
        brace_depth -= closing_braces;

        if brace_depth >= 1 {
            let starts_typed_block = trimmed_line.starts_with("input ")
                || trimmed_line.starts_with("input{")
                || trimmed_line.starts_with("secrets ")
                || trimmed_line.starts_with("secrets{")
                || trimmed_line.starts_with("schema ");

            if starts_typed_block {
                return true;
            }
        }

        if brace_depth <= 0 && !trimmed_line.is_empty() {
            break;
        }
    }

    false
}

fn is_on_fresh_line(line_prefix: &str) -> bool {
    line_prefix.trim().is_empty()
}
