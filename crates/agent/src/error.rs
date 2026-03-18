/// Validation error containing details about why validation failed
#[derive(Debug, Clone)]
pub struct ValidationError {
    pub message: String,
    pub details: std::collections::HashMap<String, serde_json::Value>,
}

impl ValidationError {
    #[must_use]
    pub fn new(message: String) -> Self {
        Self {
            message,
            details: std::collections::HashMap::new(),
        }
    }

    #[must_use]
    pub fn with_detail(mut self, key: String, value: serde_json::Value) -> Self {
        self.details.insert(key, value);
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_validation_error_creation() {
        let error = ValidationError::new("Test error".to_string())
            .with_detail("field".to_string(), json!("value"))
            .with_detail("code".to_string(), json!(42));

        assert_eq!(error.message, "Test error");
        assert_eq!(error.get_detail("field"), Some(&json!("value")));
        assert_eq!(error.get_detail("code"), Some(&json!(42)));
        assert_eq!(error.get_detail("missing"), None);
    }

    #[test]
    fn test_validation_error_display() {
        let error = ValidationError::new("Display test".to_string());
        assert_eq!(format!("{}", error), "Display test");
    }
}
