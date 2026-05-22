use crate::dsl::{
    AgentExpressionPropertyName, AgentForLoop, AgentForLoopPattern, AgentProperty, CallArgument, DeclarationKeyword, Expression,
    MatchBranch, ModelDeclaration, ModelDeclarationPropertyName, ObjectField, ProviderDeclaration, Reference, ReferenceKeyword,
    StringTemplatePart, ToolSource, Workflow,
};
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
    pub instruction: Option<String>,
    pub details: Vec<WorkflowExecutionGraphDetail>,
    pub bindings: Vec<WorkflowExecutionGraphBinding>,
    pub tools: Vec<WorkflowExecutionGraphTool>,
    pub execution_index: Option<usize>,
    pub loop_info: Option<WorkflowExecutionGraphLoopInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkflowExecutionGraphLoopInfo {
    pub pattern: String,
    pub iterable_schema: Value,
    pub iteration_output_schema: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkflowExecutionGraphDetail {
    pub name: String,
    pub value: String,
    pub secret: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkflowExecutionGraphBinding {
    pub name: String,
    pub expression: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowExecutionGraphNodeKind {
    Provider,
    Model,
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
    ProviderClient,
    Model,
    Input,
    AgentDependency,
    WorkflowOutput,
}

impl WorkflowExecutionGraph {
    #[must_use]
    pub fn stable_json(&self) -> String {
        let graph_json = serde_json::to_string_pretty(self).expect("workflow execution graph should serialize");

        format!("{graph_json}\n")
    }
}

impl ExecutionPlan {
    #[must_use]
    pub fn execution_graph(&self, workflow: &Workflow) -> WorkflowExecutionGraph {
        let mut nodes = self.provider_and_model_graph_nodes(workflow);
        nodes.push(self.workflow_input_graph_node());

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

    fn provider_and_model_graph_nodes(&self, workflow: &Workflow) -> Vec<WorkflowExecutionGraphNode> {
        let mut nodes = Vec::new();
        let mut emitted_provider_names = HashSet::new();
        let mut emitted_model_names = HashSet::new();

        for agent_name in &self.agent_execution_order {
            let Some(planned_agent) = self.planned_agents.get(agent_name) else {
                continue;
            };

            if emitted_provider_names.insert(planned_agent.provider_name.clone()) {
                if let Some(provider_declaration) = workflow.find_provider(&planned_agent.provider_name) {
                    nodes.push(provider_declaration.execution_graph_node());
                }
            }

            let Some(model_name) = planned_agent.model_name() else {
                continue;
            };

            if emitted_model_names.insert(model_name.clone()) {
                if let Some(model_declaration) = workflow.find_model(&model_name) {
                    nodes.push(model_declaration.execution_graph_node());
                }
            }
        }

        nodes
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
            instruction: None,
            details: Vec::new(),
            bindings: Vec::new(),
            tools: Vec::new(),
            execution_index: None,
            loop_info: None,
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
                    schema: planned_agent.final_output_schema.clone(),
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
            instruction: None,
            details: Vec::new(),
            bindings: self.workflow_output_graph_bindings(),
            tools: Vec::new(),
            execution_index: None,
            loop_info: None,
        }
    }

    fn workflow_output_graph_bindings(&self) -> Vec<WorkflowExecutionGraphBinding> {
        self.output_declaration
            .fields
            .iter()
            .map(ObjectField::execution_graph_binding)
            .collect()
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

        let mut emitted_provider_model_edges = HashSet::new();
        let mut emitted_model_agent_edges = HashSet::new();

        for agent_name in &self.agent_execution_order {
            let Some(planned_agent) = self.planned_agents.get(agent_name) else {
                continue;
            };

            let provider_node_id = provider_node_id(&planned_agent.provider_name);
            let Some(model_name) = planned_agent.model_name() else {
                continue;
            };
            let model_node_id = model_node_id(&model_name);

            if emitted_provider_model_edges.insert((provider_node_id.clone(), model_node_id.clone())) {
                edges.push(WorkflowExecutionGraphEdge::new(
                    &provider_node_id,
                    &model_node_id,
                    "client",
                    WorkflowExecutionGraphEdgeKind::ProviderClient,
                ));
            }

            if emitted_model_agent_edges.insert((model_node_id.clone(), planned_agent.name.clone())) {
                edges.push(WorkflowExecutionGraphEdge::new(
                    &model_node_id,
                    &planned_agent.name,
                    AgentExpressionPropertyName::Model.as_str(),
                    WorkflowExecutionGraphEdgeKind::Model,
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

impl ProviderDeclaration {
    fn execution_graph_node(&self) -> WorkflowExecutionGraphNode {
        WorkflowExecutionGraphNode {
            id: provider_node_id(&self.name),
            label: self.name.clone(),
            kind: WorkflowExecutionGraphNodeKind::Provider,
            inputs: Vec::new(),
            outputs: vec![WorkflowExecutionGraphPort {
                name: "client".to_string(),
                schema: json!({ "type": "object", "title": format!("{} client", self.driver_name) }),
            }],
            dependencies: Vec::new(),
            provider_name: Some(self.driver_name.clone()),
            model: None,
            instruction: None,
            details: self.execution_graph_details(),
            bindings: Vec::new(),
            tools: Vec::new(),
            execution_index: None,
            loop_info: None,
        }
    }

    fn execution_graph_details(&self) -> Vec<WorkflowExecutionGraphDetail> {
        self.properties.iter().map(ObjectField::execution_graph_detail).collect()
    }
}

impl ModelDeclaration {
    fn execution_graph_node(&self) -> WorkflowExecutionGraphNode {
        WorkflowExecutionGraphNode {
            id: model_node_id(&self.name),
            label: self.id_literal().unwrap_or(&self.name).to_string(),
            kind: WorkflowExecutionGraphNodeKind::Model,
            inputs: vec![WorkflowExecutionGraphPort {
                name: "client".to_string(),
                schema: json!({ "type": "object", "title": "Provider client" }),
            }],
            outputs: vec![WorkflowExecutionGraphPort {
                name: ModelDeclarationPropertyName::Id.as_str().to_string(),
                schema: json!({ "type": "object", "title": "Language model" }),
            }],
            dependencies: vec![provider_node_id(&self.provider_name)],
            provider_name: Some(self.provider_name.clone()),
            model: Some(self.name.clone()),
            instruction: None,
            details: self.execution_graph_details(),
            bindings: Vec::new(),
            tools: Vec::new(),
            execution_index: None,
            loop_info: None,
        }
    }

    fn execution_graph_details(&self) -> Vec<WorkflowExecutionGraphDetail> {
        let mut details = vec![WorkflowExecutionGraphDetail {
            name: "provider".to_string(),
            value: self.provider_name.clone(),
            secret: false,
        }];

        details.extend(self.properties.iter().map(ObjectField::execution_graph_detail));

        details
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
                schema: self.final_output_schema.clone(),
            }],
            dependencies: self.dependencies.clone(),
            provider_name: Some(self.provider_name.clone()),
            model: self.model_name(),
            instruction: self.instruction_label(),
            details: self.execution_graph_details(),
            bindings: self.execution_graph_bindings(),
            tools: self.execution_graph_tools(execution_plan, workflow),
            execution_index: Some(agent_index),
            loop_info: self.execution_graph_loop_info(),
        }
    }

    fn instruction_label(&self) -> Option<String> {
        self.declaration
            .expression_property(AgentExpressionPropertyName::Instruction)
            .map(Expression::graph_label)
    }

    fn execution_graph_details(&self) -> Vec<WorkflowExecutionGraphDetail> {
        let mut details = vec![WorkflowExecutionGraphDetail {
            name: "provider".to_string(),
            value: self.provider_name.clone(),
            secret: false,
        }];

        if let Some(model_name) = self.model_name() {
            details.push(WorkflowExecutionGraphDetail {
                name: AgentExpressionPropertyName::Model.as_str().to_string(),
                value: model_name,
                secret: false,
            });
        }

        details.extend(self.inference_fields.iter().map(ObjectField::execution_graph_detail));

        details
    }

    fn execution_graph_bindings(&self) -> Vec<WorkflowExecutionGraphBinding> {
        let mut bindings = Vec::new();

        if let Some(for_loop) = &self.declaration.for_loop {
            bindings.push(WorkflowExecutionGraphBinding {
                name: "loop".to_string(),
                expression: for_loop.iterable.graph_label(),
            });
        }

        for agent_property in &self.declaration.properties {
            match agent_property {
                AgentProperty::Instruction(expression) => {
                    bindings.push(WorkflowExecutionGraphBinding {
                        name: AgentExpressionPropertyName::Instruction.as_str().to_string(),
                        expression: expression.graph_label(),
                    });
                }
                AgentProperty::Context(expression) => {
                    bindings.push(WorkflowExecutionGraphBinding {
                        name: AgentExpressionPropertyName::Context.as_str().to_string(),
                        expression: expression.graph_label(),
                    });
                }
                AgentProperty::Uses(expression) => {
                    bindings.push(WorkflowExecutionGraphBinding {
                        name: AgentExpressionPropertyName::Uses.as_str().to_string(),
                        expression: expression.graph_label(),
                    });
                }
                AgentProperty::Dynamic(dynamic_block) => {
                    bindings.extend(dynamic_block.fields.iter().map(ObjectField::execution_graph_binding));
                }
                AgentProperty::Model(_)
                | AgentProperty::InvalidModel(_)
                | AgentProperty::Output { fields: _, span: _ }
                | AgentProperty::Unknown { name: _, span: _ } => {}
            }
        }

        bindings
    }

    fn execution_graph_loop_info(&self) -> Option<WorkflowExecutionGraphLoopInfo> {
        let for_loop = self.declaration.for_loop.as_ref()?;

        Some(WorkflowExecutionGraphLoopInfo {
            pattern: for_loop.pattern_label(),
            iterable_schema: json!({ "type": "array" }),
            iteration_output_schema: self.iteration_output_schema.clone(),
        })
    }

    fn execution_graph_inputs(&self, execution_plan: &ExecutionPlan) -> Vec<WorkflowExecutionGraphPort> {
        self.dependencies
            .iter()
            .filter_map(|dependency_name| {
                let planned_agent = execution_plan.planned_agents.get(dependency_name)?;

                Some(WorkflowExecutionGraphPort {
                    name: format!("agent.{dependency_name}"),
                    schema: planned_agent.final_output_schema.clone(),
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
        let reference = use_expression.direct_reference()?;

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
        let tool_name = reference.tool_name()?;
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
            input_schema: typed_tool.input_schema.clone(),
            output_schema: typed_tool.output_schema(),
        })
    }

    fn execution_graph_prompt(&self, reference: &Reference, workflow: &Workflow) -> Option<WorkflowExecutionGraphTool> {
        let prompt_name = reference.import_name(ReferenceKeyword::Prompt)?;
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
        let resource_name = reference.import_name(ReferenceKeyword::Resource)?;
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

impl ObjectField {
    fn execution_graph_detail(&self) -> WorkflowExecutionGraphDetail {
        let normalized_name = self.name.to_ascii_lowercase();
        let secret = normalized_name.contains("key") || normalized_name.contains("secret") || normalized_name.contains("token");

        WorkflowExecutionGraphDetail {
            name: self.name.clone(),
            value: if secret {
                mask_secret_expression(&self.value)
            } else {
                self.value.graph_label()
            },
            secret,
        }
    }

    fn execution_graph_binding(&self) -> WorkflowExecutionGraphBinding {
        WorkflowExecutionGraphBinding {
            name: self.name.clone(),
            expression: self.value.graph_label(),
        }
    }
}

impl AgentForLoop {
    fn pattern_label(&self) -> String {
        self.pattern.label()
    }
}

impl AgentForLoopPattern {
    fn label(&self) -> String {
        match self {
            Self::Identifier(identifier) => identifier.clone(),
            Self::ObjectDestructuring(field_names) => format!("{{ {} }}", field_names.join(", ")),
        }
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

impl WorkflowExecutionGraphNodeKind {
    fn node_id(&self) -> String {
        match self {
            Self::Provider => DeclarationKeyword::Provider.as_str().to_string(),
            Self::Model => ReferenceKeyword::Model.as_str().to_string(),
            Self::Input => ReferenceKeyword::Input.as_str().to_string(),
            Self::Agent => ReferenceKeyword::Agent.as_str().to_string(),
            Self::Output => "output".to_string(),
        }
    }
}

impl Expression {
    fn graph_label(&self) -> String {
        match self {
            Self::StringLiteral(string_value) => string_value.clone(),
            Self::StringTemplate(string_template) => string_template
                .parts
                .iter()
                .map(|template_part| match template_part {
                    StringTemplatePart::Text(template_text) => template_text.clone(),
                    StringTemplatePart::Interpolation(expression) => format!("{{{{ {} }}}}", expression.graph_label()),
                })
                .collect::<String>(),
            Self::NumberLiteral(number_value) => number_value.clone(),
            Self::BooleanLiteral(boolean_value) => boolean_value.to_string(),
            Self::NullLiteral => "null".to_string(),
            Self::Reference(reference) => reference.render_path(),
            Self::FunctionCall(function_call) => {
                let arguments = function_call
                    .arguments
                    .iter()
                    .map(CallArgument::graph_label)
                    .collect::<Vec<_>>()
                    .join(", ");

                format!("{}({arguments})", function_call.callee.render_path())
            }
            Self::ToolCall(tool_call) => format!("{}(...)", tool_call.callee.render_path()),
            Self::McpCall(mcp_call) => format!("{}({})", mcp_call.operation.as_str(), mcp_call.callee.render_path()),
            Self::NullFallback(null_fallback) => {
                format!("{} ?? {}", null_fallback.value.graph_label(), null_fallback.fallback.graph_label())
            }
            Self::VariantProjection(variant_projection) => format!(
                "{}.{}{}",
                variant_projection.value.render_path(),
                variant_projection.case_name,
                render_field_path_suffix(&variant_projection.field_path)
            ),
            Self::Match(match_expression) => {
                let branches = match_expression
                    .branches
                    .iter()
                    .map(MatchBranch::graph_label)
                    .collect::<Vec<_>>()
                    .join(", ");

                format!("match {} {{ {branches} }}", match_expression.value.graph_label())
            }
            Self::ArrayLiteral(array_items) => format!(
                "[{}]",
                array_items.iter().map(Expression::graph_label).collect::<Vec<_>>().join(", ")
            ),
            Self::ObjectLiteral(object_fields) => format!(
                "{{ {} }}",
                object_fields
                    .iter()
                    .map(|object_field| format!("{}: {}", object_field.name, object_field.value.graph_label()))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        }
    }
}

impl CallArgument {
    fn graph_label(&self) -> String {
        match self {
            Self::Positional(expression) => expression.graph_label(),
            Self::Named(named_argument) => format!("{}: {}", named_argument.name, named_argument.value.graph_label()),
        }
    }
}

impl MatchBranch {
    fn graph_label(&self) -> String {
        match self {
            Self::Variant {
                case_name,
                field_path,
                span: _,
            } => format!("{case_name}{}", render_field_path_suffix(field_path)),
            Self::Fallback { value, span: _ } => format!("_ => {}", value.graph_label()),
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

fn provider_node_id(provider_name: &str) -> String {
    format!("{}:{provider_name}", DeclarationKeyword::Provider.as_str())
}

fn model_node_id(model_name: &str) -> String {
    format!("{}:{model_name}", ReferenceKeyword::Model.as_str())
}

fn render_field_path_suffix(field_path: &[String]) -> String {
    let mut suffix = String::new();

    for field_name in field_path {
        suffix.push('.');
        suffix.push_str(field_name);
    }

    suffix
}

fn mask_secret_expression(expression: &Expression) -> String {
    let value = expression.graph_label();
    let value_characters = value.chars().collect::<Vec<_>>();

    if value_characters.len() <= 4 {
        return "****".to_string();
    }

    let prefix = value_characters.iter().take(2).collect::<String>();
    let suffix = value_characters.iter().skip(value_characters.len() - 2).collect::<String>();

    format!("{prefix}****{suffix}")
}
