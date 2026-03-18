use crate::error::ValidationError;
use crate::message::{Message, MessageRole, ToolCall, ToolResult};
use crate::traits::Tool;
use std::sync::Arc;

/// Context object that carries state throughout the agent execution
#[derive(Clone)]
pub struct Context<P, T>
where
    T: Tool + Clone,
{
    pub prompt: P,
    pub messages: Vec<Message>,
    pub tools: Vec<Arc<T>>,
    pub attempt: usize,
    pub total_tokens: usize,
    pub input_tokens: usize,
    pub output_tokens: usize,
}

impl<P, T> Context<P, T>
where
    T: Tool + Clone,
{
    pub fn new(prompt: P) -> Self {
        Self {
            prompt,
            messages: Vec::new(),
            tools: Vec::new(),
            attempt: 0,
            total_tokens: 0,
            input_tokens: 0,
            output_tokens: 0,
        }
    }

    #[must_use]
    pub fn with_tools(mut self, tools: Vec<Arc<T>>) -> Self {
        self.tools = tools;
        self
    }

    pub fn add_message(&mut self, message: Message) {
        self.messages.push(message);
    }

    pub fn add_user_message(&mut self, content: String) {
        self.add_message(Message::user(content));
    }

    pub fn add_tool_call(&mut self, tool_call: ToolCall) {
        self.add_message(Message::tool_call(tool_call));
    }

    pub fn add_tool_result(&mut self, result: ToolResult) {
        self.add_message(Message::tool_result(result));
    }

    pub fn add_assistant_message(&mut self, content: String) {
        self.add_message(Message::assistant(content));
    }

    pub fn add_system_message(&mut self, content: String) {
        self.add_message(Message::system(content));
    }

    pub fn add_validation_error(&mut self, error: ValidationError) {
        let error_message = serde_json::json!({
            "error": error.message,
            "details": error.details,
        });

        let mut message = Message::system(format!("Validation failed: {}", error.message));
        message.metadata.insert("validation_error".to_string(), error_message);

        self.add_message(message);
    }

    pub fn increment_attempt(&mut self) {
        self.attempt += 1;
    }

    pub fn add_tokens(&mut self, input_tokens: usize, output_tokens: usize) {
        self.input_tokens += input_tokens;
        self.output_tokens += output_tokens;
        self.total_tokens = self.input_tokens + self.output_tokens;
    }

    pub fn get_messages_by_role(&self, role: MessageRole) -> Vec<&Message> {
        self.messages.iter().filter(|m| m.role == role).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[derive(Clone)]
    struct MockTool {
        name: String,
    }

    #[async_trait::async_trait]
    impl Tool for MockTool {
        type Input = serde_json::Value;

        fn name(&self) -> &str {
            &self.name
        }

        fn description(&self) -> &str {
            "Mock tool"
        }

        async fn execute(&self, _input: Self::Input) -> Result<serde_json::Value, crate::ToolError> {
            Ok(serde_json::Value::String(format!("Result for {}", self.name)))
        }
    }

    #[test]
    fn test_context_message_management() {
        let mut context = Context::<String, MockTool>::new("input".to_string());

        assert_eq!(context.messages.len(), 0);
        assert_eq!(context.attempt, 0);

        context.add_user_message("Hello".to_string());
        assert_eq!(context.messages.len(), 1);
        assert!(matches!(context.messages[0].role, MessageRole::User));

        context.add_assistant_message("Hi".to_string());
        assert_eq!(context.messages.len(), 2);

        context.add_system_message("Notice".to_string());
        assert_eq!(context.messages.len(), 3);

        let user_messages = context.get_messages_by_role(MessageRole::User);
        assert_eq!(user_messages.len(), 1);
        assert_eq!(user_messages[0].content, "Hello");
    }

    #[test]
    fn test_context_token_tracking() {
        let context = Context::<String, MockTool>::new("input".to_string());

        assert_eq!(context.total_tokens, 0);
        assert_eq!(context.input_tokens, 0);
        assert_eq!(context.output_tokens, 0);

        let mut context = context;
        context.add_tokens(100, 50);
        assert_eq!(context.input_tokens, 100);
        assert_eq!(context.output_tokens, 50);
        assert_eq!(context.total_tokens, 150);

        context.add_tokens(200, 100);
        assert_eq!(context.input_tokens, 300);
        assert_eq!(context.output_tokens, 150);
        assert_eq!(context.total_tokens, 450);
    }

    #[test]
    fn test_context_validation_error() {
        let mut context = Context::<String, MockTool>::new("input".to_string());

        let error = ValidationError::new("Test error".to_string()).with_detail("field".to_string(), json!("invalid"));

        context.add_validation_error(error);

        assert_eq!(context.messages.len(), 1);
        assert!(matches!(context.messages[0].role, MessageRole::System));
        assert!(context.messages[0].content.contains("Validation failed"));
        assert!(context.messages[0].metadata.contains_key("validation_error"));
    }

    #[test]
    fn test_context_increment_attempt() {
        let mut context = Context::<String, MockTool>::new("input".to_string());

        assert_eq!(context.attempt, 0);

        context.increment_attempt();
        assert_eq!(context.attempt, 1);

        context.increment_attempt();
        assert_eq!(context.attempt, 2);
    }

    #[test]
    fn test_context_with_tools() {
        let tool1 = Arc::new(MockTool {
            name: "tool1".to_string(),
        });
        let tool2 = Arc::new(MockTool {
            name: "tool2".to_string(),
        });

        let context =
            Context::<String, MockTool>::new("input".to_string()).with_tools(vec![tool1.clone(), tool2.clone()]);

        assert_eq!(context.tools.len(), 2);
        assert_eq!(context.tools[0].name(), "tool1");
        assert_eq!(context.tools[1].name(), "tool2");
    }
}
