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
#[derive(Debug, Clone)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

/// Tool result information
#[derive(Debug, Clone)]
pub struct ToolResult {
    pub tool_call_id: String,
    pub content: serde_json::Value,
    pub is_error: bool,
}

/// A message in the conversation
#[derive(Debug, Clone)]
pub struct Message {
    pub role: MessageRole,
    pub content: String,
    pub tool_call: Option<ToolCall>,
    pub tool_result: Option<ToolResult>,
    pub metadata: std::collections::HashMap<String, serde_json::Value>,
}

impl Message {
    #[must_use]
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: MessageRole::User,
            content: content.into(),
            tool_call: None,
            tool_result: None,
            metadata: std::collections::HashMap::new(),
        }
    }

    #[must_use]
    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: MessageRole::Assistant,
            content: content.into(),
            tool_call: None,
            tool_result: None,
            metadata: std::collections::HashMap::new(),
        }
    }

    #[must_use]
    pub fn tool_call(tool_call: ToolCall) -> Self {
        Self {
            role: MessageRole::Assistant,
            content: String::new(),
            tool_call: Some(tool_call),
            tool_result: None,
            metadata: std::collections::HashMap::new(),
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
            metadata: std::collections::HashMap::new(),
        }
    }

    #[must_use]
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: MessageRole::System,
            content: content.into(),
            tool_call: None,
            tool_result: None,
            metadata: std::collections::HashMap::new(),
        }
    }

    #[must_use]
    pub fn with_metadata(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        self.metadata.insert(key.into(), value);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_message_constructors() {
        let user_msg = Message::user("Hello");
        assert!(matches!(user_msg.role, MessageRole::User));
        assert_eq!(user_msg.content, "Hello");
        assert!(user_msg.tool_call.is_none());
        assert!(user_msg.tool_result.is_none());

        let assistant_msg = Message::assistant("Hi there");
        assert!(matches!(assistant_msg.role, MessageRole::Assistant));
        assert_eq!(assistant_msg.content, "Hi there");

        let tool_call = ToolCall {
            id: "call_123".to_string(),
            name: "search".to_string(),
            arguments: json!({"query": "test"}),
        };
        let tool_msg = Message::tool_call(tool_call.clone());
        assert!(matches!(tool_msg.role, MessageRole::Tool));
        assert!(tool_msg.tool_call.is_some());
        assert_eq!(tool_msg.tool_call.unwrap().id, "call_123");

        let tool_result = ToolResult {
            tool_call_id: "call_123".to_string(),
            content: serde_json::Value::String("Result data".to_string()),
            is_error: false,
        };
        let result_msg = Message::tool_result(tool_result.clone());
        assert!(matches!(result_msg.role, MessageRole::ToolResult));
        assert!(result_msg.tool_result.is_some());
        assert!(result_msg.content.contains("Result data"));

        let system_msg = Message::system("System notice");
        assert!(matches!(system_msg.role, MessageRole::System));
        assert_eq!(system_msg.content, "System notice");
    }

    #[test]
    fn test_message_with_metadata() {
        let message = Message::user("Test")
            .with_metadata("key1", json!("value1"))
            .with_metadata("key2", json!(123));

        assert_eq!(message.metadata.get("key1"), Some(&json!("value1")));
        assert_eq!(message.metadata.get("key2"), Some(&json!(123)));
    }
}
