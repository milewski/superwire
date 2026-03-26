use crate::dsl::{AgentDeclaration, OutputDeclaration, Workflow};
use crate::runtime::error::WorkflowRuntimeError;
use crate::runtime::provider::{build_provider_index, ProviderConfig};
use crate::runtime::types::WorkflowType;
use crate::semantic::ir::TypedWorkflowIr;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone)]
pub struct PlannedAgent {
    pub name: String,
    pub declaration: AgentDeclaration,
    pub provider_name: String,
    pub model_name: String,
    pub iteration_output_type: WorkflowType,
    pub final_output_type: WorkflowType,
    pub dependencies: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ExecutionPlan {
    pub provider_index: HashMap<String, ProviderConfig>,
    pub input_type: Option<WorkflowType>,
    pub secrets_type: Option<WorkflowType>,
    pub output_declaration: OutputDeclaration,
    pub workflow_output_type: WorkflowType,
    pub agent_execution_order: Vec<String>,
    pub planned_agents: HashMap<String, PlannedAgent>,
}

pub fn build_execution_plan(workflow: &Workflow, typed_workflow_ir: &TypedWorkflowIr) -> Result<ExecutionPlan, WorkflowRuntimeError> {
    let provider_index = build_provider_index(workflow)?;

    validate_ir_planner_invariants(typed_workflow_ir, &provider_index)?;

    let agent_execution_order = resolve_agent_execution_order(typed_workflow_ir)?;
    let mut planned_agents = HashMap::new();

    for typed_agent in &typed_workflow_ir.agents {
        planned_agents.insert(
            typed_agent.name.clone(),
            PlannedAgent {
                name: typed_agent.name.clone(),
                declaration: typed_agent.declaration.clone(),
                provider_name: typed_agent.provider_name.clone(),
                model_name: typed_agent.model_name.clone(),
                iteration_output_type: typed_agent.iteration_output_type.clone(),
                final_output_type: typed_agent.final_output_type.clone(),
                dependencies: typed_agent.dependencies.clone(),
            },
        );
    }

    Ok(ExecutionPlan {
        provider_index,
        input_type: typed_workflow_ir.input_type.clone(),
        secrets_type: typed_workflow_ir.secrets_type.clone(),
        output_declaration: typed_workflow_ir.output_declaration.clone(),
        workflow_output_type: typed_workflow_ir.workflow_output_type.clone(),
        agent_execution_order,
        planned_agents,
    })
}

fn validate_ir_planner_invariants(
    typed_workflow_ir: &TypedWorkflowIr,
    provider_index: &HashMap<String, ProviderConfig>,
) -> Result<(), WorkflowRuntimeError> {
    let declared_agent_names = typed_workflow_ir
        .agents
        .iter()
        .map(|typed_agent| typed_agent.name.clone())
        .collect::<HashSet<_>>();

    for typed_agent in &typed_workflow_ir.agents {
        if !provider_index.contains_key(&typed_agent.provider_name) {
            return Err(WorkflowRuntimeError::ExecutionPlanInvariant {
                message: format!(
                    "agent `{}` references provider `{}` that is not declared",
                    typed_agent.name, typed_agent.provider_name
                ),
            });
        }

        for dependency_name in &typed_agent.dependencies {
            if declared_agent_names.contains(dependency_name) {
                continue;
            }

            return Err(WorkflowRuntimeError::ExecutionPlanInvariant {
                message: format!("agent `{}` depends on unknown agent `{}`", typed_agent.name, dependency_name),
            });
        }
    }

    Ok(())
}

fn resolve_agent_execution_order(typed_workflow_ir: &TypedWorkflowIr) -> Result<Vec<String>, WorkflowRuntimeError> {
    let declaration_order = typed_workflow_ir
        .agents
        .iter()
        .map(|typed_agent| typed_agent.name.clone())
        .collect::<Vec<_>>();

    let mut dependency_index = HashMap::<String, HashSet<String>>::new();

    for typed_agent in &typed_workflow_ir.agents {
        let dependencies = typed_agent.dependencies.iter().cloned().collect::<HashSet<_>>();
        dependency_index.insert(typed_agent.name.clone(), dependencies);
    }

    let mut resolved_agents = HashSet::<String>::new();
    let mut ordered_agents = Vec::<String>::new();
    let mut unresolved_agents = declaration_order.iter().cloned().collect::<HashSet<_>>();

    while !unresolved_agents.is_empty() {
        let mut iteration_progress = false;

        for agent_name in &declaration_order {
            if !unresolved_agents.contains(agent_name) {
                continue;
            }

            let dependencies = dependency_index
                .get(agent_name)
                .expect("dependency index should include all agents");

            if dependencies
                .iter()
                .any(|dependency_name| !resolved_agents.contains(dependency_name))
            {
                continue;
            }

            unresolved_agents.remove(agent_name);
            resolved_agents.insert(agent_name.clone());
            ordered_agents.push(agent_name.clone());
            iteration_progress = true;
        }

        if iteration_progress {
            continue;
        }

        let mut blocked_agents = unresolved_agents.into_iter().collect::<Vec<_>>();
        blocked_agents.sort();

        return Err(WorkflowRuntimeError::ExecutionPlanInvariant {
            message: format!("failed to resolve execution order; blocked agents: {}", blocked_agents.join(", ")),
        });
    }

    Ok(ordered_agents)
}

#[cfg(test)]
mod tests {
    use super::build_execution_plan;
    use crate::parse_inline_workflow;
    use crate::runtime::error::WorkflowRuntimeError;
    use crate::semantic::build_typed_workflow_ir;
    use schemars::JsonSchema;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Serialize, JsonSchema)]
    struct Input {
        topic: String,
    }

    #[derive(Debug, Deserialize, JsonSchema)]
    #[allow(dead_code)]
    struct Output {
        value: String,
    }

    fn build_linear_workflow() -> crate::dsl::Workflow {
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
                value: agent.second
            }
        }
    }

    #[test]
    fn resolves_agent_execution_order_from_typed_dependencies() {
        let workflow = build_linear_workflow();
        let typed_workflow_ir = build_typed_workflow_ir::<Input, Output>(&workflow).expect("typecheck should succeed");
        let execution_plan = build_execution_plan(&workflow, &typed_workflow_ir).expect("planning should succeed");

        assert_eq!(
            execution_plan.agent_execution_order,
            vec!["first".to_string(), "second".to_string()]
        );
    }

    #[test]
    fn reports_missing_provider_in_ir_planner_boundary_checks() {
        let workflow = build_linear_workflow();
        let mut typed_workflow_ir = build_typed_workflow_ir::<Input, Output>(&workflow).expect("typecheck should succeed");

        typed_workflow_ir
            .agents
            .first_mut()
            .expect("first agent should exist")
            .provider_name = "missing_provider".to_string();

        let planning_result = build_execution_plan(&workflow, &typed_workflow_ir);

        assert!(matches!(
            planning_result,
            Err(WorkflowRuntimeError::ExecutionPlanInvariant { message })
                if message.contains("missing_provider")
        ));
    }

    #[test]
    fn reports_missing_dependency_in_ir_planner_boundary_checks() {
        let workflow = build_linear_workflow();
        let mut typed_workflow_ir = build_typed_workflow_ir::<Input, Output>(&workflow).expect("typecheck should succeed");

        typed_workflow_ir
            .agents
            .first_mut()
            .expect("first agent should exist")
            .dependencies
            .push("ghost".to_string());

        let planning_result = build_execution_plan(&workflow, &typed_workflow_ir);

        assert!(matches!(
            planning_result,
            Err(WorkflowRuntimeError::ExecutionPlanInvariant { message })
                if message.contains("ghost")
        ));
    }
}
