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
pub struct ParseStageOutput {
    pub workflow: Workflow,
}

#[derive(Debug, Clone)]
pub struct NormalizeStageOutput {
    pub workflow: Workflow,
}

#[derive(Debug, Clone)]
pub struct ValidateStageOutput {
    pub workflow: Workflow,
    pub validation_report: ValidationReport,
}

#[derive(Debug, Clone)]
pub struct TypecheckStageOutput {
    pub workflow: Workflow,
    pub validation_report: ValidationReport,
    pub typed_workflow_ir: TypedWorkflowIr,
}

#[derive(Debug, Clone)]
pub struct PlanStageOutput {
    pub workflow: Workflow,
    pub validation_report: ValidationReport,
    pub typed_workflow_ir: TypedWorkflowIr,
    pub execution_plan: ExecutionPlan,
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
    NormalizeStageOutput {
        workflow: parse_stage_output.workflow,
    }
}

pub fn validate_workflow_stage(normalize_stage_output: NormalizeStageOutput) -> Result<ValidateStageOutput, WorkflowRuntimeError> {
    let validation_report = validate_workflow(&normalize_stage_output.workflow);

    if validation_report.has_issues() {
        return Err(WorkflowRuntimeError::InvalidWorkflow {
            issues: render_validation_report(&validation_report),
        });
    }

    Ok(ValidateStageOutput {
        workflow: normalize_stage_output.workflow,
        validation_report,
    })
}

pub fn typecheck_workflow_stage<Input, Output>(
    validate_stage_output: ValidateStageOutput,
) -> Result<TypecheckStageOutput, WorkflowRuntimeError>
where
    Input: Serialize + JsonSchema,
    Output: DeserializeOwned + JsonSchema,
{
    let typed_workflow_ir = build_typed_workflow_ir::<Input, Output>(&validate_stage_output.workflow)?;

    Ok(TypecheckStageOutput {
        workflow: validate_stage_output.workflow,
        validation_report: validate_stage_output.validation_report,
        typed_workflow_ir,
    })
}

pub fn plan_workflow_stage(typecheck_stage_output: TypecheckStageOutput) -> Result<PlanStageOutput, WorkflowRuntimeError> {
    let execution_plan = build_execution_plan(&typecheck_stage_output.workflow, &typecheck_stage_output.typed_workflow_ir)?;

    Ok(PlanStageOutput {
        workflow: typecheck_stage_output.workflow,
        validation_report: typecheck_stage_output.validation_report,
        typed_workflow_ir: typecheck_stage_output.typed_workflow_ir,
        execution_plan,
    })
}

pub fn compile_workflow_pipeline<Input, Output>(workflow_input: WorkflowPipelineInput<'_>) -> Result<PlanStageOutput, WorkflowRuntimeError>
where
    Input: Serialize + JsonSchema,
    Output: DeserializeOwned + JsonSchema,
{
    let parse_stage_output = parse_workflow_stage(workflow_input)?;
    let normalize_stage_output = normalize_workflow_stage(parse_stage_output);
    let validate_stage_output = validate_workflow_stage(normalize_stage_output)?;
    let typecheck_stage_output = typecheck_workflow_stage::<Input, Output>(validate_stage_output)?;

    plan_workflow_stage(typecheck_stage_output)
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
        validate_workflow_stage, WorkflowPipelineInput,
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
        let parse_stage_output = parse_workflow_stage(WorkflowPipelineInput::Workflow(&VALID_WORKFLOW)).unwrap();
        let normalize_stage_output = normalize_workflow_stage(parse_stage_output);
        let validate_stage_output = validate_workflow_stage(normalize_stage_output).unwrap();
        let typecheck_stage_output = typecheck_workflow_stage::<Input, Output>(validate_stage_output).unwrap();
        let plan_stage_output = plan_workflow_stage(typecheck_stage_output).unwrap();

        assert_eq!(
            plan_stage_output.execution_plan.agent_execution_order,
            vec!["first".to_string(), "second".to_string()]
        );
    }

    #[test]
    fn orchestration_entrypoint_runs_all_stages() {
        let plan_stage_output = compile_workflow_pipeline::<Input, Output>(WorkflowPipelineInput::Workflow(&VALID_WORKFLOW)).unwrap();

        assert_eq!(
            plan_stage_output.execution_plan.agent_execution_order,
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
