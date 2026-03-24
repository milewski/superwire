use crate::error::WorkflowError;
use engine_ai_agent::{RuntimeTool, ToolDefinition, ToolError};
use schemars::Schema;
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::sync::Arc;

#[derive(Debug)]
pub(crate) struct BoundRuntimeTool {
    bound_arguments: BTreeMap<String, Value>,
    tool_name: String,
    wrapped_tool: Arc<dyn RuntimeTool>,
}

impl BoundRuntimeTool {
    pub(crate) fn new(tool_name: String, wrapped_tool: Arc<dyn RuntimeTool>, bound_arguments: BTreeMap<String, Value>) -> Self {
        Self {
            bound_arguments,
            tool_name,
            wrapped_tool,
        }
    }
}

#[async_trait::async_trait]
impl RuntimeTool for BoundRuntimeTool {
    fn definition(&self) -> Result<ToolDefinition, ToolError> {
        let mut definition = self.wrapped_tool.definition()?;
        definition.name.clone_from(&self.tool_name);
        definition.parameters_schema = prune_bound_arguments(&definition.parameters_schema, self.bound_arguments.keys())
            .map_err(|error| ToolError::new(error.to_string()))?;

        Ok(definition)
    }

    async fn execute(&self, input: Value) -> Result<Value, ToolError> {
        let mut merged_arguments = match input {
            Value::Object(argument_map) => argument_map,
            other_value => {
                return self.wrapped_tool.execute(other_value).await;
            }
        };

        for (argument_name, argument_value) in &self.bound_arguments {
            merged_arguments.insert(argument_name.clone(), argument_value.clone());
        }

        self.wrapped_tool.execute(Value::Object(merged_arguments)).await
    }
}

fn prune_bound_arguments<'a>(schema: &Schema, bound_argument_names: impl Iterator<Item = &'a String>) -> Result<Schema, WorkflowError> {
    let mut schema_value =
        serde_json::to_value(schema).map_err(|error| WorkflowError::schema(format!("failed to serialize tool schema: {error}")))?;
    let bound_argument_names = bound_argument_names.cloned().collect::<Vec<_>>();

    if let Some(schema_object) = schema_value.as_object_mut() {
        remove_bound_properties(schema_object, &bound_argument_names);
    }

    serde_json::from_value(schema_value).map_err(|error| WorkflowError::schema(format!("failed to deserialize tool schema: {error}")))
}

fn remove_bound_properties(schema_object: &mut Map<String, Value>, bound_argument_names: &[String]) {
    if let Some(properties) = schema_object.get_mut("properties").and_then(Value::as_object_mut) {
        for bound_argument_name in bound_argument_names {
            properties.remove(bound_argument_name);
        }
    }

    if let Some(required_properties) = schema_object.get_mut("required").and_then(Value::as_array_mut) {
        required_properties.retain(|required_value| {
            required_value
                .as_str()
                .is_some_and(|required_name| !bound_argument_names.iter().any(|bound_name| bound_name == required_name))
        });
    }
}

#[cfg(test)]
mod tests {
    use super::BoundRuntimeTool;
    use engine_ai_agent::{RuntimeTool, Tool, ToolError};
    use serde::{Deserialize, Serialize};
    use serde_json::{json, Value};
    use std::collections::BTreeMap;
    use std::sync::Arc;

    #[derive(Debug, Clone, Default)]
    struct EchoArgumentsTool;

    #[derive(Debug, Clone, Deserialize, Serialize, schemars::JsonSchema)]
    struct EchoArgumentsInput {
        hidden: String,
        shown: String,
    }

    #[async_trait::async_trait]
    impl Tool for EchoArgumentsTool {
        type Input = EchoArgumentsInput;

        fn name(&self) -> &str {
            "echo_arguments"
        }

        fn description(&self) -> &str {
            "Echoes all provided arguments"
        }

        async fn execute(&self, input: Self::Input) -> Result<Value, ToolError> {
            Ok(json!({
                "hidden": input.hidden,
                "shown": input.shown,
            }))
        }
    }

    #[tokio::test]
    async fn injects_bound_arguments_when_executing_wrapped_tools() {
        let wrapped_tool = BoundRuntimeTool::new(
            "echo_arguments".to_string(),
            Arc::new(EchoArgumentsTool),
            BTreeMap::from([("hidden".to_string(), json!("secret"))]),
        );
        let value = wrapped_tool
            .execute(json!({ "shown": "visible" }))
            .await
            .expect("wrapped tool should execute");

        assert_eq!(value, json!({ "hidden": "secret", "shown": "visible" }));
    }

    #[test]
    fn removes_bound_arguments_from_tool_definition_schema() {
        let wrapped_tool = BoundRuntimeTool::new(
            "echo_arguments".to_string(),
            Arc::new(EchoArgumentsTool),
            BTreeMap::from([("hidden".to_string(), json!("secret"))]),
        );
        let definition = wrapped_tool.definition().expect("tool definition should build");
        let schema_value = serde_json::to_value(definition.parameters_schema).expect("tool schema should serialize");
        let properties = schema_value["properties"]
            .as_object()
            .expect("tool schema should expose properties");
        let required = schema_value["required"]
            .as_array()
            .expect("tool schema should expose required keys");

        assert!(!properties.contains_key("hidden"));
        assert!(properties.contains_key("shown"));
        assert!(required.iter().all(|value| value.as_str() != Some("hidden")));
    }
}
