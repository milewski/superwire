use crate::ast::{Agent, AgentProperty, NamedSchema, Reference, Value, Workflow};
use crate::execution::context::RuntimeContext;
use crate::execution::error::ExecutionError;
use crate::execution::orchestrator::AgentOrchestrator;
use crate::parser::DependencyGraph;
use crate::providers::{ProviderRef, ProviderRegistry};
use crate::tools::ToolRegistry;
use serde_json::Value as JsonValue;
use std::collections::HashMap;

type AgentTaskHandle = tokio::task::JoinHandle<Result<(String, JsonValue, Vec<crate::providers::provider::Message>), ExecutionError>>;

pub struct WorkflowExecutor<'a> {
    workflow: &'a Workflow,
    provider_registry: &'a ProviderRegistry,
    tool_registry: &'a ToolRegistry,
}

impl<'a> WorkflowExecutor<'a> {
    #[must_use]
    pub fn new(workflow: &'a Workflow, provider_registry: &'a ProviderRegistry, tool_registry: &'a ToolRegistry) -> Self {
        Self {
            workflow,
            provider_registry,
            tool_registry,
        }
    }

    pub async fn execute(&self, inputs: HashMap<String, JsonValue>) -> Result<JsonValue, ExecutionError> {
        log::info!("Building dependency graph");

        let dependency_graph = DependencyGraph::build(self.workflow).map_err(|error| ExecutionError::RuntimeError {
            agent: "workflow".to_string(),
            message: format!("Failed to build dependency graph: {error}"),
            suggestion: Some("Check for circular dependencies".to_string()),
        })?;

        let execution_levels = dependency_graph.get_execution_levels();
        log::info!("Execution levels determined: {execution_levels:?}");

        let mut runtime_context = RuntimeContext::new();

        for (field_name, value) in inputs {
            runtime_context.set_input_value(field_name, value);
        }

        self.execute_levels(&execution_levels, &mut runtime_context).await?;

        log::info!("All agents executed successfully");

        self.build_final_output(&runtime_context)
    }

    async fn execute_levels(&self, execution_levels: &[Vec<String>], runtime_context: &mut RuntimeContext) -> Result<(), ExecutionError> {
        for level in execution_levels {
            log::info!("Executing level with {} agent(s): {:?}", level.len(), level);

            let mut tasks = Vec::new();

            for agent_name in level {
                let agent = self.find_agent(agent_name)?;
                let task = self.spawn_agent_task(agent, runtime_context)?;
                tasks.push(task);
            }

            self.collect_task_results(tasks, runtime_context).await?;
        }

        Ok(())
    }

    fn find_agent(&self, agent_name: &str) -> Result<&Agent, ExecutionError> {
        self.workflow
            .agents
            .iter()
            .find(|agent| agent.name == agent_name)
            .ok_or_else(|| ExecutionError::RuntimeError {
                agent: agent_name.to_string(),
                message: "Agent not found in workflow".to_string(),
                suggestion: None,
            })
    }

    fn spawn_agent_task(&self, agent: &Agent, runtime_context: &RuntimeContext) -> Result<AgentTaskHandle, ExecutionError> {
        let agent_clone = agent.clone();
        let provider_registry_clone = self.provider_registry.clone();
        let runtime_context_clone = runtime_context.clone();
        let tool_registry_clone = self.tool_registry.clone();
        let schemas_clone = self.workflow.schemas.clone();

        log::debug!("Spawning task for agent '{}', schemas count: {}", agent.name, schemas_clone.len());

        let task = tokio::task::spawn(async move {
            let provider = Self::get_provider_for_agent(&agent_clone, &provider_registry_clone)?;
            let provider_clone = provider.clone();

            let initial_context = Self::extract_initial_context(&agent_clone, &runtime_context_clone)?;
            let for_each_property = Self::extract_for_each_property(&agent_clone);

            if let Some((collection_value, iteration_var)) = for_each_property {
                Self::execute_for_each(
                    &agent_clone,
                    collection_value,
                    iteration_var,
                    &initial_context,
                    &runtime_context_clone,
                    &provider_clone,
                    &tool_registry_clone,
                    &schemas_clone,
                )
                .await
            } else {
                let orchestrator = AgentOrchestrator::with_schemas(provider, tool_registry_clone.clone(), schemas_clone);
                let (output, context) = orchestrator
                    .execute_agent(&agent_clone, initial_context, &runtime_context_clone)
                    .await?;

                Ok((agent_clone.name.clone(), output, context))
            }
        });

        Ok(task)
    }

    fn get_provider_for_agent(agent: &Agent, provider_registry: &ProviderRegistry) -> Result<ProviderRef, ExecutionError> {
        let model_property = agent.properties.iter().find_map(|prop| {
            if let AgentProperty::Model { value, .. } = prop {
                Some(value)
            } else {
                None
            }
        });

        let model_string = model_property
            .and_then(|value| {
                if let Value::String(string) = value {
                    Some(string.as_str())
                } else {
                    None
                }
            })
            .ok_or_else(|| ExecutionError::RuntimeError {
                agent: agent.name.clone(),
                message: "Agent does not have a model property".to_string(),
                suggestion: Some("Add a model property to the agent".to_string()),
            })?;

        let parts: Vec<&str> = model_string.split('/').collect();
        if parts.len() != 2 {
            return Err(ExecutionError::RuntimeError {
                agent: agent.name.clone(),
                message: format!("Invalid model format: {model_string}"),
                suggestion: Some("Model should be in format 'provider/model'".to_string()),
            });
        }

        let provider_name = parts[0];

        provider_registry.get(provider_name).map_err(|error| ExecutionError::ProviderError {
            agent: agent.name.clone(),
            message: format!("Provider '{provider_name}' not found: {error}"),
            suggestion: Some("Check that the provider is defined in the workflow".to_string()),
        })
    }

