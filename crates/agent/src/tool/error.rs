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
    pub fn new(error: String) -> Self {
        Self {
            error,
            suggestions: Vec::new(),
            context: std::collections::HashMap::new(),
        }
    }

    #[must_use]
    pub fn with_suggestion(mut self, suggestion: String) -> Self {
        self.suggestions.push(suggestion);
        self
    }

    #[must_use]
    pub fn with_context(mut self, key: String, value: serde_json::Value) -> Self {
        self.context.insert(key, value);
        self
    }

    #[must_use]
    pub fn get_context(&self, key: &str) -> Option<&serde_json::Value> {
        self.context.get(key)
    }

    /// Format the error as a message suitable for the AI agent
    #[must_use]
    pub fn to_agent_message(&self) -> String {
        let mut message = format!("Error: {}", self.error);

        if !self.suggestions.is_empty() {
            message.push_str("\n\nSuggestions:");
            for suggestion in &self.suggestions {
                message.push_str(&format!("\n- {}", suggestion));
            }
        }

        if !self.context.is_empty() {
            message.push_str("\n\nContext:");
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

impl From<crate::error::ValidationError> for ToolError {
    fn from(error: crate::error::ValidationError) -> Self {
        let mut tool_error = ToolError::new(error.message);
        for (key, value) in error.details {
            tool_error = tool_error.with_context(key, value);
        }
        tool_error
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_tool_error_creation() {
        let error = ToolError::new("Invalid input".to_string())
            .with_suggestion("Check the parameter format".to_string())
            .with_suggestion("Ensure all required fields are present".to_string())
            .with_context("expected_format".to_string(), json!("YYYY-MM-DD"))
            .with_context("received".to_string(), json!("2024-13-45"));

        assert_eq!(error.error, "Invalid input");
        assert_eq!(error.suggestions.len(), 2);
        assert_eq!(error.get_context("expected_format"), Some(&json!("YYYY-MM-DD")));
    }

    #[test]
    fn test_tool_error_to_agent_message() {
        let error = ToolError::new("File not found".to_string())
            .with_suggestion("Check if the file path is correct".to_string())
            .with_context("path".to_string(), json!("/tmp/missing.txt"));

        let message = error.to_agent_message();
        assert!(message.contains("Error: File not found"));
        assert!(message.contains("Suggestions:"));
        assert!(message.contains("Check if the file path is correct"));
        assert!(message.contains("Context:"));
        assert!(message.contains("path"));
    }

    #[test]
    fn test_validation_error_to_tool_error() {
        let validation_error = crate::error::ValidationError::new("Validation failed".to_string())
            .with_detail("field".to_string(), json!("username"));

        let tool_error: ToolError = validation_error.into();
        assert_eq!(tool_error.error, "Validation failed");
        assert_eq!(tool_error.get_context("field"), Some(&json!("username")));
    }
}
