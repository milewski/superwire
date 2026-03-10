/// Helper macro to implement the Tool trait with automatic schema generation
///
/// Usage with parameters:
/// ```
/// use engine_ai_core::impl_tool;
/// use schemars::JsonSchema;
/// use serde::{Deserialize, Serialize};
///
/// #[derive(Serialize, Deserialize, JsonSchema)]
/// struct MyToolParams {
///     field: String,
/// }
///
/// struct MyTool;
///
/// impl_tool!(MyTool, MyToolParams, {
///     name: "my_tool",
///     description: "Does something useful",
///     execute: |params| {
///         // implementation
///         Ok(serde_json::json!({"result": params.field}))
///     }
/// });
/// ```
///
/// Usage without parameters:
/// ```
/// use engine_ai_core::impl_tool;
///
/// struct MyTool;
///
/// impl_tool!(MyTool, {
///     name: "my_tool",
///     description: "Does something useful",
///     execute: || {
///         // implementation
///         Ok(serde_json::json!({"result": "done"}))
///     }
/// });
/// ```
///
/// Usage with custom schema:
/// ```
/// use engine_ai_core::impl_tool;
/// use schemars::JsonSchema;
/// use serde::{Deserialize, Serialize};
///
/// #[derive(Serialize, Deserialize, JsonSchema)]
/// struct MyToolParams {
///     field: String,
/// }
///
/// struct MyTool {
///     custom_field: String,
/// }
///
/// impl_tool!(MyTool, MyToolParams, {
///     name: "my_tool",
///     description: "Does something useful",
///     schema: |self| {
///         // custom schema generation
///         let base = schemars::schema_for!(MyToolParams);
///         // modify base...
///         serde_json::to_value(base).unwrap()
///     },
///     execute: |params| {
///         // implementation
///         Ok(serde_json::json!({"result": params.field}))
///     }
/// });
/// ```
#[macro_export]
macro_rules! impl_tool {
    // With parameters
    ($tool_type:ty, $params_type:ty, {
        name: $name:expr,
        description: $description:expr,
        execute: |$params:ident| $body:block
    }) => {
        #[async_trait::async_trait]
        impl $crate::tools::Tool for $tool_type {
            fn name(&self) -> &str {
                $name
            }

            fn description(&self) -> &str {
                $description
            }

            fn parameters_schema(&self) -> serde_json::Value {
                let schema = schemars::schema_for!($params_type);
                serde_json::to_value(schema).unwrap()
            }

            async fn execute(
                &self,
                parameters: serde_json::Value,
            ) -> Result<serde_json::Value, $crate::tools::ToolError> {
                let $params: $params_type = serde_json::from_value(parameters).map_err(|error| {
                    $crate::tools::ToolError::InvalidParameters {
                        tool_name: self.name().to_string(),
                        message: format!("Failed to parse parameters: {}", error),
                        suggestion: Some("Check parameter types and required fields".to_string()),
                    }
                })?;

                let result: Result<serde_json::Value, $crate::tools::error::SimpleToolError> = (|| $body)();
                result.map_err(|e| e.into_tool_error(self.name().to_string()))
            }
        }
    };

    // With parameters and custom schema
    ($tool_type:ty, $params_type:ty, {
        name: $name:expr,
        description: $description:expr,
        schema: |$self_schema:ident| $schema_body:block,
        execute: |$params:ident| $body:block
    }) => {
        #[async_trait::async_trait]
        impl $crate::tools::Tool for $tool_type {
            fn name(&self) -> &str {
                $name
            }

            fn description(&self) -> &str {
                $description
            }

            fn parameters_schema(&$self_schema) -> serde_json::Value {
                $schema_body
            }

            async fn execute(
                &self,
                parameters: serde_json::Value,
            ) -> Result<serde_json::Value, $crate::tools::ToolError> {
                let $params: $params_type = serde_json::from_value(parameters).map_err(|error| {
                    $crate::tools::ToolError::InvalidParameters {
                        tool_name: self.name().to_string(),
                        message: format!("Failed to parse parameters: {}", error),
                        suggestion: Some("Check parameter types and required fields".to_string()),
                    }
                })?;

                let result: Result<serde_json::Value, $crate::tools::error::SimpleToolError> = (|| $body)();
                result.map_err(|e| e.into_tool_error(self.name().to_string()))
            }
        }
    };

    // Without parameters
    ($tool_type:ty, {
        name: $name:expr,
        description: $description:expr,
        execute: || $body:block
    }) => {
        #[async_trait::async_trait]
        impl $crate::tools::Tool for $tool_type {
            fn name(&self) -> &str {
                $name
            }

            fn description(&self) -> &str {
                $description
            }

            fn parameters_schema(&self) -> serde_json::Value {
                serde_json::json!({
                    "type": "object",
                    "properties": {},
                    "required": []
                })
            }

            async fn execute(
                &self,
                _parameters: serde_json::Value,
            ) -> Result<serde_json::Value, $crate::tools::ToolError> {
                let result: Result<serde_json::Value, $crate::tools::error::SimpleToolError> = (|| $body)();
                result.map_err(|e| e.into_tool_error(self.name().to_string()))
            }
        }
    };
}
