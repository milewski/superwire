/// Validation error containing details about why validation failed
#[derive(Debug, Clone)]
pub struct ValidationError {
    pub message: String,
    pub details: std::collections::HashMap<String, serde_json::Value>,
}

impl ValidationError {
    #[must_use]
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            details: std::collections::HashMap::new(),
        }
    }

    #[must_use]
    pub fn with_detail(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        self.details.insert(key.into(), value);
        self
    }

    #[must_use]
    pub fn get_detail(&self, key: &str) -> Option<&serde_json::Value> {
        self.details.get(key)
    }
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}", self.message)
    }
}

impl std::error::Error for ValidationError {}

/// Agent execution error
#[derive(Debug, Clone)]
pub enum AgentError {
    MaxRetriesExceeded { max_retries: usize },
    MaxTokensExceeded { max_tokens: usize, used_tokens: usize },
    ValidationFailed { error: ValidationError },
    ExecutionFailed { message: String },
}

/// Executor error with structured details
#[derive(Debug, Clone)]
pub struct ExecutorError {
    pub error: String,
    pub details: std::collections::HashMap<String, serde_json::Value>,
}

impl ExecutorError {
    #[must_use]
    pub fn new(error: impl Into<String>) -> Self {
        Self {
            error: error.into(),
            details: std::collections::HashMap::new(),
        }
    }

    #[must_use]
    pub fn with_detail(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        self.details.insert(key.into(), value);
        self
    }
}

impl std::fmt::Display for ExecutorError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}", self.error)
    }
}

impl std::error::Error for ExecutorError {}

impl From<crate::tool::ToolError> for ExecutorError {
    fn from(error: crate::tool::ToolError) -> Self {
        Self {
            error: error.error,
            details: error.context,
        }
    }
}

impl From<ExecutorError> for AgentError {
    fn from(error: ExecutorError) -> Self {
        Self::ExecutionFailed {
            message: error.to_string(),
        }
    }
}

impl From<crate::tool::ToolError> for AgentError {
    fn from(error: crate::tool::ToolError) -> Self {
        Self::ExecutionFailed {
            message: error.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tool::ToolError;
    use serde_json::json;

    #[test]
    fn test_validation_error_creation() {
        let error = ValidationError::new("Test error")
            .with_detail("field", json!("value"))
            .with_detail("code", json!(42));

        assert_eq!(error.message, "Test error");
        assert_eq!(error.get_detail("field"), Some(&json!("value")));
        assert_eq!(error.get_detail("code"), Some(&json!(42)));
        assert_eq!(error.get_detail("missing"), None);
    }

    #[test]
    fn test_validation_error_display() {
        let error = ValidationError::new("Display test");
        assert_eq!(format!("{}", error), "Display test");
    }

    #[test]
    fn test_tool_error_converts_to_execution_failed_agent_error() {
        let agent_error: AgentError = ToolError::new("Tool failed").into();

        match agent_error {
            AgentError::ExecutionFailed { message } => assert_eq!(message, "Tool failed"),
            _ => panic!("expected execution failed error"),
        }
    }
}
