use crate::error::ValidationError;
use crate::message::{Message, MessageRole, ToolCall, ToolResult};

/// Context object that carries state throughout the agent execution
#[derive(Clone, Default)]
pub struct Context {
    pub messages: Vec<Message>,
    pub attempt: usize,
    pub total_tokens: usize,
    pub input_tokens: usize,
    pub output_tokens: usize,
}

impl Context {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_message(&mut self, message: Message) {
        self.messages.push(message);
    }

    pub fn add_user_message(&mut self, content: impl Into<String>) {
        self.add_message(Message::user(content));
    }

    pub fn add_tool_call(&mut self, tool_call: ToolCall) {
        self.add_message(Message::tool_call(tool_call));
    }

    pub fn add_tool_result(&mut self, result: ToolResult) {
        self.add_message(Message::tool_result(result));
    }

    pub fn add_assistant_message(&mut self, content: impl Into<String>) {
        self.add_message(Message::assistant(content));
    }

    pub fn add_system_message(&mut self, content: impl Into<String>) {
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
