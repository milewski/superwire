use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExecutorEventKind {
    WorkflowStarted,
    WorkflowPlanned,
    AgentStarted,
    AgentCompleted,
    ToolCallStarted,
    ToolCallCompleted,
    WorkflowCompleted,
    WorkflowFailed,
}

impl ExecutorEventKind {
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::WorkflowStarted => "workflow_started",
            Self::WorkflowPlanned => "workflow_planned",
            Self::AgentStarted => "agent_started",
            Self::AgentCompleted => "agent_completed",
            Self::ToolCallStarted => "tool_call_started",
            Self::ToolCallCompleted => "tool_call_completed",
            Self::WorkflowCompleted => "workflow_completed",
            Self::WorkflowFailed => "workflow_failed",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExecutorEvent {
    pub kind: ExecutorEventKind,

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
    pub fn workflow_planned(agent_execution_order: Vec<String>) -> Self {
        Self::new(ExecutorEventKind::WorkflowPlanned).with_data(serde_json::json!({
            "agent_execution_order": agent_execution_order,
        }))
    }

    #[must_use]
    pub fn agent_started(agent_name: String, model_name: String, tool_names: Vec<String>) -> Self {
        Self::new(ExecutorEventKind::AgentStarted)
            .with_agent_name(agent_name)
            .with_data(serde_json::json!({
                "model": model_name,
                "tools": tool_names,
            }))
    }

    #[must_use]
    pub fn agent_completed(agent_name: String, output: Value) -> Self {
        Self::new(ExecutorEventKind::AgentCompleted)
            .with_agent_name(agent_name)
            .with_data(serde_json::json!({ "output": output }))
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
    pub fn tool_call_completed(agent_name: String, tool_name: String, result: Value) -> Self {
        Self::new(ExecutorEventKind::ToolCallCompleted)
            .with_agent_name(agent_name)
            .with_data(serde_json::json!({
                "tool_name": tool_name,
                "result": result,
            }))
    }

    #[must_use]
    pub fn workflow_completed(output: Value) -> Self {
        Self::new(ExecutorEventKind::WorkflowCompleted).with_data(serde_json::json!({ "output": output }))
    }

    #[must_use]
    pub fn workflow_failed(message: String) -> Self {
        Self::new(ExecutorEventKind::WorkflowFailed).with_message(message)
    }

    fn new(kind: ExecutorEventKind) -> Self {
        Self {
            kind,
            agent_name: None,
            message: None,
            data: None,
        }
    }

    fn with_agent_name(mut self, agent_name: String) -> Self {
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
