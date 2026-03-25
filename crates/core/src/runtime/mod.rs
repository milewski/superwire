pub(crate) mod error;
pub(crate) mod expression;
mod inference;
pub(crate) mod provider;
mod runner;
pub(crate) mod type_inference;
pub(crate) mod types;
mod workflow_runtime;

#[cfg(test)]
mod tests;

pub use error::WorkflowRuntimeError;
pub use provider::{ProviderConfig, ProviderDriver};
pub use runner::{AgentExecutionRequest, AgentExecutionResult, AgentRunner};
pub use workflow_runtime::{execute_workflow, execute_workflow_without_input, WorkflowRuntime};

#[macro_export]
macro_rules! try_workflow {
    ($workflow:expr) => {{
        $crate::runtime::execute_workflow_without_input(&$workflow)
    }};
    ($workflow:expr, $input:expr) => {{
        $crate::runtime::execute_workflow(&$workflow, $input)
    }};
}
