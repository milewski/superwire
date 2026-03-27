use crate::{execute_workflow, FfiError, FfiErrorCode, WorkflowExecutionRequest};
use napi::Result as NapiResult;
use napi_derive::napi;

#[napi]
pub async fn execute_workflow_json(workflow_execution_request_json: String) -> NapiResult<String> {
    let workflow_execution_request: WorkflowExecutionRequest =
        serde_json::from_str(&workflow_execution_request_json).map_err(|source_error| {
            FfiError::new(FfiErrorCode::InvalidRequest, "Failed to parse workflow execution request JSON")
                .with_details(serde_json::json!({
                    "source_error": source_error.to_string(),
                }))
                .into_napi_error()
        })?;

    let workflow_execution_response = execute_workflow(&workflow_execution_request)
        .await
        .map_err(FfiError::into_napi_error)?;

    serde_json::to_string(&workflow_execution_response).map_err(|source_error| {
        FfiError::new(
            FfiErrorCode::SerializationFailed,
            "Failed to serialize workflow execution response JSON",
        )
        .with_details(serde_json::json!({
            "source_error": source_error.to_string(),
        }))
        .into_napi_error()
    })
}
