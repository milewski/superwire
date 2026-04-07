use crate::tool::{RuntimeTool, ToolError};
use crate::traits::ToolDefinition;
use async_trait::async_trait;
use schemars::Schema;
use serde_json::{Map, Value};
use std::fmt::{Debug, Formatter};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

type DynamicToolFuture = Pin<Box<dyn Future<Output = Result<Value, ToolError>> + Send>>;
type DynamicToolExecutor = dyn Fn(Value) -> DynamicToolFuture + Send + Sync;
type DynamicToolBoundArgumentsExecutor = dyn Fn(Value, Map<String, Value>) -> DynamicToolFuture + Send + Sync;

#[derive(Clone)]
pub struct DynamicTool {
    definition: ToolDefinition,
    execute: Arc<DynamicToolExecutor>,
    execute_with_bound_arguments: Option<Arc<DynamicToolBoundArgumentsExecutor>>,
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
                bound_parameters_schema: None,
                output_schema: None,
            },
            execute_function,
        )
    }

    #[must_use]
    pub fn from_parts_with_bound_arguments<ExecuteFunction, ExecuteFuture>(
        name: impl Into<String>,
        description: impl Into<String>,
        parameters_schema: Schema,
        execute_function: ExecuteFunction,
    ) -> Self
    where
        ExecuteFunction: Fn(Value, Map<String, Value>) -> ExecuteFuture + Send + Sync + 'static,
        ExecuteFuture: Future<Output = Result<Value, ToolError>> + Send + 'static,
    {
        Self::new_with_bound_arguments(
            ToolDefinition {
                name: name.into(),
                description: description.into(),
                parameters_schema,
                bound_parameters_schema: None,
                output_schema: None,
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

        Self {
            definition,
            execute,
            execute_with_bound_arguments: None,
        }
    }

    #[must_use]
    pub fn new_with_bound_arguments<ExecuteFunction, ExecuteFuture>(definition: ToolDefinition, execute_function: ExecuteFunction) -> Self
    where
        ExecuteFunction: Fn(Value, Map<String, Value>) -> ExecuteFuture + Send + Sync + 'static,
        ExecuteFuture: Future<Output = Result<Value, ToolError>> + Send + 'static,
    {
        let execute_with_bound_arguments = Arc::new(
            move |model_input: Value, bound_arguments: Map<String, Value>| -> DynamicToolFuture {
                Box::pin(execute_function(model_input, bound_arguments))
            },
        );

        let execute_with_bound_arguments_for_execute = Arc::clone(&execute_with_bound_arguments);
        let execute = Arc::new(move |model_input: Value| -> DynamicToolFuture {
            Box::pin((execute_with_bound_arguments_for_execute)(model_input, Map::new()))
        });

        Self {
            definition,
            execute,
            execute_with_bound_arguments: Some(execute_with_bound_arguments),
        }
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

    async fn execute_with_bound_arguments(&self, model_input: Value, bound_arguments: Map<String, Value>) -> Result<Value, ToolError> {
        if let Some(execute_with_bound_arguments) = &self.execute_with_bound_arguments {
            return (execute_with_bound_arguments)(model_input, bound_arguments).await;
        }

        let Some(model_input_fields) = model_input.as_object() else {
            let input_kind = match &model_input {
                Value::Null => "null",
                Value::Bool(_) => "boolean",
                Value::Number(_) => "number",
                Value::String(_) => "string",
                Value::Array(_) => "array",
                Value::Object(_) => "object",
            };

            return Err(ToolError::new(format!(
                "tool `{}` requires object arguments, but model sent {}",
                self.definition.name, input_kind
            )));
        };

        let mut merged_arguments = model_input_fields.clone();

        for (bound_argument_name, bound_argument_value) in bound_arguments {
            merged_arguments.insert(bound_argument_name, bound_argument_value);
        }

        (self.execute)(Value::Object(merged_arguments)).await
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

    #[tokio::test]
    async fn merges_bound_arguments_for_standard_dynamic_tool() {
        let dynamic_tool = DynamicTool::from_parts(
            "dynamic_bound_merge",
            "Merges bound arguments into model input",
            schema_for!(DynamicEchoInput),
            |input| async move { Ok(input) },
        );

        let execution_result = dynamic_tool
            .execute_with_bound_arguments(json!({ "value": "from-model", "model_only": true }), {
                let mut bound_arguments = serde_json::Map::new();
                bound_arguments.insert("value".to_string(), json!("from-bound"));
                bound_arguments.insert("bound_only".to_string(), json!(true));
                bound_arguments
            })
            .await
            .expect("dynamic tool should merge bound arguments");

        assert_eq!(
            execution_result,
            json!({
                "value": "from-bound",
                "model_only": true,
                "bound_only": true
            })
        );
    }

    #[tokio::test]
    async fn forwards_separate_inputs_when_bound_executor_is_defined() {
        let dynamic_tool = DynamicTool::from_parts_with_bound_arguments(
            "dynamic_split_inputs",
            "Receives model and bound inputs separately",
            schema_for!(DynamicEchoInput),
            |model_input, bound_arguments| async move {
                Ok(json!({
                    "model_input": model_input,
                    "bound_arguments": bound_arguments,
                }))
            },
        );

        let execution_result = dynamic_tool
            .execute_with_bound_arguments(json!({ "value": "from-model" }), {
                let mut bound_arguments = serde_json::Map::new();
                bound_arguments.insert("value".to_string(), json!("from-bound"));
                bound_arguments
            })
            .await
            .expect("dynamic tool should receive split inputs");

        assert_eq!(
            execution_result,
            json!({
                "model_input": {
                    "value": "from-model"
                },
                "bound_arguments": {
                    "value": "from-bound"
                }
            })
        );
    }
}
