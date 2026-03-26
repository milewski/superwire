use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Debug, Deserialize)]
pub struct JsonRpcRequest {
    #[allow(dead_code)]
    pub jsonrpc: String,
    #[serde(default)]
    pub id: Option<Value>,
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TextDocumentIdentifier {
    pub uri: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct VersionedTextDocumentIdentifier {
    pub uri: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TextDocumentItem {
    pub uri: String,
    pub text: String,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
pub struct Position {
    pub line: u32,
    pub character: u32,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
pub struct Range {
    pub start: Position,
    pub end: Position,
}

#[derive(Debug, Deserialize)]
pub struct TextDocumentPositionParams {
    #[serde(rename = "textDocument")]
    pub text_document: TextDocumentIdentifier,
    pub position: Position,
}

#[derive(Debug, Deserialize)]
pub struct DidOpenTextDocumentParams {
    #[serde(rename = "textDocument")]
    pub text_document: TextDocumentItem,
}

#[derive(Debug, Deserialize)]
pub struct TextDocumentContentChangeEvent {
    pub text: String,
}

#[derive(Debug, Deserialize)]
pub struct DidChangeTextDocumentParams {
    #[serde(rename = "textDocument")]
    pub text_document: VersionedTextDocumentIdentifier,
    #[serde(rename = "contentChanges")]
    pub content_changes: Vec<TextDocumentContentChangeEvent>,
}

#[derive(Debug, Deserialize)]
pub struct DidCloseTextDocumentParams {
    #[serde(rename = "textDocument")]
    pub text_document: TextDocumentIdentifier,
}

#[derive(Debug, Clone, Serialize)]
pub struct Diagnostic {
    pub range: Range,
    pub severity: u32,
    pub code: DiagnosticCode,
    pub source: String,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum DiagnosticCode {
    #[serde(rename = "parse_error")]
    ParseError,
    #[serde(rename = "missing_node")]
    MissingNode,
    #[serde(rename = "unexpected_rule")]
    UnexpectedRule,
    #[serde(rename = "invalid_integer_literal")]
    InvalidIntegerLiteral,
    #[serde(rename = "duplicate_provider")]
    DuplicateProvider,
    #[serde(rename = "duplicate_schema")]
    DuplicateSchema,
    #[serde(rename = "duplicate_agent")]
    DuplicateAgent,
    #[serde(rename = "duplicate_singleton_declaration")]
    DuplicateSingletonDeclaration,
    #[serde(rename = "unknown_agent_property")]
    UnknownAgentProperty,
    #[serde(rename = "invalid_inference_setting_value_type")]
    InvalidInferenceSettingValueType,
    #[serde(rename = "invalid_model_expression")]
    InvalidModelExpression,
    #[serde(rename = "unknown_provider_in_model")]
    UnknownProviderInModel,
    #[serde(rename = "unknown_model_for_provider")]
    UnknownModelForProvider,
    #[serde(rename = "unknown_agent_reference")]
    UnknownAgentReference,
    #[serde(rename = "invalid_keyword_reference_root")]
    InvalidKeywordReferenceRoot,
    #[serde(rename = "missing_input_declaration")]
    MissingInputDeclaration,
    #[serde(rename = "missing_secrets_declaration")]
    MissingSecretsDeclaration,
    #[serde(rename = "unknown_input_field_reference")]
    UnknownInputFieldReference,
    #[serde(rename = "unknown_secrets_field_reference")]
    UnknownSecretsFieldReference,
    #[serde(rename = "secret_reference_in_llm_context")]
    SecretReferenceInLlmContext,
    #[serde(rename = "missing_agent_output_type_for_field_reference")]
    MissingAgentOutputTypeForFieldReference,
    #[serde(rename = "invalid_reference_path")]
    InvalidReferencePath,
    #[serde(rename = "unknown_schema_reference")]
    UnknownSchemaReference,
    #[serde(rename = "agent_dependency_cycle")]
    AgentDependencyCycle,
    #[serde(rename = "workflow_compilation_error")]
    WorkflowCompilationError,
}

#[must_use]
pub fn success_response(id: Value, result: Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": result,
    })
}

#[must_use]
pub fn error_response(id: Value, code: i64, message: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {
            "code": code,
            "message": message,
        },
    })
}

#[must_use]
pub fn notification(method: &str, params: Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "method": method,
        "params": params,
    })
}

#[must_use]
pub fn publish_diagnostics_notification(uri: &str, diagnostics: Vec<Diagnostic>) -> Value {
    notification(
        "textDocument/publishDiagnostics",
        json!({
            "uri": uri,
            "diagnostics": diagnostics,
        }),
    )
}
