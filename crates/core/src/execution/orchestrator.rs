use crate::ast::*;
use crate::execution::{ExecutionEngine, AgentContext};
use crate::parser::DependencyGraph;
use anyhow::Result;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use log::{info, debug, warn};

pub struct Orchestrator {
    engine: Arc<ExecutionEngine>,
}

impl Orchestrator {
    pub fn new(engine: ExecutionEngine) -> Self {
        Self {
            engine: Arc::new(engine)
        }
    }

    pub async fn execute_document(&self, doc: &Document) -> Result<Value> {
        info!("========================================");
        info!("Starting Document Execution");
        info!("========================================");
        info!("Document contains:");
        info!("  - {} agents", doc.agents.len());
        info!("  - {} schemas", doc.schemas.len());
        info!("  - {} providers", doc.providers.len());

        // Build dependency graph
        info!("Building dependency graph...");
        let dep_graph = DependencyGraph::build_from_document(doc)?;
        info!("✓ Dependency graph built successfully");

        let execution_order = dep_graph.topological_order()?;
        info!("Execution order: {:?}", execution_order);

        // Create a shared context wrapped in Arc<Mutex<>>
        let context = Arc::new(Mutex::new(AgentContext::new()));

        // Group agents by dependency level for parallel execution
        let levels = self.compute_execution_levels(&execution_order, doc)?;
        info!("Computed {} execution level(s)", levels.len());

        // Execute each level in parallel
        for (level_idx, level) in levels.iter().enumerate() {
            info!("======== Execution Level {} ========", level_idx + 1);
            info!("Agents in this level: {:?}", level);

            if level.len() == 1 {
                // Single agent - execute directly
                let agent_name = &level[0];
                let agent = doc.agents.get(agent_name)
                    .ok_or_else(|| anyhow::anyhow!("Agent '{}' not found", agent_name))?;

                info!("Executing single agent: {}", agent_name);

                let mut ctx = context.lock().unwrap();

                // Check if this agent has for_each
                if let Some(for_each) = &agent.for_each {
                    info!("Agent has for_each - will execute in parallel over collection");
                    let output = self.execute_agent_with_for_each(agent, for_each, &mut ctx, doc).await?;
                    ctx.set_variable(agent_name.clone(), output);
                } else {
                    let output = self.engine.execute_agent(agent, &mut ctx, &doc.schemas).await?;
                    info!("✓ Agent '{}' completed successfully", agent_name);
                    ctx.set_variable(agent_name.clone(), output);
                }
            } else {
                // Multiple independent agents - execute in parallel
                info!("Executing {} agents in parallel", level.len());

                let mut handles = Vec::new();

                for agent_name in level {
                    let agent = doc.agents.get(agent_name)
                        .ok_or_else(|| anyhow::anyhow!("Agent '{}' not found", agent_name))?
                        .clone();
                    let engine = Arc::clone(&self.engine);
                    let context_clone = Arc::clone(&context);
                    let agent_name_clone = agent_name.clone();
                    let schemas = doc.schemas.clone();

                    let handle = tokio::spawn(async move {
                        info!("Starting parallel execution of agent: {}", agent_name_clone);
                        let mut ctx = context_clone.lock().unwrap().clone();
                        let output = engine.execute_agent(&agent, &mut ctx, &schemas).await?;
                        info!("✓ Parallel agent '{}' completed", agent_name_clone);
                        Ok::<(String, Value), anyhow::Error>((agent_name_clone, output))
                    });

                    handles.push(handle);
                }

                // Wait for all parallel executions to complete
                for handle in handles {
                    let (agent_name, output) = handle.await??;
                    let mut ctx = context.lock().unwrap();
                    ctx.set_variable(agent_name, output);
                }

                info!("✓ All parallel agents in level {} completed", level_idx + 1);
            }
        }

        info!("======== Collecting Results ========");

        // Collect terminal agent outputs
        let ctx = context.lock().unwrap();
        let terminal_agents: Vec<&String> = doc.agents.iter()
            .filter(|(_, agent)| agent.is_terminal)
            .map(|(name, _)| name)
            .collect();

        info!("Terminal agents: {:?}", terminal_agents);

        if terminal_agents.len() == 1 {
            // Single terminal agent - return its output directly
            let agent_name = terminal_agents[0];
            info!("Returning output from single terminal agent: {}", agent_name);
            Ok(ctx.get_output(agent_name)
                .ok_or_else(|| anyhow::anyhow!("Terminal agent '{}' has no output", agent_name))?
                .clone())
        } else {
            // Multiple terminal agents - return as object
            info!("Returning outputs from {} terminal agents", terminal_agents.len());
            let mut result = HashMap::new();
            for agent_name in terminal_agents {
                if let Some(output) = ctx.get_output(agent_name) {
                    result.insert(agent_name.clone(), output.clone());
                }
            }
            Ok(serde_json::to_value(result)?)
        }
    }

