use serde_json::Value;

/// Tool call information
#[derive(Debug, Clone, PartialEq)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: Value,
}

/// Tool result information
#[derive(Debug, Clone, PartialEq)]
pub enum ToolResult {
    Success { tool_call_id: String, content: Value },
    Failure { tool_call_id: String, content: Value },
}

impl ToolResult {
    #[must_use]
    pub fn tool_call_id(&self) -> &str {
        match self {
            Self::Success { tool_call_id, content: _ } | Self::Failure { tool_call_id, content: _ } => tool_call_id,
        }
    }

    #[must_use]
    pub fn content(&self) -> &Value {
        match self {
            Self::Success { tool_call_id: _, content } | Self::Failure { tool_call_id: _, content } => content,
        }
    }
}

/// A message in the conversation
#[derive(Debug, Clone, PartialEq)]
pub enum Message {
    User { content: String },
    Assistant { content: String },
    AssistantToolCall { tool: ToolCall },
    ToolResult { result: ToolResult },
    System { content: String },
}

impl Message {
    #[must_use]
    pub fn user(content: impl Into<String>) -> Self {
        Self::User { content: content.into() }
    }

    #[must_use]
    pub fn assistant(content: impl Into<String>) -> Self {
        Self::Assistant { content: content.into() }
    }

    #[must_use]
    pub fn tool_call(tool_call: ToolCall) -> Self {
        Self::AssistantToolCall { tool: tool_call }
    }

    #[must_use]
    pub fn tool_result(tool_result: ToolResult) -> Self {
        Self::ToolResult { result: tool_result }
    }

    #[must_use]
    pub fn system(content: impl Into<String>) -> Self {
        Self::System { content: content.into() }
    }
}
