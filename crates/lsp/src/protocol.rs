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

#[derive(Debug, Deserialize)]
pub struct DocumentSymbolParams {
    #[serde(rename = "textDocument")]
    pub text_document: TextDocumentIdentifier,
}

#[derive(Debug, Deserialize)]
pub struct WorkspaceSymbolParams {
    pub query: String,
}

#[derive(Debug, Deserialize)]
pub struct DocumentFormattingParams {
    #[serde(rename = "textDocument")]
    pub text_document: TextDocumentIdentifier,
}

#[derive(Debug, Deserialize)]
pub struct FoldingRangeParams {
    #[serde(rename = "textDocument")]
    pub text_document: TextDocumentIdentifier,
}

#[derive(Debug, Deserialize)]
pub struct CodeLensParams {
    #[serde(rename = "textDocument")]
    pub text_document: TextDocumentIdentifier,
}

#[derive(Debug, Deserialize)]
pub struct ExecuteCommandParams {
    pub command: String,
    #[serde(default)]
    pub arguments: Vec<Value>,
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
    #[serde(rename = "duplicate_tool")]
    DuplicateTool,
    #[serde(rename = "duplicate_resource")]
    DuplicateResource,
    #[serde(rename = "duplicate_prompt")]
    DuplicatePrompt,
    #[serde(rename = "duplicate_agent")]
    DuplicateAgent,
    #[serde(rename = "duplicate_singleton_declaration")]
    DuplicateSingletonDeclaration,
    #[serde(rename = "duplicate_property")]
    DuplicateProperty,
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
    #[serde(rename = "missing_dynamic_declaration")]
    MissingDynamicDeclaration,
    #[serde(rename = "missing_input_declaration")]
    MissingInputDeclaration,
    #[serde(rename = "missing_secrets_declaration")]
    MissingSecretsDeclaration,
    #[serde(rename = "unknown_dynamic_field_reference")]
    UnknownDynamicFieldReference,
    #[serde(rename = "unknown_input_field_reference")]
    UnknownInputFieldReference,
    #[serde(rename = "unknown_secrets_field_reference")]
    UnknownSecretsFieldReference,
    #[serde(rename = "secret_reference_in_llm_context")]
    SecretReferenceInLlmContext,
    #[serde(rename = "missing_agent_output_type_for_field_reference")]
    MissingAgentOutputTypeForFieldReference,
    #[serde(rename = "missing_optional_reference_access")]
    MissingOptionalReferenceAccess,
    #[serde(rename = "invalid_reference_path")]
    InvalidReferencePath,
    #[serde(rename = "invalid_for_loop_iterable_type")]
    InvalidForLoopIterableType,
    #[serde(rename = "unknown_schema_reference")]
    UnknownSchemaReference,
    #[serde(rename = "unknown_tool_reference")]
    UnknownToolReference,
    #[serde(rename = "unknown_resource_reference")]
    UnknownResourceReference,
    #[serde(rename = "unknown_prompt_reference")]
    UnknownPromptReference,
    #[serde(rename = "invalid_tool_binding")]
    InvalidToolBinding,
    #[serde(rename = "invalid_type_expression_reference")]
    InvalidTypeExpressionReference,
    #[serde(rename = "agent_dependency_cycle")]
    AgentDependencyCycle,
    #[serde(rename = "dynamic_dependency_cycle")]
    DynamicDependencyCycle,
    #[serde(rename = "workflow_compilation_error")]
    WorkflowCompilationError,
}

#[derive(Debug, Clone, Serialize)]
pub struct Location {
    pub uri: String,
    pub range: Range,
}

#[derive(Debug, Clone, Serialize)]
pub struct TextEdit {
    pub range: Range,
    #[serde(rename = "newText")]
    pub new_text: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct FoldingRange {
    #[serde(rename = "startLine")]
    pub start_line: u32,
    #[serde(rename = "startCharacter")]
    pub start_character: Option<u32>,
    #[serde(rename = "endLine")]
    pub end_line: u32,
    #[serde(rename = "endCharacter")]
    pub end_character: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DocumentSymbol {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    pub kind: u32,
    pub range: Range,
    #[serde(rename = "selectionRange")]
    pub selection_range: Range,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<DocumentSymbol>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SymbolInformation {
    pub name: String,
    pub kind: u32,
    pub location: Location,
    #[serde(rename = "containerName", skip_serializing_if = "Option::is_none")]
    pub container_name: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Command {
    pub title: String,
    pub command: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub arguments: Vec<Value>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CodeLens {
    pub range: Range,
    pub command: Command,
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
