use serde_json::Value;
use std::collections::HashMap;

/// Message role in the conversation
#[derive(Debug, Clone, PartialEq)]
pub enum MessageRole {
    User,
    Assistant,
    Tool,
    ToolResult,
    System,
}

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
pub struct Message {
    pub role: MessageRole,
    pub content: String,
    pub tool_call: Option<ToolCall>,
    pub tool_result: Option<ToolResult>,
    pub metadata: HashMap<String, Value>,
}

impl Message {
    #[must_use]
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: MessageRole::User,
            content: content.into(),
            tool_call: None,
            tool_result: None,
            metadata: HashMap::new(),
        }
    }

    #[must_use]
    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: MessageRole::Assistant,
            content: content.into(),
            tool_call: None,
            tool_result: None,
            metadata: HashMap::new(),
        }
    }

    #[must_use]
    pub fn tool_call(tool_call: ToolCall) -> Self {
        Self {
            role: MessageRole::Assistant,
            content: String::new(),
            tool_call: Some(tool_call),
            tool_result: None,
            metadata: HashMap::new(),
        }
    }

    #[must_use]
    pub fn tool_result(tool_result: ToolResult) -> Self {
        let content = serde_json::to_string(&tool_result.content).unwrap_or_else(|_| tool_result.content.to_string());

        Self {
            role: MessageRole::ToolResult,
            content,
            tool_call: None,
            tool_result: Some(tool_result),
            metadata: HashMap::new(),
        }
    }

    #[must_use]
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: MessageRole::System,
            content: content.into(),
            tool_call: None,
            tool_result: None,
            metadata: HashMap::new(),
        }
    }

    #[must_use]
    pub fn with_metadata(mut self, key: impl Into<String>, value: Value) -> Self {
        self.metadata.insert(key.into(), value);
        self
    }
}