    async fn execute_agent_with_for_each(
        &self,
        agent: &Agent,
        for_each: &ForEach,
        context: &mut AgentContext,
        doc: &Document,
    ) -> Result<Value> {
        // Resolve the collection
        let collection = self.resolve_collection(&for_each.collection, context)?;

        // Execute agent for each item in parallel using rayon
        let results: Result<Vec<Value>> = {
            use rayon::prelude::*;

            let engine = Arc::clone(&self.engine);
            let agent_clone = agent.clone();
            let item_name = for_each.item_name.clone();
            let schemas = doc.schemas.clone();

            collection.par_iter().map(|item| {
                // Create a new context for this iteration
                let mut iter_context = context.clone();

                // Set the iteration variable
                iter_context.set_variable(item_name.clone(), item.clone());

                // Execute the agent synchronously in the rayon thread
                // We need to use tokio::runtime::Handle to run async code
                let handle = tokio::runtime::Handle::current();
                handle.block_on(async {
                    engine.execute_agent(&agent_clone, &mut iter_context, &schemas).await
                })
            }).collect()
        };

        // Collect results into an array
        let results = results?;
        Ok(Value::Array(results))
    }

    fn resolve_collection(&self, expr: &Expression, context: &AgentContext) -> Result<Vec<Value>> {
        match expr {
            Expression::Literal(values) => Ok(values.clone()),
            Expression::Reference(ref_name) => {
                // Parse reference like "agent.field"
                let parts: Vec<&str> = ref_name.split('.').collect();
                if parts.len() != 2 {
                    return Err(anyhow::anyhow!("Invalid collection reference: '{}'", ref_name));
                }

                let agent_name = parts[0];
                let field_name = parts[1];

                let agent_output = context.get_output(agent_name)
                    .ok_or_else(|| anyhow::anyhow!("Agent '{}' output not found", agent_name))?;

                let field_value = agent_output.get(field_name)
                    .ok_or_else(|| anyhow::anyhow!("Field '{}' not found in agent '{}' output", field_name, agent_name))?;

                if let Value::Array(arr) = field_value {
                    Ok(arr.clone())
                } else {
                    Err(anyhow::anyhow!("Field '{}' is not an array", field_name))
                }
            }
        }
    }

    fn compute_execution_levels(&self, execution_order: &[String], doc: &Document) -> Result<Vec<Vec<String>>> {
        let mut levels = Vec::new();
        let mut remaining: Vec<String> = execution_order.to_vec();
        let mut completed = std::collections::HashSet::new();

        while !remaining.is_empty() {
            let mut current_level = Vec::new();

            for agent_name in &remaining {
                let agent = doc.agents.get(agent_name).unwrap();
                let deps = self.get_agent_dependencies(agent);

                // Check if all dependencies are completed
                if deps.iter().all(|dep| completed.contains(dep)) {
                    current_level.push(agent_name.clone());
                }
            }

            if current_level.is_empty() {
                return Err(anyhow::anyhow!("Unable to compute execution levels - possible circular dependency"));
            }

            for agent_name in &current_level {
                completed.insert(agent_name.clone());
                remaining.retain(|n| n != agent_name);
            }

            levels.push(current_level);
        }

        Ok(levels)
    }

    fn get_agent_dependencies(&self, agent: &Agent) -> Vec<String> {
        let mut deps = Vec::new();

        // Check context references
        if let Some(context_ref) = &agent.context {
            let dep = match context_ref {
                ContextRef::Full(name) | ContextRef::Summary(name) => name.clone(),
            };
            deps.push(dep);
        }

        // Check prompt for variable references
        self.extract_prompt_dependencies(&agent.prompt, &mut deps);

        // Check for_each collection references
        if let Some(for_each) = &agent.for_each {
            if let Expression::Reference(ref_name) = &for_each.collection {
                if let Some(agent_name) = ref_name.split('.').next() {
                    deps.push(agent_name.to_string());
                }
            }
        }

        deps
    }

    fn extract_prompt_dependencies(&self, prompt: &PromptValue, deps: &mut Vec<String>) {
        match prompt {
            PromptValue::Inline(s) | PromptValue::Multiline(s) => {
                self.extract_string_dependencies(s, deps);
            }
            PromptValue::Function(func) => {
                for arg in func.args.values() {
                    match arg {
                        FunctionArg::String(s) => self.extract_string_dependencies(s, deps),
                        FunctionArg::Function(nested) => {
                            for nested_arg in nested.args.values() {
                                if let FunctionArg::String(s) = nested_arg {
                                    self.extract_string_dependencies(s, deps);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    fn extract_string_dependencies(&self, s: &str, deps: &mut Vec<String>) {
        let mut chars = s.chars().peekable();
        while let Some(c) = chars.next() {
            if c == '{' {
                if let Some(&'{') = chars.peek() {
                    chars.next();
                    let mut var_name = String::new();
                    while let Some(c) = chars.next() {
                        if c == '}' {
                            if let Some(&'}') = chars.peek() {
                                chars.next();
                                let trimmed = var_name.trim();
                                if let Some(agent_name) = trimmed.split('.').next() {
                                    if !agent_name.is_empty() {
                                        deps.push(agent_name.to_string());
                                    }
                                }
                                break;
                            }
                        }
                        var_name.push(c);
                    }
                }
            }
        }
    }
}
