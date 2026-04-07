pub mod error;
pub mod expression;
pub mod functions;
pub mod inference;
pub mod provider;
mod runner;
pub mod type_inference;
pub mod types;
mod wasm_tools;
mod workflow_runtime;

#[cfg(test)]
mod tests;

pub use error::WorkflowRuntimeError;
pub use inference::InferenceSetting;
pub use provider::{ProviderConfig, ProviderDriver};
pub use runner::{AgentExecutionRequest, AgentExecutionResult, AgentRunner, LoopAgentRunner, RequestedAgentTool};
pub use workflow_runtime::{execute_workflow, execute_workflow_without_input, WorkflowRuntime};

#[macro_export]
macro_rules! try_workflow {
    ($workflow_path:literal) => {{
        async {
            let workflow_source = include_str!($workflow_path);
            let parsed_workflow = $crate::dsl::parse_workflow(workflow_source).map_err(|parse_error| {
                let rendered_details = parse_error.render_with_source(workflow_source, $workflow_path);

                $crate::runtime::WorkflowRuntimeError::ParseFailed {
                    source: parse_error,
                    details: rendered_details,
                }
            })?;

            $crate::runtime::execute_workflow_without_input(&parsed_workflow).await
        }
    }};
    ($workflow_path:literal, $input:expr) => {{
        async {
            let workflow_source = include_str!($workflow_path);
            let parsed_workflow = $crate::dsl::parse_workflow(workflow_source).map_err(|parse_error| {
                let rendered_details = parse_error.render_with_source(workflow_source, $workflow_path);

                $crate::runtime::WorkflowRuntimeError::ParseFailed {
                    source: parse_error,
                    details: rendered_details,
                }
            })?;

            $crate::runtime::execute_workflow(&parsed_workflow, $input).await
        }
    }};
    ($workflow:expr) => {{
        $crate::runtime::execute_workflow_without_input(&$workflow)
    }};
    ($workflow:expr, $input:expr) => {{
        $crate::runtime::execute_workflow(&$workflow, $input)
    }};
}
