use crate::diagnostics::CommandError;
use crate::input::parse_workflow_invocation_bindings;
use engine_ai_core::semantic::{compile_dynamic_workflow, DynamicCompiledWorkflow, WorkflowPipelineInput};
use engine_ai_core::DynamicWorkflowRuntime;
use serde_json::Value;
use tokio::runtime::Runtime;

pub fn compile_dynamic_workflow_from_source(workflow_source: &str) -> Result<DynamicCompiledWorkflow, CommandError> {
    compile_dynamic_workflow(WorkflowPipelineInput::Source(workflow_source))
        .map_err(|error| CommandError::invalid_workflow(format!("workflow static compilation failed: {error}")))
}

pub fn execute_workflow_from_source(workflow_source: &str, invocation_arguments: &[String]) -> Result<Value, CommandError> {
    let dynamic_compiled_workflow = compile_dynamic_workflow_from_source(workflow_source)?;

    execute_compiled_workflow(dynamic_compiled_workflow, invocation_arguments)
}

pub fn execute_compiled_workflow(
    dynamic_compiled_workflow: DynamicCompiledWorkflow,
    invocation_arguments: &[String],
) -> Result<Value, CommandError> {
    let workflow_invocation_bindings = parse_workflow_invocation_bindings(
        invocation_arguments,
        dynamic_compiled_workflow.input_type(),
        dynamic_compiled_workflow.typed_workflow_ir().secrets_type.as_ref(),
    )?;

    let runtime = DynamicWorkflowRuntime::from_compiled_workflow(dynamic_compiled_workflow);
    let tokio_runtime = Runtime::new().map_err(|error| CommandError::internal(format!("failed to initialize tokio runtime: {error}")))?;
    let execution_result = tokio_runtime.block_on(runtime.run(
        workflow_invocation_bindings.input_values,
        workflow_invocation_bindings.secret_values,
    ));

    execution_result.map_err(|error| CommandError::runtime(error.to_string()))
}
