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
    state: PipelineState,
}

impl<PipelineState> WorkflowPipeline<PipelineState> {
    fn new(state: PipelineState) -> Self {
        Self { state }
    }

    #[must_use]
    pub fn state(&self) -> &PipelineState {
        &self.state
    }

    #[must_use]
    pub fn into_state(self) -> PipelineState {
        self.state
    }
}

impl WorkflowPipeline<ParseStageOutput> {
    pub fn parse(input: WorkflowPipelineInput<'_>) -> Result<Self, WorkflowRuntimeError> {
        let workflow = match input {
            WorkflowPipelineInput::Source(source_text) => parse_workflow(source_text).map_err(|parse_error| {
                let rendered_details = parse_error.render_with_source(source_text, "<workflow>");

                WorkflowRuntimeError::ParseFailed {
                    source: parse_error,
                    details: rendered_details,
                }
            })?,
            WorkflowPipelineInput::Workflow(workflow) => workflow.clone(),
        };

        Ok(Self::new(ParseStageOutput { workflow }))
    }

    #[must_use]
    pub fn normalize(self) -> WorkflowPipeline<NormalizeStageOutput> {
        WorkflowPipeline::new(self.state.normalize())
    }
}

impl WorkflowPipeline<NormalizeStageOutput> {
    pub fn validate(self) -> Result<WorkflowPipeline<ValidateStageOutput>, WorkflowRuntimeError> {
        Ok(WorkflowPipeline::new(self.state.validate()?))
    }
}

impl WorkflowPipeline<ValidateStageOutput> {
    pub fn typecheck<Input, Output>(self) -> Result<WorkflowPipeline<TypecheckStageOutput>, WorkflowRuntimeError>
    where
        Input: Serialize + JsonSchema,
        Output: DeserializeOwned + JsonSchema,
    {
        Ok(WorkflowPipeline::new(self.state.typecheck::<Input, Output>()?))
    }
}

impl WorkflowPipeline<TypecheckStageOutput> {
    pub fn plan(self) -> Result<WorkflowPipeline<PlanStageOutput>, WorkflowRuntimeError> {
        Ok(WorkflowPipeline::new(self.state.plan()?))
    }
}

impl WorkflowPipeline<PlanStageOutput> {
    #[must_use]
    pub fn execution_plan(&self) -> &ExecutionPlan {
        self.state.execution_plan()
    }

    #[must_use]
    pub fn into_execution_plan(self) -> ExecutionPlan {
        self.state.into_execution_plan()
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
            let rendered_validation_issues = if let Some(source_text) = self.workflow.source_text() {
                validation_report.render_with_source(source_text, "<workflow>")
            } else {
                validation_report.render()
            };

            return Err(WorkflowRuntimeError::InvalidWorkflow {
                issues: rendered_validation_issues,
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

#[cfg(test)]
mod tests {
    use super::{compile_workflow_pipeline, WorkflowPipeline, WorkflowPipelineInput};
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
                endpoint: "https://api.openai.com/v1"
                api_key: "test-api-key"
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
    fn pipeline_state_transitions_allow_stage_level_assertions() {
        let parse_pipeline = WorkflowPipeline::parse(WorkflowPipelineInput::Workflow(&VALID_WORKFLOW)).expect("parse stage should succeed");

        assert!(!parse_pipeline.state().workflow().declarations().is_empty());

        let normalize_pipeline = parse_pipeline.normalize();
        let validate_pipeline = normalize_pipeline.validate().expect("validate stage should succeed");
        let typecheck_pipeline = validate_pipeline
            .typecheck::<Input, Output>()
            .expect("typecheck stage should succeed");
        let plan_pipeline = typecheck_pipeline.plan().expect("plan stage should succeed");

        assert_eq!(
            plan_pipeline.execution_plan().agent_execution_order,
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

        let validate_result = WorkflowPipeline::parse(WorkflowPipelineInput::Workflow(&workflow))
            .expect("parse stage should succeed")
            .normalize()
            .validate();

        assert!(matches!(
            validate_result,
            Err(WorkflowRuntimeError::InvalidWorkflow { issues })
                if issues.contains("unknown_input_field_reference")
                    && issues.contains("missing_field")
                    && issues.contains("<workflow>:")
        ));
    }

    #[test]
    fn validation_stage_rejects_output_reference_without_agent_output_type() {
        let workflow = parse_inline_workflow! {
            agent greeting {
                prompt: "Write a short welcome message."
            }

            output {
                greeting: agent.greeting
            }
        };

        let validate_result = WorkflowPipeline::parse(WorkflowPipelineInput::Workflow(&workflow))
            .expect("parse stage should succeed")
            .normalize()
            .validate();

        assert!(matches!(
            validate_result,
            Err(WorkflowRuntimeError::InvalidWorkflow { issues })
                if issues.contains("missing_agent_output_type_for_field_reference")
                    && issues.contains("Agent `greeting` must declare `output`")
                    && issues.contains("output declaration")
        ));
    }

    #[test]
    fn validation_stage_renders_source_snippet_with_arrow() {
        let workflow_source = r#"
            agent greeting {
                prompt: "first"
            }

            agent greeting {
                prompt: "second"
            }
        "#;

        let validate_result = WorkflowPipeline::parse(WorkflowPipelineInput::Source(workflow_source))
            .expect("parse stage should succeed")
            .normalize()
            .validate();

        assert!(matches!(
            validate_result,
            Err(WorkflowRuntimeError::InvalidWorkflow { issues })
                if issues.contains("duplicate_agent")
                    && issues.contains("agent greeting")
                    && issues.contains("<workflow>:")
        ));
    }

    #[test]
    fn parse_stage_renders_source_snippet_with_arrow() {
        let broken_workflow_source = "agent greeting {\n    prompt: \"hello\"\n}\n@\n";

        let parse_result = WorkflowPipeline::parse(WorkflowPipelineInput::Source(broken_workflow_source));

        assert!(matches!(
            parse_result,
            Err(WorkflowRuntimeError::ParseFailed { details, source: _ })
                if details.contains("parse_error")
                    && details.contains("<workflow>:")
                    && details.contains("here")
                    && !details.contains("-->")
                    && !details.contains("SourceSpan {")
        ));
    }

    #[test]
    fn parse_stage_formats_expected_agent_properties_without_custom_property() {
        let broken_workflow_source = r#"
            agent greeting {
                a prompt: "hello"
                output: string
            }
        "#;

        let parse_result = WorkflowPipeline::parse(WorkflowPipelineInput::Source(broken_workflow_source));

        assert!(matches!(
            parse_result,
            Err(WorkflowRuntimeError::ParseFailed { details, source: _ })
                if details.contains("`model`")
                    && details.contains("`prompt`")
                    && details.contains("`output`")
                    && !details.contains("`custom`")
                    && !details.contains("property")
                    && !details.contains("SourceSpan {")
        ));
    }
}
