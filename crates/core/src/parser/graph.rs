use crate::ast::{Agent, AgentProperty, Reference, Value, Workflow};
use crate::parser::error::ParserError;
use petgraph::algo::is_cyclic_directed;
use petgraph::graph::{DiGraph, NodeIndex};
use petgraph::visit::Topo;
use std::collections::HashMap;

pub struct DependencyGraph {
    graph: DiGraph<String, ()>,
    node_indices: HashMap<String, NodeIndex>,
}

impl DependencyGraph {
    pub fn build(workflow: &Workflow) -> Result<Self, ParserError> {
        let mut graph = DiGraph::new();
        let mut node_indices = HashMap::new();

        for agent in &workflow.agents {
            let node_index = graph.add_node(agent.name.clone());
            node_indices.insert(agent.name.clone(), node_index);
        }

        for agent in &workflow.agents {
            let dependencies = Self::extract_dependencies(agent);

            for dependency in dependencies {
                if let Some(&from_index) = node_indices.get(&dependency) {
                    if let Some(&to_index) = node_indices.get(&agent.name) {
                        graph.add_edge(from_index, to_index, ());
                    }
                }
            }
        }

        if is_cyclic_directed(&graph) {
            return Err(ParserError::syntax_error(
                "workflow".to_string(),
                0,
                0,
                "Cyclic dependency detected in agent graph".to_string(),
                Some("Remove circular dependencies between agents".to_string()),
            ));
        }

        Ok(Self { graph, node_indices })
    }

    pub fn topological_order(&self) -> Vec<String> {
        let mut topo = Topo::new(&self.graph);
        let mut order = Vec::new();

        while let Some(node_index) = topo.next(&self.graph) {
            if let Some(agent_name) = self.graph.node_weight(node_index) {
                order.push(agent_name.clone());
            }
        }

        order
    }

    fn extract_dependencies(agent: &Agent) -> Vec<String> {
        let mut dependencies = Vec::new();

        for property in &agent.properties {
            match property {
                AgentProperty::Model { value, .. } => {
                    Self::extract_references_from_value(value, &mut dependencies);
                }
                AgentProperty::Tools { value, .. } => {
                    Self::extract_references_from_value(value, &mut dependencies);
                }
                AgentProperty::Context { value, .. } => {
                    Self::extract_references_from_value(value, &mut dependencies);
                }
                AgentProperty::Prompt { value, .. } => {
                    Self::extract_references_from_value(value, &mut dependencies);
                }
                AgentProperty::ForEach { collection, .. } => {
                    Self::extract_references_from_value(collection, &mut dependencies);
                }
                AgentProperty::Output { .. } => {}
            }
        }

        dependencies
    }

    fn extract_references_from_value(value: &Value, dependencies: &mut Vec<String>) {
        match value {
            Value::Reference(reference) => match reference {
                Reference::Agent { agent, .. } => {
                    if !dependencies.contains(agent) {
                        dependencies.push(agent.clone());
                    }
                }
                Reference::AgentContext { agent } => {
                    if !dependencies.contains(agent) {
                        dependencies.push(agent.clone());
                    }
                }
                _ => {}
            },
            Value::Interpolated(template) => {
                let interpolation_pattern = regex::Regex::new(r"\{\{([^}]+)\}\}").unwrap();

                for capture in interpolation_pattern.captures_iter(template) {
                    let reference_text = capture[1].trim();
                    let parts: Vec<&str> = reference_text.split('.').collect();

                    let agent_name = if parts.len() == 1 {
                        parts[0].to_string()
                    } else if parts[0] == "agent" && parts.len() >= 2 {
                        parts[1].to_string()
                    } else if parts[0] != "input" {
                        parts[0].to_string()
                    } else {
                        continue;
                    };

                    if !dependencies.contains(&agent_name) {
                        dependencies.push(agent_name);
                    }
                }
            }
            Value::Array(values) => {
                for val in values {
                    Self::extract_references_from_value(val, dependencies);
                }
            }
            Value::Object(map) => {
                for val in map.values() {
                    Self::extract_references_from_value(val, dependencies);
                }
            }
            Value::FunctionCall(func_call) => {
                for val in func_call.arguments.values() {
                    Self::extract_references_from_value(val, dependencies);
                }
            }
            _ => {}
        }
    }
}
