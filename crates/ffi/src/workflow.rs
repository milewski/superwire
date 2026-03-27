use crate::error::{FfiError, FfiErrorCode};
use engine_ai_core::dsl::{parse_workflow, Workflow};
use engine_ai_core::execute_workflow as execute_core_workflow;
use engine_ai_core::runtime::{AgentRunner, WorkflowRuntime};
use serde::{Deserialize, Serialize};
use serde_json::Value;

const DEFAULT_WORKFLOW_SOURCE_NAME: &str = "ffi";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowExecutionRequest {
    pub workflow_source: String,
    pub workflow_source_name: String,
    pub input_value: Value,
}

impl WorkflowExecutionRequest {
    #[must_use]
    pub fn new(workflow_source: impl Into<String>, input_value: Value) -> Self {
        Self {
            workflow_source: workflow_source.into(),
            workflow_source_name: DEFAULT_WORKFLOW_SOURCE_NAME.to_string(),
            input_value,
        }
    }

    #[must_use]
    pub fn without_input(workflow_source: impl Into<String>) -> Self {
        Self::new(workflow_source, Value::Null)
    }

    #[must_use]
    pub fn with_workflow_source_name(mut self, workflow_source_name: impl Into<String>) -> Self {
        self.workflow_source_name = workflow_source_name.into();
        self
    }

    pub fn parse_workflow(&self) -> Result<Workflow, FfiError> {
        parse_workflow(&self.workflow_source).map_err(|parse_error| {
            let rendered_details = parse_error.render_with_source(&self.workflow_source, &self.workflow_source_name);

            FfiError::new(FfiErrorCode::WorkflowParseFailed, rendered_details)
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowExecutionStatus {
    Succeeded,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowExecutionResponse {
    pub status: WorkflowExecutionStatus,
    pub output_value: Value,
}

impl WorkflowExecutionResponse {
    #[must_use]
    pub fn success(output_value: Value) -> Self {
        Self {
            status: WorkflowExecutionStatus::Succeeded,
            output_value,
        }
    }
}

pub async fn execute_workflow(request: &WorkflowExecutionRequest) -> Result<WorkflowExecutionResponse, FfiError> {
    let workflow = request.parse_workflow()?;

    let output_value = execute_core_workflow::<Value, Value>(&workflow, request.input_value.clone())
        .await
        .map_err(FfiError::from_workflow_runtime_error)?;

    Ok(WorkflowExecutionResponse::success(output_value))
}

pub async fn execute_workflow_with_runner<RunnerType>(
    request: &WorkflowExecutionRequest,
    runner: &RunnerType,
) -> Result<WorkflowExecutionResponse, FfiError>
where
    RunnerType: AgentRunner,
{
    let workflow = request.parse_workflow()?;

    let workflow_runtime = WorkflowRuntime::<Value, Value>::new(workflow).map_err(FfiError::from_workflow_runtime_error)?;

    let output_value = workflow_runtime
        .run_with_runner(request.input_value.clone(), runner)
        .await
        .map_err(FfiError::from_workflow_runtime_error)?;

    Ok(WorkflowExecutionResponse::success(output_value))
}
