use crate::tool::{RuntimeTool, ToolError};
use crate::traits::ToolDefinition;
use async_trait::async_trait;
use schemars::Schema;
use serde_json::Value;
use std::fmt::{Debug, Formatter};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

type DynamicToolFuture = Pin<Box<dyn Future<Output = Result<Value, ToolError>> + Send>>;
type DynamicToolExecutor = dyn Fn(Value) -> DynamicToolFuture + Send + Sync;

#[derive(Clone)]
pub struct DynamicTool {
    definition: ToolDefinition,
    execute: Arc<DynamicToolExecutor>,
}

impl DynamicTool {
    #[must_use]
    pub fn from_parts<ExecuteFunction, ExecuteFuture>(
        name: impl Into<String>,
        description: impl Into<String>,
        parameters_schema: Schema,
        execute_function: ExecuteFunction,
    ) -> Self
    where
        ExecuteFunction: Fn(Value) -> ExecuteFuture + Send + Sync + 'static,
        ExecuteFuture: Future<Output = Result<Value, ToolError>> + Send + 'static,
    {
        Self::new(
            ToolDefinition {
                name: name.into(),
                description: description.into(),
                parameters_schema,
            },
            execute_function,
        )
    }

    #[must_use]
    pub fn new<ExecuteFunction, ExecuteFuture>(definition: ToolDefinition, execute_function: ExecuteFunction) -> Self
    where
        ExecuteFunction: Fn(Value) -> ExecuteFuture + Send + Sync + 'static,
        ExecuteFuture: Future<Output = Result<Value, ToolError>> + Send + 'static,
    {
        let execute = Arc::new(move |input: Value| -> DynamicToolFuture { Box::pin(execute_function(input)) });

        Self { definition, execute }
    }

    #[must_use]
    pub fn tool_definition(&self) -> &ToolDefinition {
        &self.definition
    }
}

impl Debug for DynamicTool {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("DynamicTool")
            .field("name", &self.definition.name)
            .field("description", &self.definition.description)
            .finish_non_exhaustive()
    }
}

#[async_trait]
impl RuntimeTool for DynamicTool {
    fn definition(&self) -> Result<ToolDefinition, ToolError> {
        Ok(self.definition.clone())
    }

    async fn execute(&self, input: Value) -> Result<Value, ToolError> {
        (self.execute)(input).await
    }
}

#[cfg(test)]
mod tests {
    use super::DynamicTool;
    use crate::tool::RuntimeTool;
    use schemars::schema_for;
    use serde::Deserialize;
    use serde::Serialize;
    use serde_json::json;
    use serde_json::Value;

    #[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
    struct DynamicEchoInput {
        value: String,
    }

    #[tokio::test]
    async fn executes_dynamic_callback_and_returns_definition() {
        let dynamic_tool = DynamicTool::from_parts(
            "dynamic_echo",
            "Echoes a value from dynamic input",
            schema_for!(DynamicEchoInput),
            |input| async move {
                let echoed_value = input
                    .get("value")
                    .and_then(Value::as_str)
                    .expect("input should include a string value");

                Ok(json!({ "echo": echoed_value }))
            },
        );

        let definition = dynamic_tool.tool_definition();

        assert_eq!(definition.name, "dynamic_echo");
        assert_eq!(definition.description, "Echoes a value from dynamic input");

        let execution_result = dynamic_tool
            .execute(json!({ "value": "hello" }))
            .await
            .expect("dynamic tool should execute");

        assert_eq!(execution_result, json!({ "echo": "hello" }));
    }
}
