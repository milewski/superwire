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
pub struct ToolResult {
    pub tool_call_id: String,
    pub content: Value,
    pub is_error: bool,
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
