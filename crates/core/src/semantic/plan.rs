use crate::dsl::{AgentDeclaration, Expression, ObjectField, OutputDeclaration, Workflow};
use crate::semantic::ir::{TypedToolIr, TypedWorkflowIr};
use crate::semantic::support::provider::{build_provider_index, ProviderConfigTemplate};
use crate::semantic::support::types::{validate_value_against_type, WorkflowType};
use crate::semantic::WorkflowSemanticError;
use serde_json::Value;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone)]
pub struct PlannedAgent {
    pub name: String,
    pub declaration: AgentDeclaration,
    pub provider_name: String,
    pub model_id_expression: Expression,
    pub inference_fields: Vec<ObjectField>,
    pub iteration_output_type: WorkflowType,
    pub final_output_type: WorkflowType,
    pub iteration_output_schema: Value,
    pub final_output_schema: Value,
    pub dependencies: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct PlannedMcpImport {
    pub name: String,
    pub kind: PlannedMcpImportKind,
    pub server_name: String,
    pub item_name: String,
}

#[derive(Debug, Clone)]
pub enum PlannedMcpImportKind {
    Prompt,
    Resource,
}

#[derive(Debug, Clone)]
pub struct ExecutionPlan {
    pub provider_index: HashMap<String, ProviderConfigTemplate>,
    pub input_type: Option<WorkflowType>,
    pub secrets_type: Option<WorkflowType>,
    pub output_declaration: OutputDeclaration,
    pub workflow_output_type: WorkflowType,
    pub tools: HashMap<String, TypedToolIr>,
    pub agent_execution_order: Vec<String>,
    pub planned_agents: HashMap<String, PlannedAgent>,
    pub mcp_imports: Vec<PlannedMcpImport>,
}

impl PlannedAgent {
    #[must_use]
    pub fn iteration_output_schema(&self) -> Value {
        self.iteration_output_schema.clone()
    }

    pub fn validate_iteration_output_value(&self, output: &Value) -> Result<(), String> {
        validate_value_against_type(output, &self.iteration_output_type)
    }
}

impl ExecutionPlan {
    pub fn agent_execution_batches(&self) -> Result<Vec<Vec<String>>, WorkflowSemanticError> {
        let execution_order = &self.agent_execution_order;
        let mut unresolved_agents = execution_order.iter().cloned().collect::<HashSet<_>>();
        let mut resolved_agents = HashSet::<String>::new();
        let mut execution_batches = Vec::<Vec<String>>::new();

        while !unresolved_agents.is_empty() {
            let mut ready_agents = Vec::<String>::new();

            for agent_name in execution_order {
                if !unresolved_agents.contains(agent_name) {
                    continue;
                }

                let planned_agent = self
                    .planned_agents
                    .get(agent_name)
                    .ok_or_else(|| WorkflowSemanticError::ExecutionPlanInvariant {
                        message: format!("planned agent `{agent_name}` is missing"),
                    })?;

                if planned_agent
                    .dependencies
                    .iter()
                    .any(|dependency_name| !resolved_agents.contains(dependency_name))
                {
                    continue;
                }

                ready_agents.push(agent_name.clone());
            }

            if ready_agents.is_empty() {
                let mut blocked_agents = unresolved_agents.iter().cloned().collect::<Vec<_>>();
                blocked_agents.sort();

                return Err(WorkflowSemanticError::ExecutionPlanInvariant {
                    message: format!("failed to resolve execution batches; blocked agents: {}", blocked_agents.join(", ")),
                });
            }

            for ready_agent_name in &ready_agents {
                unresolved_agents.remove(ready_agent_name);
                resolved_agents.insert(ready_agent_name.clone());
            }

            execution_batches.push(ready_agents);
        }

        Ok(execution_batches)
    }
}

pub fn build_execution_plan(workflow: &Workflow, typed_workflow_ir: &TypedWorkflowIr) -> Result<ExecutionPlan, WorkflowSemanticError> {
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
                model_id_expression: typed_agent.model_id_expression.clone(),
                inference_fields: typed_agent.inference_fields.clone(),
                iteration_output_type: typed_agent.iteration_output_type.clone(),
                final_output_type: typed_agent.final_output_type.clone(),
                iteration_output_schema: typed_agent.iteration_output_schema.clone(),
                final_output_schema: typed_agent.final_output_schema.clone(),
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
        tools: typed_workflow_ir
            .tools
            .iter()
            .map(|typed_tool| (typed_tool.name.clone(), typed_tool.clone()))
            .collect(),
        agent_execution_order,
        planned_agents,
        mcp_imports: collect_mcp_imports(workflow),
    })
}

