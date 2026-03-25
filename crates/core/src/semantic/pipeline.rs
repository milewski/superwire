use crate::dsl::{parse_workflow, validate_workflow, ValidationReport, Workflow};
use crate::runtime::error::WorkflowRuntimeError;
use crate::semantic::ir::{build_typed_workflow_ir, TypedWorkflowIr};
use crate::semantic::plan::{build_execution_plan, ExecutionPlan};
use schemars::JsonSchema;
use serde::de::DeserializeOwned;
use serde::Serialize;

#[derive(Debug, Clone, Copy)]
pub enum WorkflowPipelineInput<'input> {
    Source(&'input str),
    Workflow(&'input Workflow),
}

#[derive(Debug, Clone)]
pub struct WorkflowPipeline<PipelineState> {
    pipeline_state: PipelineState,
}

impl<PipelineState> WorkflowPipeline<PipelineState> {
    fn new(pipeline_state: PipelineState) -> Self {
        Self { pipeline_state }
    }

    #[must_use]
    pub fn state(&self) -> &PipelineState {
        &self.pipeline_state
    }

    #[must_use]
    pub fn into_state(self) -> PipelineState {
        self.pipeline_state
    }
}

impl WorkflowPipeline<ParseStageOutput> {
    pub fn parse(workflow_input: WorkflowPipelineInput<'_>) -> Result<Self, WorkflowRuntimeError> {
        let parse_stage_output = parse_workflow_stage(workflow_input)?;

        Ok(Self::new(parse_stage_output))
    }

    #[must_use]
    pub fn normalize(self) -> WorkflowPipeline<NormalizeStageOutput> {
        WorkflowPipeline::new(self.pipeline_state.normalize())
    }
}

impl WorkflowPipeline<NormalizeStageOutput> {
    pub fn validate(self) -> Result<WorkflowPipeline<ValidateStageOutput>, WorkflowRuntimeError> {
        let validate_stage_output = self.pipeline_state.validate()?;

        Ok(WorkflowPipeline::new(validate_stage_output))
    }
}

impl WorkflowPipeline<ValidateStageOutput> {
    pub fn typecheck<Input, Output>(self) -> Result<WorkflowPipeline<TypecheckStageOutput>, WorkflowRuntimeError>
    where
        Input: Serialize + JsonSchema,
        Output: DeserializeOwned + JsonSchema,
    {
        let typecheck_stage_output = self.pipeline_state.typecheck::<Input, Output>()?;

        Ok(WorkflowPipeline::new(typecheck_stage_output))
    }
}

impl WorkflowPipeline<TypecheckStageOutput> {
    pub fn plan(self) -> Result<WorkflowPipeline<PlanStageOutput>, WorkflowRuntimeError> {
        let plan_stage_output = self.pipeline_state.plan()?;

        Ok(WorkflowPipeline::new(plan_stage_output))
    }
}

impl WorkflowPipeline<PlanStageOutput> {
    #[must_use]
    pub fn execution_plan(&self) -> &ExecutionPlan {
        self.pipeline_state.execution_plan()
    }

    #[must_use]
    pub fn into_execution_plan(self) -> ExecutionPlan {
        self.pipeline_state.into_execution_plan()
    }
}

#[derive(Debug, Clone)]
pub struct ParseStageOutput {
    workflow: Workflow,
}

impl ParseStageOutput {
    #[must_use]
    pub fn workflow(&self) -> &Workflow {
        &self.workflow
    }

    #[must_use]
    pub fn into_workflow(self) -> Workflow {
        self.workflow
    }

    #[must_use]
    pub fn normalize(self) -> NormalizeStageOutput {
        NormalizeStageOutput { workflow: self.workflow }
    }
}

#[derive(Debug, Clone)]
pub struct NormalizeStageOutput {
    workflow: Workflow,
}

impl NormalizeStageOutput {
    #[must_use]
    pub fn workflow(&self) -> &Workflow {
        &self.workflow
    }

    pub fn validate(self) -> Result<ValidateStageOutput, WorkflowRuntimeError> {
        let validation_report = validate_workflow(&self.workflow);

        if validation_report.has_issues() {
            return Err(WorkflowRuntimeError::InvalidWorkflow {
                issues: render_validation_report(&validation_report),
            });
        }

        Ok(ValidateStageOutput {
            workflow: self.workflow,
            validation_report,
        })
    }
}

#[derive(Debug, Clone)]
pub struct ValidateStageOutput {
    workflow: Workflow,
    validation_report: ValidationReport,
}

impl ValidateStageOutput {
    #[must_use]
    pub fn workflow(&self) -> &Workflow {
        &self.workflow
    }

    #[must_use]
    pub fn validation_report(&self) -> &ValidationReport {
        &self.validation_report
    }

    pub fn typecheck<Input, Output>(self) -> Result<TypecheckStageOutput, WorkflowRuntimeError>
    where
        Input: Serialize + JsonSchema,
        Output: DeserializeOwned + JsonSchema,
    {
        let typed_workflow_ir = build_typed_workflow_ir::<Input, Output>(&self.workflow)?;

        Ok(TypecheckStageOutput {
            workflow: self.workflow,
            validation_report: self.validation_report,
            typed_workflow_ir,
        })
    }
}

#[derive(Debug, Clone)]
pub struct TypecheckStageOutput {
    workflow: Workflow,
    validation_report: ValidationReport,
    typed_workflow_ir: TypedWorkflowIr,
}

impl TypecheckStageOutput {
    #[must_use]
    pub fn workflow(&self) -> &Workflow {
        &self.workflow
    }

    #[must_use]
    pub fn validation_report(&self) -> &ValidationReport {
        &self.validation_report
    }

    #[must_use]
    pub fn typed_workflow_ir(&self) -> &TypedWorkflowIr {
        &self.typed_workflow_ir
    }

    pub fn plan(self) -> Result<PlanStageOutput, WorkflowRuntimeError> {
        let execution_plan = build_execution_plan(&self.workflow, &self.typed_workflow_ir)?;

        Ok(PlanStageOutput {
            workflow: self.workflow,
            validation_report: self.validation_report,
            typed_workflow_ir: self.typed_workflow_ir,
            execution_plan,
        })
    }
}

#[derive(Debug, Clone)]
pub struct PlanStageOutput {
    workflow: Workflow,
    validation_report: ValidationReport,
    typed_workflow_ir: TypedWorkflowIr,
    execution_plan: ExecutionPlan,
}

impl PlanStageOutput {
    #[must_use]
    pub fn workflow(&self) -> &Workflow {
        &self.workflow
    }

    #[must_use]
    pub fn validation_report(&self) -> &ValidationReport {
        &self.validation_report
    }

    #[must_use]
    pub fn typed_workflow_ir(&self) -> &TypedWorkflowIr {
        &self.typed_workflow_ir
    }

    #[must_use]
    pub fn execution_plan(&self) -> &ExecutionPlan {
        &self.execution_plan
    }

    #[must_use]
    pub fn into_execution_plan(self) -> ExecutionPlan {
        self.execution_plan
    }
}

pub fn parse_workflow_stage(workflow_input: WorkflowPipelineInput<'_>) -> Result<ParseStageOutput, WorkflowRuntimeError> {
    let workflow = match workflow_input {
        WorkflowPipelineInput::Source(workflow_source) => {
            parse_workflow(workflow_source).map_err(|source| WorkflowRuntimeError::ParseFailed { source })?
        }
        WorkflowPipelineInput::Workflow(workflow) => workflow.clone(),
    };

    Ok(ParseStageOutput { workflow })
}

#[must_use]
pub fn normalize_workflow_stage(parse_stage_output: ParseStageOutput) -> NormalizeStageOutput {
    parse_stage_output.normalize()
}

pub fn validate_workflow_stage(normalize_stage_output: NormalizeStageOutput) -> Result<ValidateStageOutput, WorkflowRuntimeError> {
    normalize_stage_output.validate()
}

pub fn typecheck_workflow_stage<Input, Output>(
    validate_stage_output: ValidateStageOutput,
) -> Result<TypecheckStageOutput, WorkflowRuntimeError>
where
    Input: Serialize + JsonSchema,
    Output: DeserializeOwned + JsonSchema,
{
    validate_stage_output.typecheck::<Input, Output>()
}

pub fn plan_workflow_stage(typecheck_stage_output: TypecheckStageOutput) -> Result<PlanStageOutput, WorkflowRuntimeError> {
    typecheck_stage_output.plan()
}

pub fn compile_workflow_pipeline<Input, Output>(workflow_input: WorkflowPipelineInput<'_>) -> Result<PlanStageOutput, WorkflowRuntimeError>
where
    Input: Serialize + JsonSchema,
    Output: DeserializeOwned + JsonSchema,
{
    WorkflowPipeline::parse(workflow_input)?
        .normalize()
        .validate()?
        .typecheck::<Input, Output>()?
        .plan()
        .map(WorkflowPipeline::into_state)
}

fn render_validation_report(validation_report: &ValidationReport) -> String {
    validation_report
        .issues_with_spans()
        .map(|(validation_issue, issue_span)| match issue_span {
            Some(issue_span) => format!("- {validation_issue:?} at {}:{}", issue_span.start.line, issue_span.start.column),
            None => format!("- {validation_issue:?}"),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::{
        compile_workflow_pipeline, normalize_workflow_stage, parse_workflow_stage, plan_workflow_stage, typecheck_workflow_stage,
        validate_workflow_stage, WorkflowPipeline, WorkflowPipelineInput,
    };
    use crate::parse_inline_workflow;
    use crate::runtime::error::WorkflowRuntimeError;
    use schemars::JsonSchema;
    use serde::{Deserialize, Serialize};
    use std::sync::LazyLock;

    #[derive(Debug, Serialize, JsonSchema)]
    struct Input {
        topic: String,
    }

    #[derive(Debug, Deserialize, JsonSchema)]
    #[allow(dead_code)]
    struct Output {
        final_text: String,
    }

    static VALID_WORKFLOW: LazyLock<crate::dsl::Workflow> = LazyLock::new(|| {
        parse_inline_workflow! {
            provider openai {
                driver: "openai"
                models: ["model-a"]
            }

            input {
                topic: string
            }

            agent first {
                model: openai("model-a")
                prompt: input.topic
                output: string
            }

            agent second {
                model: openai("model-a")
                prompt: agent.first
                output: string
            }

            output {
                final_text: agent.second
            }
        }
    });

    #[test]
    fn supports_explicit_staged_pipeline_execution() {
        let plan_stage_output = WorkflowPipeline::parse(WorkflowPipelineInput::Workflow(&VALID_WORKFLOW))
            .expect("parse stage should succeed")
            .normalize()
            .validate()
            .expect("validate stage should succeed")
            .typecheck::<Input, Output>()
            .expect("typecheck stage should succeed")
            .plan()
            .expect("plan stage should succeed");

        assert_eq!(
            plan_stage_output.execution_plan().agent_execution_order,
            vec!["first".to_string(), "second".to_string()]
        );
    }

    #[test]
    fn stage_functions_match_pipeline_wrapper_transitions() {
        let parse_stage_output = parse_workflow_stage(WorkflowPipelineInput::Workflow(&VALID_WORKFLOW)).unwrap();
        let normalize_stage_output = normalize_workflow_stage(parse_stage_output);
        let validate_stage_output = validate_workflow_stage(normalize_stage_output).unwrap();
        let typecheck_stage_output = typecheck_workflow_stage::<Input, Output>(validate_stage_output).unwrap();
        let plan_stage_output = plan_workflow_stage(typecheck_stage_output).unwrap();

        assert_eq!(
            plan_stage_output.execution_plan().agent_execution_order,
            vec!["first".to_string(), "second".to_string()]
        );
    }

    #[test]
    fn orchestration_entrypoint_runs_all_stages() {
        let plan_stage_output = compile_workflow_pipeline::<Input, Output>(WorkflowPipelineInput::Workflow(&VALID_WORKFLOW)).unwrap();

        assert_eq!(
            plan_stage_output.execution_plan().agent_execution_order,
            vec!["first".to_string(), "second".to_string()]
        );
    }

    #[test]
    fn validation_stage_rejects_invalid_workflow() {
        let workflow = parse_inline_workflow! {
            input {
                known_field: string
            }

            output {
                broken: input.missing_field
            }
        };

        let parse_stage_output = parse_workflow_stage(WorkflowPipelineInput::Workflow(&workflow)).unwrap();
        let normalize_stage_output = normalize_workflow_stage(parse_stage_output);
        let validate_result = validate_workflow_stage(normalize_stage_output);

        assert!(matches!(
            validate_result,
            Err(WorkflowRuntimeError::InvalidWorkflow { issues }) if issues.contains("UnknownInputFieldReference")
        ));
    }
}
