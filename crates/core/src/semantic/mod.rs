mod ir;
mod pipeline;
mod plan;

pub use ir::{build_typed_workflow_ir, TypedAgentIr, TypedWorkflowIr};
pub use pipeline::{
    compile_workflow_pipeline, normalize_workflow_stage, parse_workflow_stage, plan_workflow_stage, typecheck_workflow_stage,
    validate_workflow_stage, NormalizeStageOutput, ParseStageOutput, PlanStageOutput, TypecheckStageOutput, ValidateStageOutput,
    WorkflowPipeline, WorkflowPipelineInput,
};
pub use plan::{build_execution_plan, ExecutionPlan, PlannedAgent};
