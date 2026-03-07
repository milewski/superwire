use crate::ast::*;
use anyhow::{Result, anyhow};
use petgraph::graph::{DiGraph, NodeIndex};
use petgraph::algo::toposort;
use std::collections::HashMap;

pub struct DependencyGraph {
    graph: DiGraph<String, ()>,
    node_map: HashMap<String, NodeIndex>,
}

impl DependencyGraph {
    pub fn new() -> Self {
        Self {
            graph: DiGraph::new(),
            node_map: HashMap::new(),
        }
    }

    pub fn build_from_document(doc: &Document) -> Result<Self> {
        let mut dep_graph = Self::new();

        // Add all agents as nodes
        for agent_name in doc.agents.keys() {
            dep_graph.add_node(agent_name.clone());
        }

        // Add edges based on dependencies
        for (agent_name, agent) in &doc.agents {
            let dependencies = extract_dependencies(agent);
            for dep in dependencies {
                if !doc.agents.contains_key(&dep) {
                    return Err(anyhow!("Agent '{}' depends on undefined agent '{}'", agent_name, dep));
                }
                dep_graph.add_edge(&dep, agent_name)?;
            }
        }

        // Check for cycles
        dep_graph.check_cycles()?;

        Ok(dep_graph)
    }

    fn add_node(&mut self, name: String) {
        let idx = self.graph.add_node(name.clone());
        self.node_map.insert(name, idx);
    }

    fn add_edge(&mut self, from: &str, to: &str) -> Result<()> {
        let from_idx = self.node_map.get(from)
            .ok_or_else(|| anyhow!("Node '{}' not found", from))?;
        let to_idx = self.node_map.get(to)
            .ok_or_else(|| anyhow!("Node '{}' not found", to))?;

        self.graph.add_edge(*from_idx, *to_idx, ());
        Ok(())
    }

    fn check_cycles(&self) -> Result<()> {
        if let Err(_) = toposort(&self.graph, None) {
            return Err(anyhow!("Cyclic dependency detected in agent graph"));
        }
        Ok(())
    }

    pub fn topological_order(&self) -> Result<Vec<String>> {
        let sorted = toposort(&self.graph, None)
            .map_err(|_| anyhow!("Cyclic dependency detected"))?;

        Ok(sorted
            .into_iter()
            .map(|idx| self.graph[idx].clone())
            .collect())
    }
}

fn extract_dependencies(agent: &Agent) -> Vec<String> {
    let mut deps = Vec::new();

    // Check context references
    if let Some(context_ref) = &agent.context {
        let dep = match context_ref {
            ContextRef::Full(name) | ContextRef::Summary(name) => name.clone(),
        };
        deps.push(dep);
    }

    // Check prompt for variable references
    extract_prompt_dependencies(&agent.prompt, &mut deps);

    // Check for_each collection references
    if let Some(for_each) = &agent.for_each {
        if let Expression::Reference(ref_name) = &for_each.collection {
            // Extract agent name from reference like "agent.field"
            if let Some(agent_name) = ref_name.split('.').next() {
                deps.push(agent_name.to_string());
            }
        }
    }

    deps
}

fn extract_prompt_dependencies(prompt: &PromptValue, deps: &mut Vec<String>) {
    match prompt {
        PromptValue::Inline(s) | PromptValue::Multiline(s) => {
            extract_string_dependencies(s, deps);
        }
        PromptValue::Function(func) => {
            for arg in func.args.values() {
                match arg {
                    FunctionArg::String(s) => extract_string_dependencies(s, deps),
                    FunctionArg::Function(nested) => {
                        for nested_arg in nested.args.values() {
                            if let FunctionArg::String(s) = nested_arg {
                                extract_string_dependencies(s, deps);
                            }
                        }
                    }
                }
            }
        }
    }
}

fn extract_string_dependencies(s: &str, deps: &mut Vec<String>) {
    // Parse {{ variable }} references
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '{' {
            if let Some(&'{') = chars.peek() {
                chars.next(); // consume second {
                let mut var_name = String::new();
                while let Some(c) = chars.next() {
                    if c == '}' {
                        if let Some(&'}') = chars.peek() {
                            chars.next(); // consume second }
                            let trimmed = var_name.trim();
                            // Extract agent name from references like "agent.field"
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
