/// Tool execution error that provides context to help the AI agent fix mistakes
#[derive(Debug, Clone)]
pub struct ToolError {
    /// What went wrong
    pub error: String,
    /// Suggestions on how to fix the issue
    pub suggestions: Vec<String>,
    /// Additional context that might help
    pub context: std::collections::HashMap<String, serde_json::Value>,
}

impl ToolError {
    #[must_use]
    pub fn new(error: impl Into<String>) -> Self {
        Self {
            error: error.into(),
            suggestions: Vec::new(),
            context: std::collections::HashMap::new(),
        }
    }

    #[must_use]
    pub fn with_suggestion(mut self, suggestion: impl Into<String>) -> Self {
        self.suggestions.push(suggestion.into());
        self
    }

    #[must_use]
    pub fn with_context(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        self.context.insert(key.into(), value);
        self
    }

    #[must_use]
    pub fn get_context(&self, key: &str) -> Option<&serde_json::Value> {
        self.context.get(key)
    }

    /// Format the error as a message suitable for the AI agent
    #[must_use]
    pub fn to_agent_message(&self) -> String {
        let mut message = format!("Error: ({})", self.error);

        if !self.suggestions.is_empty() {
            message.push_str("\n\n");
            message.push_str("Suggestions:");

            for suggestion in &self.suggestions {
                message.push_str(&format!("\n- {}", suggestion));
            }
        }

        if !self.context.is_empty() {
            message.push_str("\n\n");
            message.push_str("Context:");

            for (key, value) in &self.context {
                message.push_str(&format!("\n- {}: {}", key, value));
            }
        }

        message
    }
}

impl std::fmt::Display for ToolError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}", self.to_agent_message())
    }
}

impl std::error::Error for ToolError {}
