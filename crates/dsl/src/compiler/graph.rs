use crate::error::WorkflowError;
use petgraph::algo::toposort;
use petgraph::graph::{DiGraph, NodeIndex};
use petgraph::Direction;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone)]
pub struct DependencyGraph {
    graph: DiGraph<String, ()>,
    node_indices: BTreeMap<String, NodeIndex>,
}

impl DependencyGraph {
    pub fn new(dependencies_by_agent: &BTreeMap<String, BTreeSet<String>>) -> Result<Self, WorkflowError> {
        let mut graph = DiGraph::new();
        let mut node_indices = BTreeMap::new();

        for agent_name in dependencies_by_agent.keys() {
            let node_index = graph.add_node(agent_name.clone());
            node_indices.insert(agent_name.clone(), node_index);
        }

        for (agent_name, dependency_names) in dependencies_by_agent {
            let agent_index = node_indices[agent_name];

            for dependency_name in dependency_names {
                let dependency_index = node_indices[dependency_name];
                graph.add_edge(dependency_index, agent_index, ());
            }
        }

        toposort(&graph, None).map_err(|cycle_error| {
            let node_name = graph
                .node_weight(cycle_error.node_id())
                .expect("cycle node should always exist in the graph");

            WorkflowError::validation(format!("agent dependency cycle detected near '{node_name}'"))
        })?;

        Ok(Self { graph, node_indices })
    }

    #[must_use]
    pub fn parallel_stages(&self) -> Vec<Vec<String>> {
        let ordered_nodes = toposort(&self.graph, None).expect("validated graphs should remain acyclic");
        let mut depths_by_node = BTreeMap::new();
        let mut stage_names = BTreeMap::<usize, Vec<String>>::new();

        for node_index in ordered_nodes {
            let depth = self
                .graph
                .neighbors_directed(node_index, Direction::Incoming)
                .map(|dependency_index| depths_by_node[&dependency_index] + 1)
                .max()
                .unwrap_or(0);

            depths_by_node.insert(node_index, depth);
            stage_names.entry(depth).or_default().push(self.graph[node_index].clone());
        }

        for names in stage_names.values_mut() {
            names.sort();
        }

        stage_names.into_values().collect()
    }

    #[must_use]
    pub fn dependencies_for(&self, agent_name: &str) -> BTreeSet<String> {
        let mut dependency_names = BTreeSet::new();
        let node_index = self.node_indices[agent_name];

        for dependency_index in self.graph.neighbors_directed(node_index, Direction::Incoming) {
            dependency_names.insert(self.graph[dependency_index].clone());
        }

        dependency_names
    }
}

#[cfg(test)]
mod tests {
    use super::DependencyGraph;
    use std::collections::{BTreeMap, BTreeSet};

    #[test]
    fn groups_independent_agents_into_parallel_stages() {
        let dependencies = BTreeMap::from([
            ("a".to_string(), BTreeSet::new()),
            ("b".to_string(), BTreeSet::new()),
            ("c".to_string(), BTreeSet::from(["a".to_string(), "b".to_string()])),
        ]);

        let graph = DependencyGraph::new(&dependencies).expect("graph should build");
        let stages = graph.parallel_stages();

        assert_eq!(stages[0], vec!["a".to_string(), "b".to_string()]);
        assert_eq!(stages[1], vec!["c".to_string()]);
    }
}
