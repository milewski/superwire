mod ir;
mod pipeline;
mod plan;

pub use ir::{build_typed_workflow_ir, TypedAgentIr, TypedWorkflowIr};
pub use pipeline::{
    compile_workflow_pipeline, NormalizeStageOutput, ParseStageOutput, PlanStageOutput, TypecheckStageOutput, ValidateStageOutput,
    WorkflowPipeline, WorkflowPipelineInput,
};
pub use plan::{build_execution_plan, ExecutionPlan, PlannedAgent};
