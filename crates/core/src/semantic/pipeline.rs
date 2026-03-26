use crate::diagnostic::{render_diagnostics_for_cli, Diagnostic};
use crate::dsl::{parse_workflow, validate_workflow, ValidationReport, Workflow};
use crate::runtime::error::WorkflowRuntimeError;
use crate::runtime::types::WorkflowType;
use crate::semantic::ir::{build_typed_workflow_ir, build_typed_workflow_ir_dynamic, TypedWorkflowIr};
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
            WorkflowPipelineInput::Source(source) => {
                parse_workflow(source).map_err(|source| WorkflowRuntimeError::ParseFailed { source })?
            }
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

    pub fn typecheck_dynamic(self) -> Result<WorkflowPipeline<TypecheckStageOutput>, WorkflowRuntimeError> {
        Ok(WorkflowPipeline::new(self.state.typecheck_dynamic()?))
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

    pub fn typecheck_dynamic(self) -> Result<TypecheckStageOutput, WorkflowRuntimeError> {
        let typed_workflow_ir = build_typed_workflow_ir_dynamic(&self.workflow)?;

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

    #[must_use]
    pub fn input_type(&self) -> Option<&WorkflowType> {
        self.typed_workflow_ir.input_type.as_ref()
    }

    #[must_use]
    pub fn output_type(&self) -> &WorkflowType {
        &self.typed_workflow_ir.workflow_output_type
    }
}

#[derive(Debug, Clone)]
pub struct DynamicCompiledWorkflow {
    plan_stage_output: PlanStageOutput,
}

impl DynamicCompiledWorkflow {
    #[must_use]
    pub fn workflow(&self) -> &Workflow {
        self.plan_stage_output.workflow()
    }

    #[must_use]
    pub fn validation_report(&self) -> &ValidationReport {
        self.plan_stage_output.validation_report()
    }

    #[must_use]
    pub fn validation_diagnostics(&self) -> Vec<Diagnostic> {
        self.plan_stage_output.validation_report().diagnostics()
    }

    #[must_use]
    pub fn typed_workflow_ir(&self) -> &TypedWorkflowIr {
        self.plan_stage_output.typed_workflow_ir()
    }

    #[must_use]
    pub fn execution_plan(&self) -> &ExecutionPlan {
        self.plan_stage_output.execution_plan()
    }

    #[must_use]
    pub fn input_type(&self) -> Option<&WorkflowType> {
        self.plan_stage_output.input_type()
    }

    #[must_use]
    pub fn output_type(&self) -> &WorkflowType {
        self.plan_stage_output.output_type()
    }

    #[must_use]
    pub fn into_plan_stage_output(self) -> PlanStageOutput {
        self.plan_stage_output
    }

    #[must_use]
    pub fn into_execution_plan(self) -> ExecutionPlan {
        self.plan_stage_output.into_execution_plan()
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

pub fn compile_workflow_pipeline_dynamic(workflow_input: WorkflowPipelineInput<'_>) -> Result<PlanStageOutput, WorkflowRuntimeError> {
    WorkflowPipeline::parse(workflow_input)?
        .normalize()
        .validate()?
        .typecheck_dynamic()?
        .plan()
        .map(WorkflowPipeline::into_state)
}

pub fn compile_dynamic_workflow(workflow_input: WorkflowPipelineInput<'_>) -> Result<DynamicCompiledWorkflow, WorkflowRuntimeError> {
    let plan_stage_output = compile_workflow_pipeline_dynamic(workflow_input)?;

    Ok(DynamicCompiledWorkflow { plan_stage_output })
}

fn render_validation_report(validation_report: &ValidationReport) -> String {
    let validation_diagnostics = validation_report.diagnostics();

    render_diagnostics_for_cli(&validation_diagnostics, None)
}

#[cfg(test)]
mod tests {
    use super::{
        compile_dynamic_workflow, compile_workflow_pipeline, compile_workflow_pipeline_dynamic, WorkflowPipeline, WorkflowPipelineInput,
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
            Err(WorkflowRuntimeError::InvalidWorkflow { issues }) if issues.contains("unknown_input_field_reference")
        ));
    }

    #[test]
    fn dynamic_pipeline_reports_parse_failures() {
        let compile_result = compile_workflow_pipeline_dynamic(WorkflowPipelineInput::Source("agent broken {"));

        assert!(matches!(compile_result, Err(WorkflowRuntimeError::ParseFailed { .. })));
    }

    #[test]
    fn dynamic_pipeline_reports_validation_failures() {
        let workflow = parse_inline_workflow! {
            input {
                known_field: string
            }

            output {
                broken: input.missing_field
            }
        };

        let compile_result = compile_workflow_pipeline_dynamic(WorkflowPipelineInput::Workflow(&workflow));

        assert!(matches!(
            compile_result,
            Err(WorkflowRuntimeError::InvalidWorkflow { issues }) if issues.contains("unknown_input_field_reference")
        ));
    }

    #[test]
    fn dynamic_pipeline_reports_dependency_cycles_during_validation() {
        let workflow = parse_inline_workflow! {
            provider openai {
                driver: "openai"
                endpoint: "https://api.openai.com/v1"
                api_key: "test-api-key"
                models: ["model-a"]
            }

            agent first {
                model: openai("model-a")
                prompt: agent.second
                output: string
            }

            agent second {
                model: openai("model-a")
                prompt: agent.first
                output: string
            }

            output {
                value: agent.first
            }
        };

        let compile_result = compile_workflow_pipeline_dynamic(WorkflowPipelineInput::Workflow(&workflow));

        assert!(matches!(
            compile_result,
            Err(WorkflowRuntimeError::InvalidWorkflow { issues }) if issues.contains("agent_dependency_cycle")
        ));
    }

    #[test]
    fn dynamic_pipeline_reports_typecheck_failures() {
        let workflow = parse_inline_workflow! {
            provider openai {
                driver: "openai"
                endpoint: "https://api.openai.com/v1"
                api_key: "test-api-key"
                models: ["model-a", "model-b"]
            }

            agent writer {
                model: openai("model-a", model: "model-b")
                prompt: "hello"
                output: string
            }

            output {
                value: agent.writer
            }
        };

        let compile_result = compile_workflow_pipeline_dynamic(WorkflowPipelineInput::Workflow(&workflow));

        assert!(matches!(
            compile_result,
            Err(WorkflowRuntimeError::InvalidAgentProperty {
                property,
                message,
                ..
            }) if property == "model" && message.contains("ambiguous model name arguments")
        ));
    }

    #[test]
    fn dynamic_pipeline_reports_missing_output_declaration_typecheck_failures() {
        let workflow = parse_inline_workflow! {
            provider openai {
                driver: "openai"
                endpoint: "https://api.openai.com/v1"
                api_key: "test-api-key"
                models: ["model-a"]
            }

            agent writer {
                model: openai("model-a")
                prompt: "hello"
                output: string
            }
        };

        let compile_result = compile_workflow_pipeline_dynamic(WorkflowPipelineInput::Workflow(&workflow));

        assert!(matches!(
            compile_result,
            Err(WorkflowRuntimeError::MissingDeclaration { message }) if message.contains("`output` block")
        ));
    }

    #[test]
    fn dynamic_pipeline_reports_plan_failures() {
        let workflow = parse_inline_workflow! {
            provider openai {
                driver: "openai"
                api_key: "test-api-key"
                models: ["model-a"]
            }

            agent writer {
                model: openai("model-a")
                prompt: "hello"
                output: string
            }

            output {
                value: agent.writer
            }
        };

        let compile_result = compile_workflow_pipeline_dynamic(WorkflowPipelineInput::Workflow(&workflow));

        assert!(matches!(
            compile_result,
            Err(WorkflowRuntimeError::ProviderConfiguration { provider_name, message })
                if provider_name == "openai" && message.contains("missing `endpoint` property")
        ));
    }

    #[test]
    fn dynamic_compiler_returns_reusable_artifact() {
        let compiled_workflow = compile_dynamic_workflow(WorkflowPipelineInput::Workflow(&VALID_WORKFLOW)).unwrap();

        assert_eq!(
            compiled_workflow.execution_plan().agent_execution_order,
            vec!["first".to_string(), "second".to_string()]
        );

        assert!(compiled_workflow.input_type().is_some());
        assert!(compiled_workflow.validation_diagnostics().is_empty());
    }
}
