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
pub use wasm_tools::WasmTool;
pub use workflow_runtime::{
    execute_workflow, execute_workflow_file, execute_workflow_file_without_input, execute_workflow_without_input, WorkflowRuntime,
};

#[macro_export]
macro_rules! try_workflow {
    ($workflow_path:literal) => {{
        async {
            let manifest_directory = ::std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
            let caller_file_path = ::std::path::Path::new(file!());
            let caller_directory = caller_file_path.parent().unwrap_or_else(|| ::std::path::Path::new(""));

            let mut workflow_path = manifest_directory.join(caller_directory).join($workflow_path);

            if !workflow_path.exists() {
                let workspace_root = manifest_directory
                    .parent()
                    .and_then(::std::path::Path::parent)
                    .unwrap_or(manifest_directory);

                workflow_path = workspace_root.join(caller_directory).join($workflow_path);
            }

            $crate::runtime::execute_workflow_file_without_input(&workflow_path).await
        }
    }};
    ($workflow_path:literal, $input:expr) => {{
        async {
            let manifest_directory = ::std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
            let caller_file_path = ::std::path::Path::new(file!());
            let caller_directory = caller_file_path.parent().unwrap_or_else(|| ::std::path::Path::new(""));

            let mut workflow_path = manifest_directory.join(caller_directory).join($workflow_path);

            if !workflow_path.exists() {
                let workspace_root = manifest_directory
                    .parent()
                    .and_then(::std::path::Path::parent)
                    .unwrap_or(manifest_directory);

                workflow_path = workspace_root.join(caller_directory).join($workflow_path);
            }

            $crate::runtime::execute_workflow_file(&workflow_path, $input).await
        }
    }};
    ($workflow:expr) => {{
        $crate::runtime::execute_workflow_without_input(&$workflow)
    }};
    ($workflow:expr, $input:expr) => {{
        $crate::runtime::execute_workflow(&$workflow, $input)
    }};
}

#[macro_export]
macro_rules! tool {
    ($tool_path:literal) => {{
        {
            let manifest_directory = ::std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
            let caller_file_path = ::std::path::Path::new(file!());
            let caller_directory = caller_file_path.parent().unwrap_or_else(|| ::std::path::Path::new(""));

            let mut resolved_tool_path = manifest_directory.join(caller_directory).join($tool_path);

            if !resolved_tool_path.exists() {
                let workspace_root = manifest_directory
                    .parent()
                    .and_then(::std::path::Path::parent)
                    .unwrap_or(manifest_directory);

                resolved_tool_path = workspace_root.join(caller_directory).join($tool_path);
            }

            $crate::runtime::WasmTool::from_file(&resolved_tool_path)
        }
    }};
}
