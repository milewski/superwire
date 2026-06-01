use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const EVENT_DURATION_FIELD_NAME: &str = "duration_ms";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PlannedMcpImportEvent {
    pub name: String,
    pub kind: String,
    pub server_name: String,
    pub item_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct McpCallEventDetails {
    pub operation: String,
    pub target_name: String,
    pub server_name: String,
    pub item_name: String,
    pub arguments: Value,
    pub input_schema: Option<Value>,
}

impl McpCallEventDetails {
    #[must_use]
    pub fn new(
        operation: String,
        target_name: String,
        server_name: String,
        item_name: String,
        arguments: Value,
        input_schema: Option<Value>,
    ) -> Self {
        Self {
            operation,
            target_name,
            server_name,
            item_name,
            arguments,
            input_schema,
        }
    }

    #[must_use]
    fn into_event_data(self) -> serde_json::Map<String, Value> {
        let legacy_parameters = self.arguments.clone();
        let mut event_data = serde_json::Map::from_iter([
            ("operation".to_string(), Value::String(self.operation)),
            ("target_name".to_string(), Value::String(self.target_name)),
            ("server_name".to_string(), Value::String(self.server_name)),
            ("item_name".to_string(), Value::String(self.item_name)),
            ("arguments".to_string(), self.arguments),
            ("params".to_string(), legacy_parameters),
        ]);

        if let Some(input_schema) = self.input_schema {
            event_data.insert("input_schema".to_string(), input_schema);
        }

        event_data
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
// Keep downstream integration packages in sync when changing executor event
// names or payload shapes. External packages live under integration/.
pub enum ExecutorEventKind {
    WorkflowStarted,
    WorkflowPlanned,
    AgentLoopStarted,
    AgentLoopCompleted,
    ContextCompactionStarted,
    ContextCompactionCompleted,
    ContextCompactionFailed,
    AgentStarted,
    AgentCompleted,
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
    WorkflowCompleted,
    WorkflowFailed,
}

impl ExecutorEventKind {
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::WorkflowStarted => "workflow_started",
            Self::WorkflowPlanned => "workflow_planned",
            Self::AgentLoopStarted => "agent_loop_started",
            Self::AgentLoopCompleted => "agent_loop_completed",
            Self::ContextCompactionStarted => "context_compaction_started",
            Self::ContextCompactionCompleted => "context_compaction_completed",
            Self::ContextCompactionFailed => "context_compaction_failed",
            Self::AgentStarted => "agent_started",
            Self::AgentCompleted => "agent_completed",
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
            Self::WorkflowCompleted => "workflow_completed",
            Self::WorkflowFailed => "workflow_failed",
        }
    }
}

impl ExecutorEventKind {
    #[must_use]
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::WorkflowCompleted | Self::WorkflowFailed)
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
    pub fn agent_loop_started(agent_name: String, iterations: Vec<Value>) -> Self {
        Self::new(ExecutorEventKind::AgentLoopStarted)
            .with_agent_name(agent_name)
            .with_data(serde_json::json!({
                "iteration_count": iterations.len(),
                "iterations": iterations,
            }))
    }

    #[must_use]
    pub fn agent_loop_completed(agent_name: String, output: Value, duration: Duration, iteration_count: usize) -> Self {
        Self::new(ExecutorEventKind::AgentLoopCompleted)
            .with_agent_name(agent_name)
            .with_data(serde_json::json!({
                "output": output,
                "duration_ms": duration_ms(duration),
                "iteration_count": iteration_count,
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
    pub fn context_compaction_completed(agent_name: String, output: Value, duration: Duration) -> Self {
        Self::new(ExecutorEventKind::ContextCompactionCompleted)
            .with_agent_name(agent_name)
            .with_data(serde_json::json!({
                "output": output,
                "duration_ms": duration_ms(duration),
            }))
    }

    #[must_use]
    pub fn context_compaction_failed(agent_name: String, message: String, duration: Duration) -> Self {
        Self::new(ExecutorEventKind::ContextCompactionFailed)
            .with_agent_name(agent_name)
            .with_message(message)
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
    pub fn agent_completed(agent_name: String, output: Value, duration: Duration, iteration_index: Option<usize>, cache_hit: bool) -> Self {
        let mut event_data = serde_json::Map::from_iter([
            ("output".to_string(), output),
            ("duration_ms".to_string(), serde_json::json!(duration_ms(duration))),
            ("cache_hit".to_string(), serde_json::json!(cache_hit)),
        ]);

        if let Some(iteration_index) = iteration_index {
            event_data.insert("iteration_index".to_string(), serde_json::json!(iteration_index));
        }

        Self::new(ExecutorEventKind::AgentCompleted)
            .with_agent_name(agent_name)
            .with_data(Value::Object(event_data))
    }

    #[must_use]
    pub fn tool_call_started(agent_name: String, tool_name: String, arguments: Value) -> Self {
        Self::new(ExecutorEventKind::ToolCallStarted)
            .with_agent_name(agent_name)
            .with_data(serde_json::json!({
                "tool_name": tool_name,
                "arguments": arguments,
            }))
    }

    #[must_use]
    pub fn tool_call_completed(agent_name: String, tool_name: String, result: Value, duration: Duration) -> Self {
        Self::new(ExecutorEventKind::ToolCallCompleted)
            .with_agent_name(agent_name)
            .with_data(serde_json::json!({
                "tool_name": tool_name,
                "result": result,
                "duration_ms": duration_ms(duration),
            }))
    }

    #[must_use]
    pub fn tool_call_failed(agent_name: String, tool_name: String, error: Value, duration: Duration) -> Self {
        Self::new(ExecutorEventKind::ToolCallFailed)
            .with_agent_name(agent_name)
            .with_data(serde_json::json!({
                "tool_name": tool_name,
                "error": error,
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
    pub fn mcp_tool_schema_fetch_failed(server_name: String, error: Value, duration: Duration) -> Self {
        Self::new(ExecutorEventKind::McpToolSchemaFetchFailed).with_data(serde_json::json!({
            "server_name": server_name,
            "error": error,
            "duration_ms": duration_ms(duration),
        }))
    }

    #[must_use]
    pub fn mcp_tool_validation_started(agent_name: String, tool_name: String, arguments: Value, input_schema: Value) -> Self {
        let legacy_parameters = arguments.clone();
        let event_data = Value::Object(serde_json::Map::from_iter([
            ("tool_name".to_string(), Value::String(tool_name)),
            ("arguments".to_string(), arguments),
            ("params".to_string(), legacy_parameters),
            ("input_schema".to_string(), input_schema),
        ]));

        Self::new(ExecutorEventKind::McpToolValidationStarted)
            .with_agent_name(agent_name)
            .with_data(event_data)
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
    pub fn mcp_tool_validation_failed(agent_name: String, tool_name: String, error: Value, duration: Duration) -> Self {
        Self::new(ExecutorEventKind::McpToolValidationFailed)
            .with_agent_name(agent_name)
            .with_data(serde_json::json!({
                "tool_name": tool_name,
                "error": error,
                "duration_ms": duration_ms(duration),
            }))
    }

    #[must_use]
    pub fn mcp_call_started(details: McpCallEventDetails) -> Self {
        Self::new(ExecutorEventKind::McpCallStarted).with_data(Value::Object(details.into_event_data()))
    }

    #[must_use]
    pub fn mcp_call_completed(details: McpCallEventDetails, result: Value, raw_result: Value, duration: Duration) -> Self {
        let mut event_data = details.into_event_data();
        event_data.insert("result".to_string(), result);
        event_data.insert("output".to_string(), raw_result.clone());
        event_data.insert("raw_result".to_string(), raw_result);
        event_data.insert("duration_ms".to_string(), serde_json::json!(duration_ms(duration)));

        Self::new(ExecutorEventKind::McpCallCompleted).with_data(Value::Object(event_data))
    }

    #[must_use]
    pub fn mcp_call_failed(details: McpCallEventDetails, error: Value, duration: Duration) -> Self {
        let mut event_data = details.into_event_data();
        event_data.insert("error".to_string(), error);
        event_data.insert("duration_ms".to_string(), serde_json::json!(duration_ms(duration)));

        Self::new(ExecutorEventKind::McpCallFailed).with_data(Value::Object(event_data))
    }

    #[must_use]
    pub fn workflow_completed(output: Value, duration: Duration) -> Self {
        Self::new(ExecutorEventKind::WorkflowCompleted).with_data(serde_json::json!({
            "output": output,
            "duration_ms": duration_ms(duration),
        }))
    }

    #[must_use]
    pub fn workflow_failed(message: String, duration: Option<Duration>) -> Self {
        let event = Self::new(ExecutorEventKind::WorkflowFailed).with_message(message);

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
            data: None,
        }
    }

    #[must_use]
    pub fn is_terminal(&self) -> bool {
        self.kind.is_terminal()
    }

    #[must_use]
    pub fn stream_deduplication_key(&self) -> String {
        let stable_data = self.data.as_ref().map(Self::stable_stream_data);

        serde_json::to_string(&(&self.kind, &self.agent_name, &self.message, &stable_data))
            .expect("executor event stream deduplication key should serialize")
    }

    #[must_use]
    fn stable_stream_data(data: &Value) -> Value {
        let Value::Object(object) = data else {
            return data.clone();
        };

        let mut stable_object = object.clone();
        stable_object.remove(EVENT_DURATION_FIELD_NAME);

        Value::Object(stable_object)
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

    fn with_data(mut self, data: Value) -> Self {
        self.data = Some(data);
        self
    }
}

fn duration_ms(duration: Duration) -> u64 {
    duration.as_millis().try_into().unwrap_or(u64::MAX)
}
