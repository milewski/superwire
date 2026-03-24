use crate::runtime::engine::WorkflowRuntime;
use crate::runtime::error::WorkflowRuntimeError;
use crate::runtime::provider::DefaultProviderFactory;
use serde::de::DeserializeOwned;
use serde_json::{json, Value};

impl WorkflowRuntime<DefaultProviderFactory> {
    #[must_use]
    pub fn with_default_providers() -> Self {
        Self::new(DefaultProviderFactory)
    }
}

pub async fn try_workflow_from_source<OutputType>(
    workflow_source: &str,
) -> Result<Result<OutputType, serde_json::Error>, WorkflowRuntimeError>
where
    OutputType: DeserializeOwned,
{
    try_workflow_from_source_with_values(workflow_source, json!({}), json!({})).await
}

pub async fn try_workflow_from_source_with_values<OutputType>(
    workflow_source: &str,
    input_values: Value,
    secret_values: Value,
) -> Result<Result<OutputType, serde_json::Error>, WorkflowRuntimeError>
where
    OutputType: DeserializeOwned,
{
    let runtime = WorkflowRuntime::with_default_providers();
    let execution_result = runtime.execute_source(workflow_source, input_values, secret_values).await?;

    Ok(serde_json::from_value(execution_result.output))
}
