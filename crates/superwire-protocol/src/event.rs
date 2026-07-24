use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::error::Error;
use std::fmt;
use std::io;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use superwire_semantic::PlannedMcpImportKind as SemanticPlannedMcpImportKind;
use superwire_types::ast::McpCallOperation as DslMcpCallOperation;

pub const MAX_SERIALIZED_PUBLIC_EVENT_BYTES: usize = 240 * 1024;

const MAX_EVENT_IDENTIFIER_DECIMAL_BYTES: usize = 20;
const SSE_EVENT_FIELD_PREFIX_BYTES: usize = "event: ".len();
const SSE_IDENTIFIER_FIELD_PREFIX_BYTES: usize = "id: ".len();
const SSE_DATA_FIELD_PREFIX_BYTES: usize = "data: ".len();
const SSE_FIELD_LINE_FEED_BYTES: usize = 1;
const SSE_FINAL_LINE_FEED_BYTES: usize = 1;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExecutorDiagnosticCode {
    InvalidWorkflow,
    InvalidInput,
    InvalidSecrets,
    InvalidOutput,
    InvalidConfiguration,
    ModelProviderFailed,
    ModelRejected,
    ProviderRateLimited,
    ProviderRetriesExhausted,
    ToolFailed,
    McpFailed,
    CacheUnavailable,
    StreamGap,
    StreamExpired,
    StreamCapacityExceeded,
    UnknownRun,
    CancellationConflict,
    EventTooLarge,
    Cancelled,
    InternalPanic,
    InternalError,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExecutorStage {
    Planning,
    Input,
    Secrets,
    Agent,
    Model,
    Tool,
    Mcp,
    Cache,
    Output,
    Stream,
    Cancellation,
    Internal,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticSeverity {
    Warning,
    Error,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticRetryability {
    Never,
    Unknown,
    Safe,
    AfterDelay,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CacheOperation {
    Connect,
    Read,
    Write,
    Purge,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ExecutorDiagnosticSubject {
    Workflow,
    Agent {
        agent_name: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        iteration_index: Option<usize>,
    },
    Provider {
        agent_name: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        provider_name: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        model_name: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        attempt: Option<u32>,
        #[serde(skip_serializing_if = "Option::is_none")]
        http_status: Option<u16>,
    },
    Tool {
        #[serde(skip_serializing_if = "Option::is_none")]
        agent_name: Option<String>,
        tool_name: String,
    },
    Mcp {
        #[serde(skip_serializing_if = "Option::is_none")]
        agent_name: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        server_name: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        target_name: Option<String>,
    },
    Cache {
        operation: CacheOperation,
    },
    Stream {
        #[serde(skip_serializing_if = "Option::is_none")]
        requested_after: Option<u64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        oldest_available: Option<u64>,
    },
    Event {
        actual_bytes: usize,
        maximum_bytes: usize,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExecutorDiagnostic {
    pub code: ExecutorDiagnosticCode,
    pub stage: ExecutorStage,
    pub severity: DiagnosticSeverity,
    pub retryability: DiagnosticRetryability,
    pub message: String,
    pub subject: ExecutorDiagnosticSubject,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_after_ms: Option<u64>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub cause: Option<Box<Self>>,
}

impl ExecutorDiagnostic {
    #[must_use]
    pub fn error(
        code: ExecutorDiagnosticCode,
        stage: ExecutorStage,
        message: impl Into<String>,
        subject: ExecutorDiagnosticSubject,
    ) -> Self {
        Self {
            code,
            stage,
            severity: DiagnosticSeverity::Error,
            retryability: DiagnosticRetryability::Never,
            message: message.into(),
            subject,
            retry_after_ms: None,
            cause: None,
        }
    }

    #[must_use]
    pub fn warning(
        code: ExecutorDiagnosticCode,
        stage: ExecutorStage,
        message: impl Into<String>,
        subject: ExecutorDiagnosticSubject,
    ) -> Self {
        Self {
            severity: DiagnosticSeverity::Warning,
            ..Self::error(code, stage, message, subject)
        }
    }

    #[must_use]
    pub fn with_retryability(mut self, retryability: DiagnosticRetryability) -> Self {
        self.retryability = retryability;
        self
    }

    #[must_use]
    pub fn with_retry_after(mut self, retry_after: Duration) -> Self {
        self.retryability = DiagnosticRetryability::AfterDelay;
        self.retry_after_ms = Some(duration_ms(retry_after));
        self
    }

    #[must_use]
    pub fn with_cause(mut self, cause: Self) -> Self {
        self.cause = Some(Box::new(cause));
        self
    }
}

impl ExecutorDiagnostic {
    #[must_use]
    pub fn event_too_large(actual_bytes: usize, maximum_bytes: usize) -> Self {
        Self::error(
            ExecutorDiagnosticCode::EventTooLarge,
            ExecutorStage::Output,
            format!("serialized public executor event requires {actual_bytes} bytes, exceeding the maximum of {maximum_bytes} bytes"),
            ExecutorDiagnosticSubject::Event {
                actual_bytes,
                maximum_bytes,
            },
        )
    }

    #[must_use]
    pub fn stream_capacity_exceeded() -> Self {
        Self::error(
            ExecutorDiagnosticCode::StreamCapacityExceeded,
            ExecutorStage::Stream,
            "stream retention capacity is exhausted",
            ExecutorDiagnosticSubject::Stream {
                requested_after: None,
                oldest_available: None,
            },
        )
    }
}

impl fmt::Display for ExecutorDiagnostic {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum McpImportKind {
    Prompt,
    Resource,
}

impl From<&SemanticPlannedMcpImportKind> for McpImportKind {
    fn from(kind: &SemanticPlannedMcpImportKind) -> Self {
        match kind {
            SemanticPlannedMcpImportKind::Prompt => Self::Prompt,
            SemanticPlannedMcpImportKind::Resource => Self::Resource,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlannedMcpImportEvent {
    pub name: String,
    pub kind: McpImportKind,
    pub server_name: String,
    pub item_name: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum McpOperation {
    Call,
    Read,
    Render,
}

impl McpOperation {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Call => "call",
            Self::Read => "read",
            Self::Render => "render",
        }
    }
}

impl From<DslMcpCallOperation> for McpOperation {
    fn from(operation: DslMcpCallOperation) -> Self {
        match operation {
            DslMcpCallOperation::Read => Self::Read,
            DslMcpCallOperation::Render => Self::Render,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EventValueKind {
    Null,
    Boolean,
    Number,
    String,
    Array,
    Object,
}

impl EventValueKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Null => "null",
            Self::Boolean => "boolean",
            Self::Number => "number",
            Self::String => "string",
            Self::Array => "array",
            Self::Object => "object",
        }
    }

    #[must_use]
    pub fn from_value(value: &Value) -> Self {
        match value {
            Value::Null => Self::Null,
            Value::Bool(_) => Self::Boolean,
            Value::Number(_) => Self::Number,
            Value::String(_) => Self::String,
            Value::Array(_) => Self::Array,
            Value::Object(_) => Self::Object,
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct McpCallEventDetails {
    pub operation: McpOperation,
    pub target_name: String,
    pub server_name: String,
    pub item_name: String,
    pub argument_names: Vec<String>,
}

impl McpCallEventDetails {
    #[must_use]
    pub fn new(
        operation: McpOperation,
        target_name: String,
        server_name: String,
        item_name: String,
        mut argument_names: Vec<String>,
    ) -> Self {
        argument_names.sort();
        argument_names.dedup();

        Self {
            operation,
            target_name,
            server_name,
            item_name,
            argument_names,
        }
    }

    #[must_use]
    pub fn from_arguments(operation: McpOperation, target_name: String, server_name: String, item_name: String, arguments: &Value) -> Self {
        Self::new(operation, target_name, server_name, item_name, argument_names(arguments))
    }

    #[must_use]
    fn into_event_data(self) -> serde_json::Map<String, Value> {
        serde_json::Map::from_iter([
            ("operation".to_string(), Value::String(self.operation.as_str().to_string())),
            ("target_name".to_string(), Value::String(self.target_name)),
            ("server_name".to_string(), Value::String(self.server_name)),
            ("item_name".to_string(), Value::String(self.item_name)),
            ("argument_names".to_string(), serde_json::json!(self.argument_names)),
        ])
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
// Keep downstream integration packages in sync when changing executor event
// names or payload shapes. External packages live under integration/.
pub enum ExecutorEventKind {
    WorkflowStarted,
    WorkflowPlanned,
    AgentLoopStarted,
    AgentLoopCompleted,
    AgentLoopFailed,
    AgentLoopCancelled,
    ContextCompactionStarted,
    ContextCompactionCompleted,
    ContextCompactionFailed,
    AgentFileCreated,
    AgentFileDeleted,
    AgentStarted,
    AgentCompleted,
    AgentFailed,
    AgentCancelled,
    ProviderAttemptStarted,
    ProviderAttemptCompleted,
    ProviderAttemptFailed,
    ToolCallStarted,
    ToolCallFailed,
    ToolCallCompleted,
    McpToolSchemaFetchStarted,
    McpToolSchemaFetchFailed,
    McpToolSchemaFetchCompleted,
    McpToolValidationStarted,
    McpToolValidationFailed,
    McpToolValidationCompleted,
    McpCallStarted,
    McpCallFailed,
    McpCallCompleted,
    CacheDegraded,
    StreamGap,
    WorkflowCompleted,
    WorkflowFailed,
    WorkflowCancelled,
}

impl ExecutorEventKind {
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::WorkflowStarted => "workflow_started",
            Self::WorkflowPlanned => "workflow_planned",
            Self::AgentLoopStarted => "agent_loop_started",
            Self::AgentLoopCompleted => "agent_loop_completed",
            Self::AgentLoopFailed => "agent_loop_failed",
            Self::AgentLoopCancelled => "agent_loop_cancelled",
            Self::ContextCompactionStarted => "context_compaction_started",
            Self::ContextCompactionCompleted => "context_compaction_completed",
            Self::ContextCompactionFailed => "context_compaction_failed",
            Self::AgentFileCreated => "agent_file_created",
            Self::AgentFileDeleted => "agent_file_deleted",
            Self::AgentStarted => "agent_started",
            Self::AgentCompleted => "agent_completed",
            Self::AgentFailed => "agent_failed",
            Self::AgentCancelled => "agent_cancelled",
            Self::ProviderAttemptStarted => "provider_attempt_started",
            Self::ProviderAttemptCompleted => "provider_attempt_completed",
            Self::ProviderAttemptFailed => "provider_attempt_failed",
            Self::ToolCallStarted => "tool_call_started",
            Self::ToolCallFailed => "tool_call_failed",
            Self::ToolCallCompleted => "tool_call_completed",
            Self::McpToolSchemaFetchStarted => "mcp_tool_schema_fetch_started",
            Self::McpToolSchemaFetchFailed => "mcp_tool_schema_fetch_failed",
            Self::McpToolSchemaFetchCompleted => "mcp_tool_schema_fetch_completed",
            Self::McpToolValidationStarted => "mcp_tool_validation_started",
            Self::McpToolValidationFailed => "mcp_tool_validation_failed",
            Self::McpToolValidationCompleted => "mcp_tool_validation_completed",
            Self::McpCallStarted => "mcp_call_started",
            Self::McpCallFailed => "mcp_call_failed",
            Self::McpCallCompleted => "mcp_call_completed",
            Self::CacheDegraded => "cache_degraded",
            Self::StreamGap => "stream_gap",
            Self::WorkflowCompleted => "workflow_completed",
            Self::WorkflowFailed => "workflow_failed",
            Self::WorkflowCancelled => "workflow_cancelled",
        }
    }
}

impl ExecutorEventKind {
    #[must_use]
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::WorkflowCompleted | Self::WorkflowFailed | Self::WorkflowCancelled)
    }

    fn maximum_sse_frame_bytes(&self, serialized_data_bytes: usize) -> usize {
        SSE_EVENT_FIELD_PREFIX_BYTES
            .saturating_add(self.as_str().len())
            .saturating_add(SSE_FIELD_LINE_FEED_BYTES)
            .saturating_add(SSE_IDENTIFIER_FIELD_PREFIX_BYTES)
            .saturating_add(MAX_EVENT_IDENTIFIER_DECIMAL_BYTES)
            .saturating_add(SSE_FIELD_LINE_FEED_BYTES)
            .saturating_add(SSE_DATA_FIELD_PREFIX_BYTES)
            .saturating_add(serialized_data_bytes)
            .saturating_add(SSE_FIELD_LINE_FEED_BYTES)
            .saturating_add(SSE_FINAL_LINE_FEED_BYTES)
    }
}

#[derive(Debug)]
pub enum PublicEventSerializationError {
    Serialization { message: String },
    TooLarge { actual_bytes: usize, maximum_bytes: usize },
}

impl fmt::Display for PublicEventSerializationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Serialization { message } => formatter.write_str(message),
            Self::TooLarge {
                actual_bytes,
                maximum_bytes,
            } => write!(
                formatter,
                "serialized public executor event requires {actual_bytes} bytes, exceeding the maximum of {maximum_bytes} bytes"
            ),
        }
    }
}

impl Error for PublicEventSerializationError {}

#[derive(Debug)]
pub struct SerializedPublicExecutorEvent {
    event: ExecutorEvent,
    serialized_data: String,
    maximum_sse_frame_bytes: usize,
}

impl SerializedPublicExecutorEvent {
    #[must_use]
    pub fn event(&self) -> &ExecutorEvent {
        &self.event
    }

    #[must_use]
    pub fn serialized_data(&self) -> &str {
        &self.serialized_data
    }

    #[must_use]
    pub fn maximum_sse_frame_bytes(&self) -> usize {
        self.maximum_sse_frame_bytes
    }

    #[must_use]
    pub fn into_parts(self) -> (ExecutorEvent, String, usize) {
        (self.event, self.serialized_data, self.maximum_sse_frame_bytes)
    }
}

#[derive(Debug)]
struct BoundedPublicEventWriter {
    serialized_data: Vec<u8>,
    serialized_data_bytes: usize,
    maximum_buffer_bytes: usize,
}

impl BoundedPublicEventWriter {
    fn new(maximum_buffer_bytes: usize) -> Self {
        Self {
            serialized_data: Vec::new(),
            serialized_data_bytes: 0,
            maximum_buffer_bytes,
        }
    }

    fn into_parts(self) -> (Vec<u8>, usize) {
        (self.serialized_data, self.serialized_data_bytes)
    }
}

impl io::Write for BoundedPublicEventWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.serialized_data_bytes = self.serialized_data_bytes.saturating_add(bytes.len());
        let remaining_buffer_bytes = self.maximum_buffer_bytes.saturating_sub(self.serialized_data.len());
        let buffered_byte_count = remaining_buffer_bytes.min(bytes.len());

        self.serialized_data.extend_from_slice(&bytes[..buffered_byte_count]);

        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExecutorEvent {
    pub kind: ExecutorEventKind,

    pub timestamp_ms: u64,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_name: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub diagnostic: Option<ExecutorDiagnostic>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl ExecutorEvent {
    #[must_use]
    pub fn workflow_started() -> Self {
        Self::new(ExecutorEventKind::WorkflowStarted)
    }

    #[must_use]
    pub fn workflow_planned(agent_execution_order: Vec<String>, mcp_imports: Vec<PlannedMcpImportEvent>, steps: Value) -> Self {
        Self::new(ExecutorEventKind::WorkflowPlanned).with_data(serde_json::json!({
            "agent_execution_order": agent_execution_order,
            "mcp_imports": mcp_imports,
            "steps": steps,
        }))
    }

    #[must_use]
    pub fn agent_loop_started(agent_name: String, iteration_count: usize, mut binding_names: Vec<String>) -> Self {
        binding_names.sort();
        binding_names.dedup();

        Self::new(ExecutorEventKind::AgentLoopStarted)
            .with_agent_name(agent_name)
            .with_data(serde_json::json!({
                "iteration_count": iteration_count,
                "binding_names": binding_names,
            }))
    }

    #[must_use]
    pub fn agent_loop_completed(agent_name: String, duration: Duration, iteration_count: usize) -> Self {
        Self::new(ExecutorEventKind::AgentLoopCompleted)
            .with_agent_name(agent_name)
            .with_data(serde_json::json!({
                "result_kind": EventValueKind::Array.as_str(),
                "item_count": iteration_count,
                "duration_ms": duration_ms(duration),
                "iteration_count": iteration_count,
            }))
    }

    #[must_use]
    pub fn agent_loop_failed(agent_name: String, diagnostic: ExecutorDiagnostic, duration: Duration) -> Self {
        Self::new(ExecutorEventKind::AgentLoopFailed)
            .with_agent_name(agent_name)
            .with_message(diagnostic.message.clone())
            .with_diagnostic(diagnostic)
            .with_data(serde_json::json!({
                "duration_ms": duration_ms(duration),
            }))
    }

    #[must_use]
    pub fn agent_loop_cancelled(agent_name: String, diagnostic: ExecutorDiagnostic, duration: Duration) -> Self {
        Self::new(ExecutorEventKind::AgentLoopCancelled)
            .with_agent_name(agent_name)
            .with_message(diagnostic.message.clone())
            .with_diagnostic(diagnostic)
            .with_data(serde_json::json!({
                "duration_ms": duration_ms(duration),
            }))
    }

    #[must_use]
    pub fn context_compaction_started(agent_name: String, source_agent_name: Option<String>, model_name: String) -> Self {
        let mut event_data = serde_json::Map::from_iter([("model".to_string(), Value::String(model_name))]);

        if let Some(source_agent_name) = source_agent_name {
            event_data.insert("source_agent_name".to_string(), Value::String(source_agent_name));
        }

        Self::new(ExecutorEventKind::ContextCompactionStarted)
            .with_agent_name(agent_name)
            .with_data(Value::Object(event_data))
    }

    #[must_use]
    pub fn context_compaction_completed(agent_name: String, output: &Value, duration: Duration) -> Self {
        let mut event_data = result_metadata(output);
        event_data.insert("duration_ms".to_string(), serde_json::json!(duration_ms(duration)));

        Self::new(ExecutorEventKind::ContextCompactionCompleted)
            .with_agent_name(agent_name)
            .with_data(Value::Object(event_data))
    }

    #[must_use]
    pub fn context_compaction_failed(agent_name: String, diagnostic: ExecutorDiagnostic, duration: Duration) -> Self {
        Self::new(ExecutorEventKind::ContextCompactionFailed)
            .with_agent_name(agent_name)
            .with_message(diagnostic.message.clone())
            .with_diagnostic(diagnostic)
            .with_data(serde_json::json!({
                "duration_ms": duration_ms(duration),
            }))
    }

    #[must_use]
    pub fn agent_started(agent_name: String, model_name: String, tool_names: Vec<String>, iteration_index: Option<usize>) -> Self {
        let mut event_data = serde_json::Map::from_iter([
            ("model".to_string(), Value::String(model_name)),
            ("tools".to_string(), serde_json::json!(tool_names)),
        ]);

        if let Some(iteration_index) = iteration_index {
            event_data.insert("iteration_index".to_string(), serde_json::json!(iteration_index));
        }

        Self::new(ExecutorEventKind::AgentStarted)
            .with_agent_name(agent_name)
            .with_data(Value::Object(event_data))
    }

    #[must_use]
    pub fn agent_file_created(agent_name: String, filename: String, purpose: String, bytes: Option<u64>) -> Self {
        let mut event_data = serde_json::Map::from_iter([
            ("filename".to_string(), Value::String(filename)),
            ("purpose".to_string(), Value::String(purpose)),
        ]);

        if let Some(bytes) = bytes {
            event_data.insert("bytes".to_string(), serde_json::json!(bytes));
        }

        Self::new(ExecutorEventKind::AgentFileCreated)
            .with_agent_name(agent_name)
            .with_data(Value::Object(event_data))
    }

    #[must_use]
    pub fn agent_file_deleted(agent_name: String, filename: String, purpose: String) -> Self {
        Self::new(ExecutorEventKind::AgentFileDeleted)
            .with_agent_name(agent_name)
            .with_data(serde_json::json!({
                "filename": filename,
                "purpose": purpose,
            }))
    }

    #[must_use]
    pub fn agent_completed(
        agent_name: String,
        output: &Value,
        duration: Duration,
        iteration_index: Option<usize>,
        cache_hit: bool,
    ) -> Self {
        let mut event_data = result_metadata(output);
        event_data.insert("duration_ms".to_string(), serde_json::json!(duration_ms(duration)));
        event_data.insert("cache_hit".to_string(), Value::Bool(cache_hit));

        if let Some(iteration_index) = iteration_index {
            event_data.insert("iteration_index".to_string(), serde_json::json!(iteration_index));
        }

        Self::new(ExecutorEventKind::AgentCompleted)
            .with_agent_name(agent_name)
            .with_data(Value::Object(event_data))
    }

    #[must_use]
    pub fn agent_failed(agent_name: String, diagnostic: ExecutorDiagnostic, duration: Duration, iteration_index: Option<usize>) -> Self {
        let mut event_data = serde_json::Map::from_iter([("duration_ms".to_string(), serde_json::json!(duration_ms(duration)))]);

        if let Some(iteration_index) = iteration_index {
            event_data.insert("iteration_index".to_string(), serde_json::json!(iteration_index));
        }

        Self::new(ExecutorEventKind::AgentFailed)
            .with_agent_name(agent_name)
            .with_message(diagnostic.message.clone())
            .with_diagnostic(diagnostic)
            .with_data(Value::Object(event_data))
    }

    #[must_use]
    pub fn agent_cancelled(agent_name: String, diagnostic: ExecutorDiagnostic, duration: Duration, iteration_index: Option<usize>) -> Self {
        let mut event_data = serde_json::Map::from_iter([("duration_ms".to_string(), serde_json::json!(duration_ms(duration)))]);

        if let Some(iteration_index) = iteration_index {
            event_data.insert("iteration_index".to_string(), serde_json::json!(iteration_index));
        }

        Self::new(ExecutorEventKind::AgentCancelled)
            .with_agent_name(agent_name)
            .with_message(diagnostic.message.clone())
            .with_diagnostic(diagnostic)
            .with_data(Value::Object(event_data))
    }

    #[must_use]
    pub fn provider_attempt_started(
        agent_name: String,
        provider_name: String,
        model_name: String,
        attempt: u32,
        total_attempts: u32,
    ) -> Self {
        Self::new(ExecutorEventKind::ProviderAttemptStarted)
            .with_agent_name(agent_name)
            .with_data(serde_json::json!({
                "provider_name": provider_name,
                "model_name": model_name,
                "attempt": attempt,
                "total_attempts": total_attempts,
            }))
    }

    #[must_use]
    pub fn provider_attempt_completed(
        agent_name: String,
        provider_name: String,
        model_name: String,
        attempt: u32,
        total_attempts: u32,
        duration: Duration,
    ) -> Self {
        Self::new(ExecutorEventKind::ProviderAttemptCompleted)
            .with_agent_name(agent_name)
            .with_data(serde_json::json!({
                "provider_name": provider_name,
                "model_name": model_name,
                "attempt": attempt,
                "total_attempts": total_attempts,
                "duration_ms": duration_ms(duration),
            }))
    }

    #[must_use]
    pub fn provider_attempt_failed(
        agent_name: String,
        provider_name: String,
        model_name: String,
        attempt: u32,
        total_attempts: u32,
        diagnostic: ExecutorDiagnostic,
    ) -> Self {
        Self::new(ExecutorEventKind::ProviderAttemptFailed)
            .with_agent_name(agent_name)
            .with_message(diagnostic.message.clone())
            .with_diagnostic(diagnostic)
            .with_data(serde_json::json!({
                "provider_name": provider_name,
                "model_name": model_name,
                "attempt": attempt,
                "total_attempts": total_attempts,
            }))
    }

    #[must_use]
    pub fn tool_call_started(agent_name: String, tool_name: String, arguments: &Value) -> Self {
        Self::new(ExecutorEventKind::ToolCallStarted)
            .with_agent_name(agent_name)
            .with_data(serde_json::json!({
                "tool_name": tool_name,
                "argument_names": argument_names(arguments),
            }))
    }

    #[must_use]
    pub fn tool_call_completed(agent_name: String, tool_name: String, result: &Value, duration: Duration) -> Self {
        let mut event_data = result_metadata(result);
        event_data.insert("tool_name".to_string(), Value::String(tool_name));
        event_data.insert("duration_ms".to_string(), serde_json::json!(duration_ms(duration)));

        Self::new(ExecutorEventKind::ToolCallCompleted)
            .with_agent_name(agent_name)
            .with_data(Value::Object(event_data))
    }

    #[must_use]
    pub fn tool_call_failed(agent_name: String, tool_name: String, duration: Duration) -> Self {
        let diagnostic = ExecutorDiagnostic::error(
            ExecutorDiagnosticCode::ToolFailed,
            ExecutorStage::Tool,
            format!("tool `{tool_name}` call failed"),
            ExecutorDiagnosticSubject::Tool {
                agent_name: Some(agent_name.clone()),
                tool_name: tool_name.clone(),
            },
        );

        Self::new(ExecutorEventKind::ToolCallFailed)
            .with_agent_name(agent_name)
            .with_message(diagnostic.message.clone())
            .with_diagnostic(diagnostic)
            .with_data(serde_json::json!({
                "tool_name": tool_name,
                "duration_ms": duration_ms(duration),
            }))
    }

    #[must_use]
    pub fn mcp_tool_schema_fetch_started(server_name: String) -> Self {
        Self::new(ExecutorEventKind::McpToolSchemaFetchStarted).with_data(serde_json::json!({
            "server_name": server_name,
        }))
    }

    #[must_use]
    pub fn mcp_tool_schema_fetch_completed(server_name: String, tool_count: usize, duration: Duration) -> Self {
        Self::new(ExecutorEventKind::McpToolSchemaFetchCompleted).with_data(serde_json::json!({
            "server_name": server_name,
            "tool_count": tool_count,
            "duration_ms": duration_ms(duration),
        }))
    }

    #[must_use]
    pub fn mcp_tool_schema_fetch_failed(server_name: String, duration: Duration) -> Self {
        let diagnostic = ExecutorDiagnostic::error(
            ExecutorDiagnosticCode::McpFailed,
            ExecutorStage::Mcp,
            format!("MCP tool schema fetch failed for server `{server_name}`"),
            ExecutorDiagnosticSubject::Mcp {
                agent_name: None,
                server_name: Some(server_name.clone()),
                target_name: None,
            },
        )
        .with_retryability(DiagnosticRetryability::Unknown);

        Self::new(ExecutorEventKind::McpToolSchemaFetchFailed)
            .with_message(diagnostic.message.clone())
            .with_diagnostic(diagnostic)
            .with_data(serde_json::json!({
                "server_name": server_name,
                "duration_ms": duration_ms(duration),
            }))
    }

    #[must_use]
    pub fn mcp_tool_validation_started(agent_name: String, tool_name: String, arguments: &Value) -> Self {
        Self::new(ExecutorEventKind::McpToolValidationStarted)
            .with_agent_name(agent_name)
            .with_data(serde_json::json!({
                "tool_name": tool_name,
                "argument_names": argument_names(arguments),
            }))
    }

    #[must_use]
    pub fn mcp_tool_validation_completed(agent_name: String, tool_name: String, duration: Duration) -> Self {
        Self::new(ExecutorEventKind::McpToolValidationCompleted)
            .with_agent_name(agent_name)
            .with_data(serde_json::json!({
                "tool_name": tool_name,
                "duration_ms": duration_ms(duration),
            }))
    }

    #[must_use]
    pub fn mcp_tool_validation_failed(agent_name: String, tool_name: String, duration: Duration) -> Self {
        let diagnostic = ExecutorDiagnostic::error(
            ExecutorDiagnosticCode::ToolFailed,
            ExecutorStage::Tool,
            format!("MCP tool validation failed for `{tool_name}`"),
            ExecutorDiagnosticSubject::Tool {
                agent_name: Some(agent_name.clone()),
                tool_name: tool_name.clone(),
            },
        );

        Self::new(ExecutorEventKind::McpToolValidationFailed)
            .with_agent_name(agent_name)
            .with_message(diagnostic.message.clone())
            .with_diagnostic(diagnostic)
            .with_data(serde_json::json!({
                "tool_name": tool_name,
                "duration_ms": duration_ms(duration),
            }))
    }

    #[must_use]
    pub fn mcp_call_started(details: McpCallEventDetails) -> Self {
        Self::new(ExecutorEventKind::McpCallStarted).with_data(Value::Object(details.into_event_data()))
    }

    #[must_use]
    pub fn mcp_call_completed(details: McpCallEventDetails, result: &Value, duration: Duration) -> Self {
        let mut event_data = details.into_event_data();
        event_data.extend(result_metadata(result));
        event_data.insert("duration_ms".to_string(), serde_json::json!(duration_ms(duration)));

        Self::new(ExecutorEventKind::McpCallCompleted).with_data(Value::Object(event_data))
    }

    #[must_use]
    pub fn mcp_call_failed(details: McpCallEventDetails, duration: Duration) -> Self {
        let diagnostic = ExecutorDiagnostic::error(
            ExecutorDiagnosticCode::McpFailed,
            ExecutorStage::Mcp,
            format!("MCP {} failed for `{}`", details.operation.as_str(), details.target_name),
            ExecutorDiagnosticSubject::Mcp {
                agent_name: None,
                server_name: Some(details.server_name.clone()),
                target_name: Some(details.target_name.clone()),
            },
        )
        .with_retryability(DiagnosticRetryability::Unknown);
        let mut event_data = details.into_event_data();

        event_data.insert("duration_ms".to_string(), serde_json::json!(duration_ms(duration)));

        Self::new(ExecutorEventKind::McpCallFailed)
            .with_message(diagnostic.message.clone())
            .with_diagnostic(diagnostic)
            .with_data(Value::Object(event_data))
    }

    #[must_use]
    pub fn cache_degraded(agent_name: Option<String>, diagnostic: ExecutorDiagnostic) -> Self {
        let mut event = Self::new(ExecutorEventKind::CacheDegraded)
            .with_message(diagnostic.message.clone())
            .with_diagnostic(diagnostic);

        if let Some(agent_name) = agent_name {
            event = event.with_agent_name(agent_name);
        }

        event
    }

    #[must_use]
    pub fn stream_gap(diagnostic: ExecutorDiagnostic) -> Self {
        Self::new(ExecutorEventKind::StreamGap)
            .with_message(diagnostic.message.clone())
            .with_diagnostic(diagnostic)
    }

    #[must_use]
    pub fn workflow_completed(output: Value, duration: Duration) -> Self {
        Self::new(ExecutorEventKind::WorkflowCompleted).with_data(serde_json::json!({
            "output": output,
            "duration_ms": duration_ms(duration),
        }))
    }

    #[must_use]
    pub fn workflow_failed(diagnostic: ExecutorDiagnostic, duration: Option<Duration>) -> Self {
        let event = Self::new(ExecutorEventKind::WorkflowFailed)
            .with_message(diagnostic.message.clone())
            .with_diagnostic(diagnostic);

        match duration {
            Some(duration) => event.with_data(serde_json::json!({ "duration_ms": duration_ms(duration) })),
            None => event,
        }
    }

    #[must_use]
    pub fn workflow_cancelled(diagnostic: ExecutorDiagnostic, duration: Option<Duration>) -> Self {
        let event = Self::new(ExecutorEventKind::WorkflowCancelled)
            .with_message(diagnostic.message.clone())
            .with_diagnostic(diagnostic);

        match duration {
            Some(duration) => event.with_data(serde_json::json!({ "duration_ms": duration_ms(duration) })),
            None => event,
        }
    }

    fn new(kind: ExecutorEventKind) -> Self {
        Self {
            kind,
            timestamp_ms: Self::current_timestamp_ms(),
            agent_name: None,
            message: None,
            diagnostic: None,
            data: None,
        }
    }

    #[must_use]
    pub fn is_terminal(&self) -> bool {
        self.kind.is_terminal()
    }

    pub fn into_serialized_public(self) -> Result<SerializedPublicExecutorEvent, PublicEventSerializationError> {
        let event = self.into_public();
        let maximum_serialized_data_bytes = MAX_SERIALIZED_PUBLIC_EVENT_BYTES.saturating_sub(event.kind.maximum_sse_frame_bytes(0));
        let mut event_writer = BoundedPublicEventWriter::new(maximum_serialized_data_bytes);

        serde_json::to_writer(&mut event_writer, &event).map_err(|error| PublicEventSerializationError::Serialization {
            message: format!("failed to serialize public executor event: {error}"),
        })?;

        let (serialized_data, serialized_data_bytes) = event_writer.into_parts();
        let maximum_sse_frame_bytes = event.kind.maximum_sse_frame_bytes(serialized_data_bytes);

        if maximum_sse_frame_bytes > MAX_SERIALIZED_PUBLIC_EVENT_BYTES {
            return Err(PublicEventSerializationError::TooLarge {
                actual_bytes: maximum_sse_frame_bytes,
                maximum_bytes: MAX_SERIALIZED_PUBLIC_EVENT_BYTES,
            });
        }

        let serialized_data = String::from_utf8(serialized_data).map_err(|error| PublicEventSerializationError::Serialization {
            message: format!("failed to encode public executor event as UTF-8: {error}"),
        })?;

        Ok(SerializedPublicExecutorEvent {
            event,
            serialized_data,
            maximum_sse_frame_bytes,
        })
    }

    #[must_use]
    pub fn into_public(mut self) -> Self {
        let Some(Value::Object(mut event_data)) = self.data.take() else {
            return self;
        };
        let public_field_names = match self.kind {
            ExecutorEventKind::WorkflowStarted | ExecutorEventKind::CacheDegraded | ExecutorEventKind::StreamGap => &[][..],
            ExecutorEventKind::WorkflowPlanned => &["agent_execution_order", "mcp_imports", "steps"],
            ExecutorEventKind::AgentLoopStarted => &["iteration_count", "binding_names"],
            ExecutorEventKind::AgentLoopCompleted => &["result_kind", "item_count", "duration_ms", "iteration_count"],
            ExecutorEventKind::AgentLoopFailed | ExecutorEventKind::AgentLoopCancelled | ExecutorEventKind::ContextCompactionFailed => {
                &["duration_ms"]
            }
            ExecutorEventKind::ContextCompactionStarted => &["model", "source_agent_name"],
            ExecutorEventKind::ContextCompactionCompleted => &["result_kind", "item_count", "duration_ms"],
            ExecutorEventKind::AgentFileCreated => &["filename", "purpose", "bytes"],
            ExecutorEventKind::AgentFileDeleted => &["filename", "purpose"],
            ExecutorEventKind::AgentStarted => &["model", "tools", "iteration_index"],
            ExecutorEventKind::AgentCompleted => &["result_kind", "item_count", "duration_ms", "cache_hit", "iteration_index"],
            ExecutorEventKind::AgentFailed | ExecutorEventKind::AgentCancelled => &["duration_ms", "iteration_index"],
            ExecutorEventKind::ProviderAttemptStarted | ExecutorEventKind::ProviderAttemptFailed => {
                &["provider_name", "model_name", "attempt", "total_attempts"]
            }
            ExecutorEventKind::ProviderAttemptCompleted => &["provider_name", "model_name", "attempt", "total_attempts", "duration_ms"],
            ExecutorEventKind::ToolCallStarted => &["tool_name", "argument_names"],
            ExecutorEventKind::ToolCallFailed => &["tool_name", "duration_ms"],
            ExecutorEventKind::ToolCallCompleted => &["tool_name", "result_kind", "item_count", "duration_ms"],
            ExecutorEventKind::McpToolSchemaFetchStarted => &["server_name"],
            ExecutorEventKind::McpToolSchemaFetchFailed => &["server_name", "duration_ms"],
            ExecutorEventKind::McpToolSchemaFetchCompleted => &["server_name", "tool_count", "duration_ms"],
            ExecutorEventKind::McpToolValidationStarted => &["tool_name", "argument_names"],
            ExecutorEventKind::McpToolValidationFailed | ExecutorEventKind::McpToolValidationCompleted => &["tool_name", "duration_ms"],
            ExecutorEventKind::McpCallStarted => &["operation", "target_name", "server_name", "item_name", "argument_names"],
            ExecutorEventKind::McpCallFailed => &[
                "operation",
                "target_name",
                "server_name",
                "item_name",
                "argument_names",
                "duration_ms",
            ],
            ExecutorEventKind::McpCallCompleted => &[
                "operation",
                "target_name",
                "server_name",
                "item_name",
                "argument_names",
                "result_kind",
                "item_count",
                "duration_ms",
            ],
            ExecutorEventKind::WorkflowCompleted => &["output", "duration_ms"],
            ExecutorEventKind::WorkflowFailed | ExecutorEventKind::WorkflowCancelled => &["duration_ms"],
        };

        event_data.retain(|field_name, _field_value| public_field_names.contains(&field_name.as_str()));

        if !event_data.is_empty() {
            self.data = Some(Value::Object(event_data));
        }

        self
    }

    #[must_use]
    pub fn current_timestamp_ms() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
            .try_into()
            .unwrap_or(u64::MAX)
    }

    #[must_use]
    pub fn with_agent_name(mut self, agent_name: String) -> Self {
        self.agent_name = Some(agent_name);
        self
    }

    fn with_message(mut self, message: String) -> Self {
        self.message = Some(message);
        self
    }

    #[must_use]
    pub fn with_diagnostic(mut self, diagnostic: ExecutorDiagnostic) -> Self {
        self.diagnostic = Some(diagnostic);
        self
    }

    fn with_data(mut self, data: Value) -> Self {
        self.data = Some(data);
        self
    }
}

fn duration_ms(duration: Duration) -> u64 {
    duration.as_millis().try_into().unwrap_or(u64::MAX)
}

fn argument_names(arguments: &Value) -> Vec<String> {
    let mut names = arguments
        .as_object()
        .map(|arguments| arguments.keys().cloned().collect::<Vec<_>>())
        .unwrap_or_default();

    names.sort();
    names
}

fn result_metadata(result: &Value) -> serde_json::Map<String, Value> {
    let mut metadata = serde_json::Map::from_iter([(
        "result_kind".to_string(),
        Value::String(EventValueKind::from_value(result).as_str().to_string()),
    )]);
    let item_count = match result {
        Value::Array(items) => Some(items.len()),
        Value::Object(fields) => Some(fields.len()),
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => None,
    };

    if let Some(item_count) = item_count {
        metadata.insert("item_count".to_string(), serde_json::json!(item_count));
    }

    metadata
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn provider_attempt_failure_serializes_typed_diagnostic_contract() {
        let cause = ExecutorDiagnostic::error(
            ExecutorDiagnosticCode::ModelProviderFailed,
            ExecutorStage::Model,
            "HTTP 503",
            ExecutorDiagnosticSubject::Provider {
                agent_name: "writer".to_string(),
                provider_name: Some("openai".to_string()),
                model_name: Some("gpt-test".to_string()),
                attempt: Some(2),
                http_status: Some(503),
            },
        )
        .with_retryability(DiagnosticRetryability::Safe);
        let diagnostic = ExecutorDiagnostic::error(
            ExecutorDiagnosticCode::ProviderRetriesExhausted,
            ExecutorStage::Model,
            "provider request failed after 2 attempts",
            cause.subject.clone(),
        )
        .with_retryability(DiagnosticRetryability::Safe)
        .with_cause(cause);
        let event =
            ExecutorEvent::provider_attempt_failed("writer".to_string(), "openai".to_string(), "gpt-test".to_string(), 2, 2, diagnostic);
        let mut event_value = serde_json::to_value(event).expect("event should serialize");

        event_value
            .as_object_mut()
            .expect("event should be an object")
            .remove("timestamp_ms");

        assert_eq!(
            event_value,
            json!({
                "kind": "provider_attempt_failed",
                "agent_name": "writer",
                "message": "provider request failed after 2 attempts",
                "diagnostic": {
                    "code": "provider_retries_exhausted",
                    "stage": "model",
                    "severity": "error",
                    "retryability": "safe",
                    "message": "provider request failed after 2 attempts",
                    "subject": {
                        "type": "provider",
                        "agent_name": "writer",
                        "provider_name": "openai",
                        "model_name": "gpt-test",
                        "attempt": 2,
                        "http_status": 503,
                    },
                    "cause": {
                        "code": "model_provider_failed",
                        "stage": "model",
                        "severity": "error",
                        "retryability": "safe",
                        "message": "HTTP 503",
                        "subject": {
                            "type": "provider",
                            "agent_name": "writer",
                            "provider_name": "openai",
                            "model_name": "gpt-test",
                            "attempt": 2,
                            "http_status": 503,
                        },
                    },
                },
                "data": {
                    "provider_name": "openai",
                    "model_name": "gpt-test",
                    "attempt": 2,
                    "total_attempts": 2,
                },
            })
        );
    }

    #[test]
    fn workflow_cancellation_is_terminal_and_serializes_duration() {
        let event = ExecutorEvent::workflow_cancelled(
            ExecutorDiagnostic::error(
                ExecutorDiagnosticCode::Cancelled,
                ExecutorStage::Cancellation,
                "workflow execution was cancelled",
                ExecutorDiagnosticSubject::Workflow,
            ),
            Some(Duration::from_millis(7)),
        );
        let event_value = serde_json::to_value(&event).expect("event should serialize");

        assert!(event.is_terminal());
        assert_eq!(event_value["kind"], "workflow_cancelled");
        assert_eq!(event_value["diagnostic"]["code"], "cancelled");
        assert_eq!(event_value["data"]["duration_ms"], 7);
    }

    #[test]
    fn event_payloads_serialize_only_public_metadata() {
        const SECRET_SENTINEL: &str = "superwire-secret-sentinel";
        let arguments = json!({
            "z_secret": SECRET_SENTINEL,
            "a_public": "value",
        });
        let result = json!({
            "secret": SECRET_SENTINEL,
            "visible": [1, 2],
        });
        let call_details = McpCallEventDetails::from_arguments(
            McpOperation::Call,
            "search".to_string(),
            "local".to_string(),
            "search".to_string(),
            &arguments,
        );
        let events = [
            ExecutorEvent::agent_loop_started("writer".to_string(), 2, vec!["z_binding".to_string(), "a_binding".to_string()]),
            ExecutorEvent::context_compaction_completed("writer".to_string(), &result, Duration::from_millis(3)),
            ExecutorEvent::tool_call_started("writer".to_string(), "search".to_string(), &arguments),
            ExecutorEvent::tool_call_completed("writer".to_string(), "search".to_string(), &result, Duration::from_millis(4)),
            ExecutorEvent::mcp_tool_validation_started("writer".to_string(), "search".to_string(), &arguments),
            ExecutorEvent::mcp_call_started(call_details.clone()),
            ExecutorEvent::mcp_call_completed(call_details, &result, Duration::from_millis(5)),
        ];
        let serialized_events = serde_json::to_string(&events).expect("events should serialize");

        assert!(!serialized_events.contains(SECRET_SENTINEL));
        assert!(!serialized_events.contains("\"arguments\""));
        assert!(!serialized_events.contains("\"params\""));
        assert!(!serialized_events.contains("\"result\""));
        assert!(!serialized_events.contains("\"output\""));
        assert!(!serialized_events.contains("\"raw_result\""));
        assert!(serialized_events.contains("\"argument_names\":[\"a_public\",\"z_secret\"]"));
        assert!(serialized_events.contains("\"result_kind\":\"object\""));
        assert!(serialized_events.contains("\"item_count\":2"));
    }

    #[test]
    fn public_projection_removes_unrecognized_payload_fields() {
        const SECRET_SENTINEL: &str = "superwire-secret-sentinel";
        let event = ExecutorEvent::new(ExecutorEventKind::ToolCallCompleted)
            .with_data(json!({
                "tool_name": "search",
                "result_kind": "object",
                "item_count": 1,
                "duration_ms": 5,
                "result": SECRET_SENTINEL,
                "future_private_field": SECRET_SENTINEL,
            }))
            .into_public();
        let serialized_event = serde_json::to_string(&event).expect("event should serialize");

        assert!(!serialized_event.contains(SECRET_SENTINEL));
        assert!(!serialized_event.contains("\"result\""));
        assert!(!serialized_event.contains("\"future_private_field\""));
    }

    #[test]
    fn public_event_limit_counts_sse_framing_multibyte_and_json_escaping_bytes() {
        for fragment in ["a", "é", "\""] {
            let mut fitting_repetitions = 0_usize;
            let mut oversized_repetitions = MAX_SERIALIZED_PUBLIC_EVENT_BYTES + 1;

            while fitting_repetitions + 1 < oversized_repetitions {
                let candidate_repetitions = fitting_repetitions + (oversized_repetitions - fitting_repetitions) / 2;
                let candidate_event = ExecutorEvent::workflow_completed(json!(fragment.repeat(candidate_repetitions)), Duration::ZERO);

                if candidate_event.into_serialized_public().is_ok() {
                    fitting_repetitions = candidate_repetitions;
                } else {
                    oversized_repetitions = candidate_repetitions;
                }
            }

            let fitting_event = ExecutorEvent::workflow_completed(json!(fragment.repeat(fitting_repetitions)), Duration::ZERO)
                .into_serialized_public()
                .expect("largest fitting event should serialize");
            let oversized_error = ExecutorEvent::workflow_completed(json!(fragment.repeat(oversized_repetitions)), Duration::ZERO)
                .into_serialized_public()
                .expect_err("next event should exceed the serialized public event limit");

            assert!(fitting_event.maximum_sse_frame_bytes() <= MAX_SERIALIZED_PUBLIC_EVENT_BYTES);
            assert!(fitting_event.serialized_data().contains(fragment));

            let PublicEventSerializationError::TooLarge {
                actual_bytes,
                maximum_bytes,
            } = oversized_error
            else {
                panic!("oversized public event should return its framed byte counts");
            };

            assert!(actual_bytes > maximum_bytes);
            assert_eq!(maximum_bytes, MAX_SERIALIZED_PUBLIC_EVENT_BYTES);
        }
    }

    #[test]
    fn event_too_large_diagnostic_serializes_only_byte_counts() {
        const SECRET_SENTINEL: &str = "superwire-secret-sentinel";

        let diagnostic = ExecutorDiagnostic::event_too_large(245_761, MAX_SERIALIZED_PUBLIC_EVENT_BYTES);
        let serialized_diagnostic = serde_json::to_string(&diagnostic).expect("diagnostic should serialize");

        assert_eq!(diagnostic.code, ExecutorDiagnosticCode::EventTooLarge);
        assert_eq!(diagnostic.stage, ExecutorStage::Output);
        assert!(serialized_diagnostic.contains("\"type\":\"event\""));
        assert!(serialized_diagnostic.contains("\"actual_bytes\":245761"));
        assert!(serialized_diagnostic.contains("\"maximum_bytes\":245760"));
        assert!(!serialized_diagnostic.contains(SECRET_SENTINEL));
    }
}
