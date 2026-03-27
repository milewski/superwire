mod error;
#[cfg(feature = "js")]
mod javascript;
mod tool;
mod workflow;

pub use error::{FfiError, FfiErrorCode};
#[cfg(feature = "js")]
pub use javascript::execute_workflow_json;
pub use tool::{
    ForeignToolDefinition, ForeignToolRuntime, ToolInvocationError, ToolInvocationErrorCode, ToolInvocationRequest, ToolInvocationResult,
};
pub use workflow::{
    execute_workflow, execute_workflow_with_runner, WorkflowExecutionRequest, WorkflowExecutionResponse, WorkflowExecutionStatus,
};

pub use engine_ai_core::try_workflow;
