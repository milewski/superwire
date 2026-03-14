use engine_ai_core::execution::ExecutionEngine as CoreExecutionEngine;
use engine_ai_core::tools::{Tool as CoreTool, ToolError, ToolRef, ToolRegistry};
use napi::bindgen_prelude::*;
use napi::threadsafe_function::ThreadsafeFunction;
use napi_derive::napi;
use serde_json::Value;
use std::sync::Arc;

#[napi]
pub struct ExecutionEngine {
    inner: CoreExecutionEngine,
}

impl Default for ExecutionEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[napi]
impl ExecutionEngine {
    #[napi(constructor)]
    pub fn new() -> Self {
        Self {
            inner: CoreExecutionEngine::new(),
        }
    }

    #[napi(factory)]
    pub fn with_tools(tools: Vec<&Tool>) -> Self {
        let mut registry = ToolRegistry::new();
        for tool in tools {
            registry.register(tool.inner.clone());
        }
        Self {
            inner: CoreExecutionEngine::with_tools(registry),
        }
    }

    #[napi]
    pub async fn execute_workflow(&self, workflow_path: String) -> Result<String> {
        let result = self
            .inner
            .execute_workflow(&workflow_path)
            .await
            .map_err(|error| Error::from_reason(error.to_string()))?;

        serde_json::to_string_pretty(&result).map_err(|error| Error::from_reason(error.to_string()))
    }

    #[napi]
    pub async fn execute_workflow_content(&self, workflow_content: String) -> Result<String> {
        let result = self
            .inner
            .execute_workflow_content(&workflow_content)
            .await
            .map_err(|error| Error::from_reason(error.to_string()))?;

        serde_json::to_string_pretty(&result).map_err(|error| Error::from_reason(error.to_string()))
    }

    #[napi]
    pub async fn execute_workflow_with_inputs(&self, workflow_path: String, inputs: String) -> Result<String> {
        let inputs_map: std::collections::HashMap<String, Value> =
            serde_json::from_str(&inputs).map_err(|error| Error::from_reason(error.to_string()))?;

        let result = self
            .inner
            .execute_workflow_with_inputs(&workflow_path, inputs_map)
            .await
            .map_err(|error| Error::from_reason(error.to_string()))?;

        serde_json::to_string_pretty(&result).map_err(|error| Error::from_reason(error.to_string()))
    }

    #[napi]
    pub async fn execute_workflow_content_with_inputs(
        &self,
        workflow_content: String,
        inputs: String,
    ) -> Result<String> {
        let inputs_map: std::collections::HashMap<String, Value> =
            serde_json::from_str(&inputs).map_err(|error| Error::from_reason(error.to_string()))?;

        let result = self
            .inner
            .execute_workflow_from_content_with_inputs(&workflow_content, "workflow", inputs_map)
            .await
            .map_err(|error| Error::from_reason(error.to_string()))?;

        serde_json::to_string_pretty(&result).map_err(|error| Error::from_reason(error.to_string()))
    }
}

struct ToolWrapper {
    name: String,
    description: String,
    parameters_schema: Value,
    execute_fn: Arc<ThreadsafeFunction<String, String>>,
}

impl ToolWrapper {
    fn new(
        name: String,
        description: String,
        parameters_schema: Value,
        execute_fn: ThreadsafeFunction<String, String>,
    ) -> Self {
        Self {
            name,
            description,
            parameters_schema,
            execute_fn: Arc::new(execute_fn),
        }
    }
}

#[async_trait::async_trait]
impl CoreTool for ToolWrapper {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn parameters_schema(&self) -> Value {
        self.parameters_schema.clone()
    }

    async fn execute(&self, parameters: Value) -> std::result::Result<Value, ToolError> {
        let params_string = serde_json::to_string(&parameters).map_err(|error| ToolError::ExecutionError {
            tool_name: self.name.clone(),
            message: format!("Failed to serialize parameters: {error}"),
            suggestion: None,
        })?;

        let result_string =
            self.execute_fn
                .call_async(Ok(params_string))
                .await
                .map_err(|error| ToolError::ExecutionError {
                    tool_name: self.name.clone(),
                    message: format!("Tool execution failed: {error}"),
                    suggestion: None,
                })?;

        serde_json::from_str(&result_string).map_err(|error| ToolError::ExecutionError {
            tool_name: self.name.clone(),
            message: format!("Failed to parse tool result: {error}"),
            suggestion: Some("Ensure the tool returns valid JSON".to_string()),
        })
    }
}

#[napi]
pub struct Tool {
    inner: ToolRef,
}

#[napi]
impl Tool {
    #[napi(constructor)]
    pub fn new(
        name: String,
        description: String,
        parameters_schema: String,
        execute_fn: ThreadsafeFunction<String, String>,
    ) -> Result<Self> {
        let schema: Value =
            serde_json::from_str(&parameters_schema).map_err(|error| Error::from_reason(error.to_string()))?;

        let wrapper = ToolWrapper::new(name, description, schema, execute_fn);

        Ok(Self {
            inner: Arc::new(wrapper),
        })
    }

    #[must_use]
    #[napi(getter)]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }

    #[must_use]
    #[napi(getter)]
    pub fn description(&self) -> String {
        self.inner.description().to_string()
    }

    #[must_use]
    #[napi(getter)]
    pub fn parameters_schema(&self) -> String {
        serde_json::to_string_pretty(&self.inner.parameters_schema()).unwrap_or_default()
    }
}
