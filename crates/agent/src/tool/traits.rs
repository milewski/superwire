use crate::tool::ToolError;
use crate::traits::ToolDefinition;
use serde_json::Value;
use std::fmt::Debug;

/// Trait for tools that can be used by the agent
#[async_trait::async_trait]
pub trait Tool: Clone + Send + Sync {
    type Input: serde::de::DeserializeOwned + schemars::JsonSchema + Send;

    fn inferred_name() -> &'static str
    where
        Self: Sized,
    {
        let qualified_type_name = std::any::type_name::<Self>();

        let type_name_without_generic_arguments = qualified_type_name.split('<').next().unwrap_or(qualified_type_name);

        type_name_without_generic_arguments
            .rsplit("::")
            .next()
            .unwrap_or(type_name_without_generic_arguments)
    }

    fn name(&self) -> &str {
        Self::inferred_name()
    }

    fn description(&self) -> &str;

    fn parameters_schema() -> schemars::Schema {
        schemars::schema_for!(Self::Input)
    }

    async fn execute(&self, input: Self::Input) -> Result<Value, ToolError>;
}

#[async_trait::async_trait]
pub trait RuntimeTool: Send + Sync + Debug {
    fn definition(&self) -> Result<ToolDefinition, ToolError>;

    async fn execute(&self, input: Value) -> Result<Value, ToolError>;
}

#[async_trait::async_trait]
impl<T> RuntimeTool for T
where
    T: Tool + Send + Sync + Debug,
{
    fn definition(&self) -> Result<ToolDefinition, ToolError> {
        Ok(ToolDefinition {
            name: self.name().to_string(),
            description: self.description().to_string(),
            parameters_schema: T::parameters_schema(),
        })
    }

    async fn execute(&self, input: Value) -> Result<Value, ToolError> {
        let input = serde_json::from_value(input).map_err(|error| {
            ToolError::new(format!("Failed to deserialize tool input for '{}': {error}", self.name()))
                .with_suggestion("Check that the arguments match the expected schema")
        })?;

        self.execute(input).await
    }
}

#[cfg(test)]
mod tests {
    use super::Tool;
    use crate::tool::ToolError;
    use async_trait::async_trait;
    use schemars::JsonSchema;
    use serde::Deserialize;
    use serde::Serialize;
    use serde_json::Value;

    #[derive(Debug, Clone)]
    struct InferredNameTool;

    #[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
    struct InferredNameInput {
        value: String,
    }

    #[async_trait]
    impl Tool for InferredNameTool {
        type Input = InferredNameInput;

        fn description(&self) -> &str {
            "Test tool that uses inferred name"
        }

        async fn execute(&self, input: Self::Input) -> Result<Value, ToolError> {
            Ok(serde_json::json!({ "echo": input.value }))
        }
    }

    #[test]
    fn infers_tool_name_from_type_name() {
        let inferred_name_tool = InferredNameTool;

        assert_eq!(inferred_name_tool.name(), "InferredNameTool");
    }

    #[test]
    fn infers_name_from_generic_type_without_module_path() {
        type GenericInferredNameTool = crate::tool::FinalizeTool<Value>;

        assert_eq!(<GenericInferredNameTool as Tool>::inferred_name(), "FinalizeTool");
    }
}