    fn extract_initial_context(
        agent: &Agent,
        runtime_context: &RuntimeContext,
    ) -> Result<Vec<crate::providers::provider::Message>, ExecutionError> {
        let context_property = agent.properties.iter().find_map(|prop| {
            if let AgentProperty::Context { value, .. } = prop {
                Some(value)
            } else {
                None
            }
        });

        if let Some(context_value) = context_property {
            let resolved = runtime_context.resolve_value(context_value)?;

            if let JsonValue::Array(messages) = resolved {
                Ok(messages.into_iter().filter_map(|msg| serde_json::from_value(msg).ok()).collect())
            } else {
                Ok(Vec::new())
            }
        } else {
            Ok(Vec::new())
        }
    }

    fn extract_for_each_property(agent: &Agent) -> Option<(&Value, &String)> {
        agent.properties.iter().find_map(|prop| {
            if let AgentProperty::ForEach {
                collection, identifier, ..
            } = prop
            {
                Some((collection, identifier))
            } else {
                None
            }
        })
    }

    async fn execute_for_each(
        agent: &Agent,
        collection_value: &Value,
        iteration_var: &str,
        initial_context: &[crate::providers::provider::Message],
        runtime_context: &RuntimeContext,
        provider: &ProviderRef,
        tool_registry: &ToolRegistry,
        schemas: &[NamedSchema],
    ) -> Result<(String, JsonValue, Vec<crate::providers::provider::Message>), ExecutionError> {
        let collection = runtime_context.resolve_value(collection_value)?;

        let JsonValue::Array(items) = collection else {
            return Err(ExecutionError::RuntimeError {
                agent: agent.name.clone(),
                message: "for_each collection must be an array".to_string(),
                suggestion: Some("Ensure the collection resolves to an array".to_string()),
            });
        };

        let mut iteration_tasks = Vec::new();

        for (iteration_index, item) in items.into_iter().enumerate() {
            let agent_clone = agent.clone();
            let initial_context_clone = initial_context.to_vec();
            let iteration_context = runtime_context.clone();
            iteration_context.set_input_value(iteration_var.to_string(), item);
            let provider_for_iteration = provider.clone();
            let tool_registry_for_iteration = tool_registry.clone();
            let schemas_for_iteration = schemas.to_vec();

            let iteration_task = tokio::task::spawn(async move {
                let orchestrator_inner =
                    AgentOrchestrator::with_schemas(provider_for_iteration, tool_registry_for_iteration, schemas_for_iteration);
                let result = orchestrator_inner
                    .execute_agent(&agent_clone, initial_context_clone, &iteration_context)
                    .await;

                (iteration_index, result)
            });

            iteration_tasks.push(iteration_task);
        }

        let mut iteration_results = Vec::new();
        for iteration_task in iteration_tasks {
            let (iteration_index, result) = iteration_task.await.map_err(|error| ExecutionError::RuntimeError {
                agent: agent.name.clone(),
                message: format!("Failed to execute for_each iteration: {error}"),
                suggestion: None,
            })?;

            let (output, context) = result?;
            iteration_results.push((iteration_index, output, context));
        }

        iteration_results.sort_by_key(|(index, _, _)| *index);

        let mut results = Vec::new();
        let mut all_contexts = Vec::new();
        for (_, output, context) in iteration_results {
            results.push(output);
            all_contexts.extend(context);
        }

        Ok((agent.name.clone(), JsonValue::Array(results), all_contexts))
    }

    async fn collect_task_results(&self, tasks: Vec<AgentTaskHandle>, runtime_context: &mut RuntimeContext) -> Result<(), ExecutionError> {
        for task in tasks {
            let (agent_name, output, context) = task.await.map_err(|error| ExecutionError::RuntimeError {
                agent: "parallel_execution".to_string(),
                message: format!("Failed to execute agent in parallel: {error}"),
                suggestion: None,
            })??;

            runtime_context.set_agent_context(agent_name.clone(), context);
            runtime_context.set_agent_output(agent_name, output);
        }

        Ok(())
    }

    fn build_final_output(&self, runtime_context: &RuntimeContext) -> Result<JsonValue, ExecutionError> {
        let terminal_agents: Vec<_> = self.workflow.agents.iter().filter(|agent| agent.is_terminal).collect();

        if terminal_agents.is_empty() && self.workflow.output.is_none() {
            log::info!("No terminal agents or output block defined, returning null");
            return Ok(JsonValue::Null);
        }

        log::info!("Building final output");

        let mut result = serde_json::Map::new();

        for terminal_agent in terminal_agents {
            if let Ok(output) = runtime_context.resolve_value(&Value::Reference(Reference::Agent {
                agent: terminal_agent.name.clone(),
                field: "_output".to_string(),
            })) {
                result.insert(terminal_agent.name.clone(), output);
            }
        }

        if let Some(output_block) = &self.workflow.output {
            for field in &output_block.fields {
                let value = runtime_context.resolve_value(&field.value)?;
                result.insert(field.name.clone(), value);
            }
        }

        Ok(JsonValue::Object(result))
    }
}
