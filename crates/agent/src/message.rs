use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

/// Tool call information
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: Value,
}

/// Tool result information
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
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
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub enum Message {
    User { id: String, content: String },
    Assistant { id: String, content: String },
    AssistantToolCall { id: String, tool: ToolCall },
    ToolResult { id: String, result: ToolResult },
    System { id: String, content: String },
}

impl Message {
    fn generated_message_id() -> String {
        format!("message_{}", Uuid::new_v4())
    }

    #[must_use]
    pub fn id(&self) -> &str {
        match self {
            Self::User { id, content: _ }
            | Self::Assistant { id, content: _ }
            | Self::AssistantToolCall { id, tool: _ }
            | Self::ToolResult { id, result: _ }
            | Self::System { id, content: _ } => id,
        }
    }

    #[must_use]
    pub fn has_same_content_as(&self, other_message: &Self) -> bool {
        match (self, other_message) {
            (
                Self::User {
                    id: _,
                    content: left_content,
                },
                Self::User {
                    id: _,
                    content: right_content,
                },
            ) => left_content == right_content,

            (
                Self::Assistant {
                    id: _,
                    content: left_content,
                },
                Self::Assistant {
                    id: _,
                    content: right_content,
                },
            ) => left_content == right_content,

            (
                Self::AssistantToolCall {
                    id: _,
                    tool: left_tool_call,
                },
                Self::AssistantToolCall {
                    id: _,
                    tool: right_tool_call,
                },
            ) => left_tool_call == right_tool_call,

            (
                Self::ToolResult {
                    id: _,
                    result: left_tool_result,
                },
                Self::ToolResult {
                    id: _,
                    result: right_tool_result,
                },
            ) => left_tool_result == right_tool_result,

            (
                Self::System {
                    id: _,
                    content: left_content,
                },
                Self::System {
                    id: _,
                    content: right_content,
                },
            ) => left_content == right_content,

            _ => false,
        }
    }

    #[must_use]
    pub fn user(content: impl Into<String>) -> Self {
        Self::user_with_id(Self::generated_message_id(), content)
    }

    #[must_use]
    pub fn user_with_id(id: impl Into<String>, content: impl Into<String>) -> Self {
        Self::User {
            id: id.into(),
            content: content.into(),
        }
    }

    #[must_use]
    pub fn assistant(content: impl Into<String>) -> Self {
        Self::assistant_with_id(Self::generated_message_id(), content)
    }

    #[must_use]
    pub fn assistant_with_id(id: impl Into<String>, content: impl Into<String>) -> Self {
        Self::Assistant {
            id: id.into(),
            content: content.into(),
        }
    }

    #[must_use]
    pub fn tool_call(tool_call: ToolCall) -> Self {
        let message_id = format!("tool_call_{}", tool_call.id);

        Self::tool_call_with_id(message_id, tool_call)
    }

    #[must_use]
    pub fn tool_call_with_id(id: impl Into<String>, tool_call: ToolCall) -> Self {
        Self::AssistantToolCall {
            id: id.into(),
            tool: tool_call,
        }
    }

    #[must_use]
    pub fn tool_result(tool_result: ToolResult) -> Self {
        Self::tool_result_with_id(Self::generated_message_id(), tool_result)
    }

    #[must_use]
    pub fn tool_result_with_id(id: impl Into<String>, tool_result: ToolResult) -> Self {
        Self::ToolResult {
            id: id.into(),
            result: tool_result,
        }
    }

    #[must_use]
    pub fn system(content: impl Into<String>) -> Self {
        Self::system_with_id(Self::generated_message_id(), content)
    }

    #[must_use]
    pub fn system_with_id(id: impl Into<String>, content: impl Into<String>) -> Self {
        Self::System {
            id: id.into(),
            content: content.into(),
        }
    }
}
