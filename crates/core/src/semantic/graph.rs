use crate::dsl::{AgentExpressionPropertyName, Expression, Reference, ReferenceKeyword, ToolSource, Workflow};
use crate::semantic::plan::{ExecutionPlan, PlannedAgent};
use crate::semantic::support::types::workflow_type_to_json_schema;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashSet;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkflowExecutionGraph {
    pub nodes: Vec<WorkflowExecutionGraphNode>,
    pub edges: Vec<WorkflowExecutionGraphEdge>,
    pub agent_execution_order: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkflowExecutionGraphNode {
    pub id: String,
    pub label: String,
    pub kind: WorkflowExecutionGraphNodeKind,
    pub inputs: Vec<WorkflowExecutionGraphPort>,
    pub outputs: Vec<WorkflowExecutionGraphPort>,
    pub dependencies: Vec<String>,
    pub provider_name: Option<String>,
    pub model: Option<String>,
    pub tools: Vec<WorkflowExecutionGraphTool>,
    pub execution_index: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowExecutionGraphNodeKind {
    Input,
    Agent,
    Output,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkflowExecutionGraphPort {
    pub name: String,
    pub schema: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkflowExecutionGraphTool {
    pub name: String,
    pub kind: WorkflowExecutionGraphToolKind,
    pub server_name: Option<String>,
    pub item_name: Option<String>,
    pub description: Option<String>,
    pub max_calls: Option<u64>,
    pub input_schema: Value,
    pub output_schema: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowExecutionGraphToolKind {
    LocalTool,
    McpTool,
    McpPrompt,
    McpResource,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkflowExecutionGraphEdge {
    pub id: String,
    pub source: String,
    pub target: String,
    pub label: String,
    pub kind: WorkflowExecutionGraphEdgeKind,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowExecutionGraphEdgeKind {
    Input,
    AgentDependency,
    WorkflowOutput,
}

impl ExecutionPlan {
    #[must_use]
    pub fn execution_graph(&self, workflow: &Workflow) -> WorkflowExecutionGraph {
        let mut nodes = vec![self.workflow_input_graph_node()];

        for (agent_index, agent_name) in self.agent_execution_order.iter().enumerate() {
            if let Some(planned_agent) = self.planned_agents.get(agent_name) {
                nodes.push(planned_agent.execution_graph_node(self, workflow, agent_index));
            }
        }

        nodes.push(self.workflow_output_graph_node());

        WorkflowExecutionGraph {
            nodes,
            edges: self.execution_graph_edges(),
            agent_execution_order: self.agent_execution_order.clone(),
        }
    }

    fn workflow_input_graph_node(&self) -> WorkflowExecutionGraphNode {
        let mut outputs = Vec::new();

        if let Some(input_type) = &self.input_type {
            outputs.push(WorkflowExecutionGraphPort {
                name: ReferenceKeyword::Input.as_str().to_string(),
                schema: workflow_type_to_json_schema(input_type),
            });
        }

        if let Some(secrets_type) = &self.secrets_type {
            outputs.push(WorkflowExecutionGraphPort {
                name: ReferenceKeyword::Secrets.as_str().to_string(),
                schema: workflow_type_to_json_schema(secrets_type),
            });
        }

        if outputs.is_empty() {
            outputs.push(WorkflowExecutionGraphPort {
                name: "runtime".to_string(),
                schema: json!({ "type": "object", "additionalProperties": true }),
            });
        }

        WorkflowExecutionGraphNode {
            id: WorkflowExecutionGraphNodeKind::Input.node_id(),
            label: "Runtime input".to_string(),
            kind: WorkflowExecutionGraphNodeKind::Input,
            inputs: Vec::new(),
            outputs,
            dependencies: Vec::new(),
            provider_name: None,
            model: None,
            tools: Vec::new(),
            execution_index: None,
        }
    }

    fn workflow_output_graph_node(&self) -> WorkflowExecutionGraphNode {
        let inputs = self
            .workflow_output_agent_dependencies()
            .into_iter()
            .filter_map(|agent_name| {
                let planned_agent = self.planned_agents.get(&agent_name)?;

                Some(WorkflowExecutionGraphPort {
                    name: format!("agent.{agent_name}"),
                    schema: workflow_type_to_json_schema(&planned_agent.final_output_type),
                })
            })
            .collect::<Vec<_>>();

        WorkflowExecutionGraphNode {
            id: WorkflowExecutionGraphNodeKind::Output.node_id(),
            label: "Workflow output".to_string(),
            kind: WorkflowExecutionGraphNodeKind::Output,
            inputs,
            outputs: vec![WorkflowExecutionGraphPort {
                name: "output".to_string(),
                schema: workflow_type_to_json_schema(&self.workflow_output_type),
            }],
            dependencies: self.workflow_output_agent_dependencies(),
            provider_name: None,
            model: None,
            tools: Vec::new(),
            execution_index: None,
        }
    }

    fn workflow_output_agent_dependencies(&self) -> Vec<String> {
        let mut dependencies = HashSet::new();

        for output_field in &self.output_declaration.fields {
            output_field.value.collect_agent_dependencies(&mut dependencies);
        }

        let mut sorted_dependencies = dependencies.into_iter().collect::<Vec<_>>();
        sorted_dependencies.sort();

        sorted_dependencies
    }

    fn execution_graph_edges(&self) -> Vec<WorkflowExecutionGraphEdge> {
        let mut edges = Vec::new();
        let input_node_id = WorkflowExecutionGraphNodeKind::Input.node_id();
        let output_node_id = WorkflowExecutionGraphNodeKind::Output.node_id();

        for agent_name in &self.agent_execution_order {
            let Some(planned_agent) = self.planned_agents.get(agent_name) else {
                continue;
            };

            if planned_agent.dependencies.is_empty() {
                edges.push(WorkflowExecutionGraphEdge::new(
                    &input_node_id,
                    agent_name,
                    "runtime",
                    WorkflowExecutionGraphEdgeKind::Input,
                ));

                continue;
            }

            for dependency_name in &planned_agent.dependencies {
                edges.push(WorkflowExecutionGraphEdge::new(
                    dependency_name,
                    agent_name,
                    "agent output",
                    WorkflowExecutionGraphEdgeKind::AgentDependency,
                ));
            }
        }

        let output_dependencies = self.workflow_output_agent_dependencies();

        if output_dependencies.is_empty() {
            edges.push(WorkflowExecutionGraphEdge::new(
                &input_node_id,
                &output_node_id,
                "output",
                WorkflowExecutionGraphEdgeKind::WorkflowOutput,
            ));
        } else {
            for dependency_name in output_dependencies {
                edges.push(WorkflowExecutionGraphEdge::new(
                    &dependency_name,
                    &output_node_id,
                    "workflow output",
                    WorkflowExecutionGraphEdgeKind::WorkflowOutput,
                ));
            }
        }

        edges
    }
}

impl PlannedAgent {
    fn execution_graph_node(&self, execution_plan: &ExecutionPlan, workflow: &Workflow, agent_index: usize) -> WorkflowExecutionGraphNode {
        WorkflowExecutionGraphNode {
            id: self.name.clone(),
            label: self.name.clone(),
            kind: WorkflowExecutionGraphNodeKind::Agent,
            inputs: self.execution_graph_inputs(execution_plan),
            outputs: vec![WorkflowExecutionGraphPort {
                name: format!("agent.{}", self.name),
                schema: workflow_type_to_json_schema(&self.final_output_type),
            }],
            dependencies: self.dependencies.clone(),
            provider_name: Some(self.provider_name.clone()),
            model: self.model_name(),
            tools: self.execution_graph_tools(execution_plan, workflow),
            execution_index: Some(agent_index),
        }
    }

    fn execution_graph_inputs(&self, execution_plan: &ExecutionPlan) -> Vec<WorkflowExecutionGraphPort> {
        self.dependencies
            .iter()
            .filter_map(|dependency_name| {
                let planned_agent = execution_plan.planned_agents.get(dependency_name)?;

                Some(WorkflowExecutionGraphPort {
                    name: format!("agent.{dependency_name}"),
                    schema: workflow_type_to_json_schema(&planned_agent.final_output_type),
                })
            })
            .collect()
    }

    fn model_name(&self) -> Option<String> {
        self.declaration.model_usage()?.model_name().map(str::to_string)
    }

    fn execution_graph_tools(&self, execution_plan: &ExecutionPlan, workflow: &Workflow) -> Vec<WorkflowExecutionGraphTool> {
        let Some(Expression::ArrayLiteral(use_expressions)) = self.declaration.expression_property(AgentExpressionPropertyName::Uses)
        else {
            return Vec::new();
        };
        let mut tools = Vec::new();

        for use_expression in use_expressions {
            if let Some(tool) = self.execution_graph_tool(use_expression, execution_plan, workflow) {
                tools.push(tool);
            }
        }

        tools
    }

    fn execution_graph_tool(
        &self,
        use_expression: &Expression,
        execution_plan: &ExecutionPlan,
        workflow: &Workflow,
    ) -> Option<WorkflowExecutionGraphTool> {
        let reference = use_expression.use_reference()?;

        match reference.root_keyword()? {
            ReferenceKeyword::Tool => self.execution_graph_declared_tool(use_expression, reference, execution_plan),
            ReferenceKeyword::Prompt => self.execution_graph_prompt(reference, workflow),
            ReferenceKeyword::Resource => self.execution_graph_resource(reference, workflow),
            ReferenceKeyword::Agent
            | ReferenceKeyword::Dynamic
            | ReferenceKeyword::Input
            | ReferenceKeyword::Model
            | ReferenceKeyword::Secrets => None,
        }
    }

    fn execution_graph_declared_tool(
        &self,
        use_expression: &Expression,
        reference: &Reference,
        execution_plan: &ExecutionPlan,
    ) -> Option<WorkflowExecutionGraphTool> {
        let tool_name = reference.first_access_field()?;
        let typed_tool = execution_plan.tools.get(tool_name)?;
        let (kind, server_name, item_name) = match &typed_tool.declaration.source {
            Some(ToolSource::Mcp(mcp_tool_source)) => (
                WorkflowExecutionGraphToolKind::McpTool,
                mcp_tool_source.server_name.clone(),
                Some(mcp_tool_source.tool_name.clone()),
            ),
            None => (WorkflowExecutionGraphToolKind::LocalTool, None, None),
        };

        Some(WorkflowExecutionGraphTool {
            name: typed_tool.name.clone(),
            kind,
            server_name,
            item_name,
            description: typed_tool.declaration.description.clone(),
            max_calls: use_expression.max_calls_override().or(typed_tool.declaration.max_calls),
            input_schema: workflow_type_to_json_schema(&typed_tool.input_type),
            output_schema: workflow_type_to_json_schema(&typed_tool.output_type),
        })
    }

    fn execution_graph_prompt(&self, reference: &Reference, workflow: &Workflow) -> Option<WorkflowExecutionGraphTool> {
        let prompt_name = reference.first_access_field()?;
        let prompt_import = workflow.find_prompt_import(prompt_name)?;

        Some(WorkflowExecutionGraphTool {
            name: format!("render_{prompt_name}"),
            kind: WorkflowExecutionGraphToolKind::McpPrompt,
            server_name: Some(prompt_import.source.server_name.clone()),
            item_name: Some(prompt_import.source.item_name.clone()),
            description: Some(format!("prompt MCP import `{prompt_name}`")),
            max_calls: None,
            input_schema: WorkflowExecutionGraphTool::open_object_schema(),
            output_schema: json!({ "type": "string" }),
        })
    }

    fn execution_graph_resource(&self, reference: &Reference, workflow: &Workflow) -> Option<WorkflowExecutionGraphTool> {
        let resource_name = reference.first_access_field()?;
        let resource_import = workflow.find_resource_import(resource_name)?;

        Some(WorkflowExecutionGraphTool {
            name: format!("read_{resource_name}"),
            kind: WorkflowExecutionGraphToolKind::McpResource,
            server_name: Some(resource_import.source.server_name.clone()),
            item_name: Some(resource_import.source.item_name.clone()),
            description: Some(format!("resource MCP import `{resource_name}`")),
            max_calls: None,
            input_schema: WorkflowExecutionGraphTool::open_object_schema(),
            output_schema: json!({ "type": "string" }),
        })
    }
}

impl WorkflowExecutionGraphTool {
    fn open_object_schema() -> Value {
        json!({
            "type": "object",
            "additionalProperties": true,
        })
    }
}

impl Expression {
    fn use_reference(&self) -> Option<&Reference> {
        match self {
            Self::Reference(reference) => Some(reference),
            Self::ToolCall(tool_call) => Some(&tool_call.callee),
            Self::StringLiteral(_)
            | Self::StringTemplate(_)
            | Self::NumberLiteral(_)
            | Self::BooleanLiteral(_)
            | Self::NullLiteral
            | Self::FunctionCall(_)
            | Self::McpCall(_)
            | Self::NullFallback(_)
            | Self::VariantProjection(_)
            | Self::Match(_)
            | Self::ArrayLiteral(_)
            | Self::ObjectLiteral(_) => None,
        }
    }

    fn max_calls_override(&self) -> Option<u64> {
        match self {
            Self::ToolCall(tool_call) => tool_call.max_calls,
            Self::StringLiteral(_)
            | Self::StringTemplate(_)
            | Self::NumberLiteral(_)
            | Self::BooleanLiteral(_)
            | Self::NullLiteral
            | Self::Reference(_)
            | Self::FunctionCall(_)
            | Self::McpCall(_)
            | Self::NullFallback(_)
            | Self::VariantProjection(_)
            | Self::Match(_)
            | Self::ArrayLiteral(_)
            | Self::ObjectLiteral(_) => None,
        }
    }
}

impl WorkflowExecutionGraphNodeKind {
    fn node_id(&self) -> String {
        match self {
            Self::Input => ReferenceKeyword::Input.as_str().to_string(),
            Self::Agent => ReferenceKeyword::Agent.as_str().to_string(),
            Self::Output => "output".to_string(),
        }
    }
}

impl WorkflowExecutionGraphEdge {
    fn new(source: &str, target: &str, label: &str, kind: WorkflowExecutionGraphEdgeKind) -> Self {
        Self {
            id: format!("{source}->{target}:{label}"),
            source: source.to_string(),
            target: target.to_string(),
            label: label.to_string(),
            kind,
        }
    }
}