fn collect_mcp_imports(workflow: &Workflow) -> Vec<PlannedMcpImport> {
    let mut imports = Vec::new();

    for prompt_import in workflow.prompt_imports() {
        imports.push(PlannedMcpImport {
            name: prompt_import.name.clone(),
            kind: PlannedMcpImportKind::Prompt,
            server_name: prompt_import.source.server_name.clone(),
            item_name: prompt_import.source.item_name.clone(),
        });
    }

    for resource_import in workflow.resource_imports() {
        imports.push(PlannedMcpImport {
            name: resource_import.name.clone(),
            kind: PlannedMcpImportKind::Resource,
            server_name: resource_import.source.server_name.clone(),
            item_name: resource_import.source.item_name.clone(),
        });
    }

    imports
}

fn validate_ir_planner_invariants(
    typed_workflow_ir: &TypedWorkflowIr,
    provider_index: &HashMap<String, ProviderConfigTemplate>,
) -> Result<(), WorkflowSemanticError> {
    let declared_agent_names = typed_workflow_ir
        .agents
        .iter()
        .map(|typed_agent| typed_agent.name.clone())
        .collect::<HashSet<_>>();

    for typed_agent in &typed_workflow_ir.agents {
        if !provider_index.contains_key(&typed_agent.provider_name) {
            return Err(WorkflowSemanticError::ExecutionPlanInvariant {
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

            return Err(WorkflowSemanticError::ExecutionPlanInvariant {
                message: format!("agent `{}` depends on unknown agent `{}`", typed_agent.name, dependency_name),
            });
        }
    }

    Ok(())
}

fn resolve_agent_execution_order(typed_workflow_ir: &TypedWorkflowIr) -> Result<Vec<String>, WorkflowSemanticError> {
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

        return Err(WorkflowSemanticError::ExecutionPlanInvariant {
            message: format!("failed to resolve execution order; blocked agents: {}", blocked_agents.join(", ")),
        });
    }

    Ok(ordered_agents)
}

#[cfg(test)]
mod tests {
    use super::build_execution_plan;
    use crate::parse_inline_workflow;
    use crate::semantic::build_typed_workflow_ir;
    use crate::semantic::WorkflowSemanticError;
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
            provider openai from openai {
                endpoint: "https://api.openai.com/v1"
                api_key: "test-api-key"
            }

            model openai_model from openai {
                id: "model-a"
            }

            input {
                topic: string
            }

            agent first {
                model: model.openai_model
                instruction: input.topic
                output {
                    value: string
                }
            }

            agent second {
                model: model.openai_model
                instruction: agent.first.value
                output {
                    value: string
                }
            }

            output {
                value: agent.second.value
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
    fn resolves_agent_execution_batches_without_execution_state() {
        let workflow = parse_inline_workflow! {
            provider openai from openai {
                endpoint: "https://api.openai.com/v1"
                api_key: "test-api-key"
            }

            model openai_model from openai {
                id: "model-a"
            }

            input {
                topic: string
            }

            agent first {
                model: model.openai_model
                instruction: input.topic
                output {
                    value: string
                }
            }

            agent second {
                model: model.openai_model
                instruction: input.topic
                output {
                    value: string
                }
            }

            agent final {
                model: model.openai_model
                instruction: agent.first.value
                output {
                    value: string
                }
            }

            output {
                value: agent.final.value
            }
        };

        let typed_workflow_ir = build_typed_workflow_ir::<Input, Output>(&workflow).expect("typecheck should succeed");
        let execution_plan = build_execution_plan(&workflow, &typed_workflow_ir).expect("planning should succeed");
        let execution_batches = execution_plan.agent_execution_batches().expect("batch planning should succeed");

        assert_eq!(
            execution_batches,
            vec![vec!["first".to_string(), "second".to_string()], vec!["final".to_string()]]
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
            Err(WorkflowSemanticError::ExecutionPlanInvariant { message })
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
            Err(WorkflowSemanticError::ExecutionPlanInvariant { message })
                if message.contains("ghost")
        ));
    }
}
