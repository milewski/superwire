use crate::message::{Message, ToolCall, ToolResult};

/// Context object that carries state throughout the agent execution
#[derive(Debug, Clone, Default)]
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

    pub fn increment_attempt(&mut self) {
        self.attempt += 1;
    }

    /// Detects if the agent is stuck by checking if the last `window` messages are all identical
    pub fn is_stuck(&self, window: usize) -> bool {
        if self.messages.len() < window {
            return false;
        }

        let start = self.messages.len() - window;
        let recent = &self.messages[start..];

        recent.iter().all(|message| message == &recent[0])
    }
}
