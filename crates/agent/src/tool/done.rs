use super::error::ToolError;
use super::traits::Tool;
use crate::traits::Validator;
use async_trait::async_trait;
use serde::Serialize;
use std::marker::PhantomData;
use std::sync::Arc;

pub struct DoneTool<V, O>
where
    V: Validator<Output = O>,
{
    validator: Arc<V>,
    phantom: PhantomData<O>,
}

impl<V, O> DoneTool<V, O>
where
    V: Validator<Output = O>,
{
    pub fn new(validator: Arc<V>) -> Self {
        Self {
            validator,
            phantom: PhantomData,
        }
    }

    pub async fn validate_output(&self, output: O) -> Result<serde_json::Value, ToolError>
    where
        O: Serialize,
    {
        self.validator.validate(&output).await.map_err(ToolError::from)?;

        serde_json::to_value(&output).map_err(|error| {
            ToolError::new(format!("Failed to serialize output: {error}"))
                .with_suggestion("Ensure the output type implements Serialize correctly".to_string())
        })
    }
}

impl<V, O> Clone for DoneTool<V, O>
where
    V: Validator<Output = O>,
{
    fn clone(&self) -> Self {
        Self {
            validator: Arc::clone(&self.validator),
            phantom: PhantomData,
        }
    }
}

#[async_trait]
impl<V, O> Tool for DoneTool<V, O>
where
    V: Validator<Output = O> + Send + Sync,
    O: Send + Sync + serde::de::DeserializeOwned + Serialize + schemars::JsonSchema,
{
    type Input = O;

    fn name(&self) -> &'static str {
        "done"
    }

    fn description(&self) -> &'static str {
        "Call this tool when you have completed the task. The output will be validated and you will receive feedback if validation fails."
    }

    async fn execute(&self, input: Self::Input) -> Result<serde_json::Value, ToolError> {
        self.validate_output(input).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ValidationError;
    use serde_json::json;

    struct MockValidator {
        should_pass: bool,
    }

    #[async_trait::async_trait]
    impl Validator for MockValidator {
        type Output = String;

        async fn validate(&self, _output: &Self::Output) -> Result<(), ValidationError> {
            if self.should_pass {
                Ok(())
            } else {
                Err(ValidationError::new("Validation failed".to_string())
                    .with_detail("reason".to_string(), json!("test failure")))
            }
        }
    }

    #[tokio::test]
    async fn test_done_tool_success() {
        let validator = Arc::new(MockValidator { should_pass: true });
        let done_tool = DoneTool::new(validator);

        let result = done_tool.execute("test output".to_string()).await;

        assert!(result.is_ok());
        let value = result.unwrap();
        assert_eq!(value, serde_json::Value::String("test output".to_string()));
    }

    #[tokio::test]
    async fn test_done_tool_validation_failure() {
        let validator = Arc::new(MockValidator { should_pass: false });
        let done_tool = DoneTool::new(validator);

        let result = done_tool.execute("test output".to_string()).await;

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert_eq!(error.error, "Validation failed");
        assert_eq!(error.get_context("reason"), Some(&json!("test failure")));
    }

    #[test]
    fn test_done_tool_metadata() {
        let validator = Arc::new(MockValidator { should_pass: true });
        let done_tool = DoneTool::new(validator);

        assert_eq!(done_tool.name(), "done");
        assert!(done_tool.description().contains("completed"));
    }
}
