use crate::semantic::ir::TypedToolIr;
use crate::semantic::plan::{ExecutionPlan, PlannedAgent};
use crate::semantic::support::type_inference::{ExpressionTypeInferenceExt, TypeInferenceContext};
use crate::semantic::support::types::WorkflowSchemaCache;
use crate::semantic::support::types::WorkflowType;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use superwire_types::ast::{
    AgentContext, AgentContextPropertyName, AgentExpressionPropertyName, AgentForLoop, AgentForLoopPattern, AgentProperty, CallArgument,
    CompactAgentContext, DeclarationKeyword, Expression, MatchBranch, McpPromptImportDeclaration, McpResourceImportDeclaration,
    ModelDeclaration, ModelDeclarationPropertyName, ObjectField, OutputDeclaration, ProviderDeclaration, Reference, ReferenceKeyword,
    StringTemplatePart, ToolCall, ToolSource, Workflow,
};

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
    Dynamic,
    Compact,
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
    Dynamic,
    AgentDependency,
    WorkflowOutput,
}

struct WorkflowExecutionGraphLookups<'workflow> {
    provider_declarations: HashMap<&'workflow str, &'workflow ProviderDeclaration>,
    model_declarations: HashMap<&'workflow str, &'workflow ModelDeclaration>,
    prompt_imports: HashMap<&'workflow str, &'workflow McpPromptImportDeclaration>,
    resource_imports: HashMap<&'workflow str, &'workflow McpResourceImportDeclaration>,
}

impl<'workflow> WorkflowExecutionGraphLookups<'workflow> {
    fn from_workflow(workflow: &'workflow Workflow) -> Self {
        Self {
            provider_declarations: workflow.provider_declarations_by_name(),
            model_declarations: workflow.model_declarations_by_name(),
            prompt_imports: workflow.prompt_imports_by_name(),
            resource_imports: workflow.resource_imports_by_name(),
        }
    }
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
        let graph_lookups = WorkflowExecutionGraphLookups::from_workflow(workflow);
        let mut schema_cache = WorkflowSchemaCache::new();
        let mut nodes = self.provider_and_model_graph_nodes(&graph_lookups);
        nodes.push(self.workflow_input_graph_node(&mut schema_cache));

        if let Some(dynamic_node) = self.workflow_dynamic_graph_node(workflow, &mut schema_cache) {
            nodes.push(dynamic_node);
        }

        for (agent_index, agent_name) in self.agent_execution_order.iter().enumerate() {
            if let Some(planned_agent) = self.planned_agents.get(agent_name) {
                if let Some(compact_node) = planned_agent.compact_context_execution_graph_node(&graph_lookups, agent_index) {
                    nodes.push(compact_node);
                }

                nodes.push(planned_agent.execution_graph_node(self, workflow, &graph_lookups, agent_index, &mut schema_cache));
            }
        }

        nodes.push(self.workflow_output_graph_node(&mut schema_cache));

        WorkflowExecutionGraph {
            nodes,
            edges: self.execution_graph_edges(workflow),
            agent_execution_order: self.agent_execution_order.clone(),
        }
    }

    fn provider_and_model_graph_nodes(&self, graph_lookups: &WorkflowExecutionGraphLookups<'_>) -> Vec<WorkflowExecutionGraphNode> {
        let mut nodes = Vec::new();
        let mut emitted_provider_names = HashSet::new();
        let mut emitted_model_names = HashSet::new();

        for agent_name in &self.agent_execution_order {
            let Some(planned_agent) = self.planned_agents.get(agent_name) else {
                continue;
            };

            if emitted_provider_names.insert(planned_agent.provider_name.clone()) {
                if let Some(provider_declaration) = graph_lookups.provider_declarations.get(planned_agent.provider_name.as_str()) {
                    nodes.push(provider_declaration.execution_graph_node());
                }
            }

            let Some(model_name) = planned_agent.model_name() else {
                continue;
            };

            if emitted_model_names.insert(model_name.clone()) {
                if let Some(model_declaration) = graph_lookups.model_declarations.get(model_name.as_str()) {
                    nodes.push(model_declaration.execution_graph_node());
                }
            }

            let Some(compact_model_name) = planned_agent.compact_context_model_name() else {
                continue;
            };
            let Some(model_declaration) = graph_lookups.model_declarations.get(compact_model_name.as_str()) else {
                continue;
            };

            if emitted_provider_names.insert(model_declaration.provider_name.clone()) {
                if let Some(provider_declaration) = graph_lookups.provider_declarations.get(model_declaration.provider_name.as_str()) {
                    nodes.push(provider_declaration.execution_graph_node());
                }
            }

            if emitted_model_names.insert(compact_model_name) {
                nodes.push(model_declaration.execution_graph_node());
            }
        }

        nodes
    }

    fn workflow_input_graph_node(&self, schema_cache: &mut WorkflowSchemaCache) -> WorkflowExecutionGraphNode {
        let mut outputs = Vec::new();

        if let Some(input_type) = &self.input_type {
            outputs.push(WorkflowExecutionGraphPort {
                name: ReferenceKeyword::Input.as_str().to_string(),
                schema: input_type.json_schema_value_with_cache(schema_cache),
            });
        }

        if let Some(secrets_type) = &self.secrets_type {
            outputs.push(WorkflowExecutionGraphPort {
                name: ReferenceKeyword::Secrets.as_str().to_string(),
                schema: secrets_type.json_schema_value_with_cache(schema_cache),
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

    fn workflow_dynamic_graph_node(
        &self,
        workflow: &Workflow,
        schema_cache: &mut WorkflowSchemaCache,
    ) -> Option<WorkflowExecutionGraphNode> {
        let dynamic_fields = workflow
            .dynamic_blocks()
            .flat_map(|dynamic_block| dynamic_block.fields.iter())
            .collect::<Vec<_>>();

        if dynamic_fields.is_empty() {
            return None;
        }

        let dynamic_field_types = self.workflow_dynamic_field_types(dynamic_fields.as_slice());
        let outputs = dynamic_fields
            .iter()
            .map(|dynamic_field| {
                let schema = dynamic_field_types
                    .get(&dynamic_field.name)
                    .map_or_else(WorkflowExecutionGraphTool::open_object_schema, |field_type| {
                        field_type.json_schema_value_with_cache(schema_cache)
                    });

                WorkflowExecutionGraphPort {
                    name: dynamic_field.name.clone(),
                    schema,
                }
            })
            .collect::<Vec<_>>();

        Some(WorkflowExecutionGraphNode {
            id: WorkflowExecutionGraphNodeKind::Dynamic.node_id(),
            label: "Dynamic values".to_string(),
            kind: WorkflowExecutionGraphNodeKind::Dynamic,
            inputs: self.workflow_dynamic_graph_inputs(dynamic_fields.as_slice()),
            outputs,
            dependencies: Vec::new(),
            provider_name: None,
            model: None,
            instruction: None,
            details: Vec::new(),
            bindings: dynamic_fields
                .iter()
                .map(|dynamic_field| dynamic_field.execution_graph_binding())
                .collect(),
            tools: self.workflow_dynamic_graph_tools(dynamic_fields.as_slice(), schema_cache),
            execution_index: None,
            loop_info: None,
        })
    }

    fn workflow_dynamic_graph_inputs(&self, dynamic_fields: &[&ObjectField]) -> Vec<WorkflowExecutionGraphPort> {
        if dynamic_fields.iter().any(|dynamic_field| dynamic_field.value.references_runtime()) {
            return vec![WorkflowExecutionGraphPort {
                name: "runtime".to_string(),
                schema: json!({ "type": "object", "additionalProperties": true }),
            }];
        }

        Vec::new()
    }

    fn workflow_dynamic_field_types(&self, dynamic_fields: &[&ObjectField]) -> HashMap<String, WorkflowType> {
        let mut inference_context = self.type_inference_context();
        let mut pending_dynamic_fields = dynamic_fields.to_vec();

        while !pending_dynamic_fields.is_empty() {
            let pending_count_before_pass = pending_dynamic_fields.len();

            pending_dynamic_fields.retain(|dynamic_field| {
                match dynamic_field.value.infer_type(
                    &inference_context,
                    &format!("dynamic field `{}` graph type inference", dynamic_field.name),
                ) {
                    Ok(field_type) => {
                        inference_context.local_binding_types.insert(dynamic_field.name.clone(), field_type);

                        false
                    }
                    Err(_) => true,
                }
            });

            if pending_dynamic_fields.len() == pending_count_before_pass {
                break;
            }
        }

        inference_context.local_binding_types
    }

    fn workflow_dynamic_field_types_for_workflow(&self, workflow: &Workflow) -> HashMap<String, WorkflowType> {
        let dynamic_fields = workflow
            .dynamic_blocks()
            .flat_map(|dynamic_block| dynamic_block.fields.iter())
            .collect::<Vec<_>>();

        self.workflow_dynamic_field_types(dynamic_fields.as_slice())
    }

    fn type_inference_context(&self) -> TypeInferenceContext {
        TypeInferenceContext {
            input_type: self.input_type.clone(),
            secrets_type: self.secrets_type.clone(),
            agent_output_types: self
                .planned_agents
                .iter()
                .map(|(agent_name, planned_agent)| (agent_name.clone(), planned_agent.final_output_type.clone()))
                .collect(),
            tool_input_types: self
                .tools
                .iter()
                .map(|(tool_name, typed_tool)| (tool_name.clone(), typed_tool.input_type.clone()))
                .collect(),
            tool_binding_types: self
                .tools
                .iter()
                .map(|(tool_name, typed_tool)| (tool_name.clone(), typed_tool.binding_type.clone()))
                .collect(),
            tool_output_types: self
                .tools
                .iter()
                .map(|(tool_name, typed_tool)| (tool_name.clone(), typed_tool.output_type.clone()))
                .collect(),
            local_binding_types: HashMap::new(),
        }
    }

    fn workflow_dynamic_graph_tools(
        &self,
        dynamic_fields: &[&ObjectField],
        schema_cache: &mut WorkflowSchemaCache,
    ) -> Vec<WorkflowExecutionGraphTool> {
        let mut tools = Vec::new();
        let mut emitted_tool_names = HashSet::new();

        for dynamic_field in dynamic_fields {
            for tool_call in dynamic_field.value.tool_calls() {
                let Some(tool_name) = tool_call.callee.tool_name() else {
                    continue;
                };

                if !emitted_tool_names.insert(tool_name.to_string()) {
                    continue;
                }

                let Some(typed_tool) = self.tools.get(tool_name) else {
                    continue;
                };

                tools.push(typed_tool.execution_graph_tool(tool_call, schema_cache));
            }
        }

        tools
    }

    fn workflow_output_graph_node(&self, schema_cache: &mut WorkflowSchemaCache) -> WorkflowExecutionGraphNode {
        let mut inputs = self
            .workflow_output_agent_dependencies()
            .into_iter()
            .filter_map(|agent_name| {
                let planned_agent = self.planned_agents.get(&agent_name)?;

                Some(WorkflowExecutionGraphPort {
                    name: format!("agent.{agent_name}"),
                    schema: planned_agent.final_output_schema_with_cache(schema_cache),
                })
            })
            .collect::<Vec<_>>();

        if !self.workflow_output_dynamic_dependencies().is_empty() {
            inputs.push(WorkflowExecutionGraphPort {
                name: ReferenceKeyword::Dynamic.as_str().to_string(),
                schema: json!({ "type": "object", "additionalProperties": true }),
            });
        }

        if self.output_declaration.references_runtime() {
            inputs.push(WorkflowExecutionGraphPort {
                name: "runtime".to_string(),
                schema: json!({ "type": "object", "additionalProperties": true }),
            });
        }

        WorkflowExecutionGraphNode {
            id: WorkflowExecutionGraphNodeKind::Output.node_id(),
            label: "Workflow output".to_string(),
            kind: WorkflowExecutionGraphNodeKind::Output,
            inputs,
            outputs: vec![WorkflowExecutionGraphPort {
                name: "output".to_string(),
                schema: self.workflow_output_type.json_schema_value_with_cache(schema_cache),
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

    fn workflow_output_dynamic_dependencies(&self) -> Vec<String> {
        self.output_declaration.dynamic_dependencies()
    }

    #[allow(clippy::too_many_lines)]
    fn execution_graph_edges(&self, workflow: &Workflow) -> Vec<WorkflowExecutionGraphEdge> {
        let mut edges = Vec::new();
        let input_node_id = WorkflowExecutionGraphNodeKind::Input.node_id();
        let dynamic_node_id = WorkflowExecutionGraphNodeKind::Dynamic.node_id();
        let output_node_id = WorkflowExecutionGraphNodeKind::Output.node_id();

        if workflow
            .dynamic_blocks()
            .flat_map(|dynamic_block| dynamic_block.fields.iter())
            .any(|dynamic_field| dynamic_field.value.references_runtime())
        {
            edges.push(WorkflowExecutionGraphEdge::new(
                &input_node_id,
                &dynamic_node_id,
                "runtime",
                WorkflowExecutionGraphEdgeKind::Input,
            ));
        }

        for agent_name in &self.agent_execution_order {
            let Some(planned_agent) = self.planned_agents.get(agent_name) else {
                continue;
            };

            if planned_agent.references_runtime() {
                edges.push(WorkflowExecutionGraphEdge::new(
                    &input_node_id,
                    agent_name,
                    "runtime",
                    WorkflowExecutionGraphEdgeKind::Input,
                ));
            }

            if !planned_agent.workflow_dynamic_dependencies().is_empty() {
                edges.push(WorkflowExecutionGraphEdge::new(
                    &dynamic_node_id,
                    agent_name,
                    "dynamic values",
                    WorkflowExecutionGraphEdgeKind::Dynamic,
                ));
            }

            if let Some(compact_agent_context) = planned_agent.compact_agent_context() {
                let compact_node_id = compact_context_node_id(&planned_agent.name);

                if compact_agent_context.references_runtime() {
                    edges.push(WorkflowExecutionGraphEdge::new(
                        &input_node_id,
                        &compact_node_id,
                        "runtime",
                        WorkflowExecutionGraphEdgeKind::Input,
                    ));
                }

                if let Some(source_agent_name) = compact_agent_context.agent_name() {
                    edges.push(WorkflowExecutionGraphEdge::new(
                        source_agent_name,
                        &compact_node_id,
                        "source context",
                        WorkflowExecutionGraphEdgeKind::AgentDependency,
                    ));
                    edges.push(WorkflowExecutionGraphEdge::new(
                        &compact_node_id,
                        agent_name,
                        "compacted context",
                        WorkflowExecutionGraphEdgeKind::AgentDependency,
                    ));
                }
            }

            for dependency_name in &planned_agent.dependencies {
                if planned_agent
                    .compact_agent_context()
                    .and_then(CompactAgentContext::agent_name)
                    .is_some_and(|source_agent_name| source_agent_name == dependency_name)
                {
                    continue;
                }

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
            let model_node_identifier = model_node_id(&model_name);

            if emitted_provider_model_edges.insert((provider_node_id.clone(), model_node_identifier.clone())) {
                edges.push(WorkflowExecutionGraphEdge::new(
                    &provider_node_id,
                    &model_node_identifier,
                    "client",
                    WorkflowExecutionGraphEdgeKind::ProviderClient,
                ));
            }

            if emitted_model_agent_edges.insert((model_node_identifier.clone(), planned_agent.name.clone())) {
                edges.push(WorkflowExecutionGraphEdge::new(
                    &model_node_identifier,
                    &planned_agent.name,
                    AgentExpressionPropertyName::Model.as_str(),
                    WorkflowExecutionGraphEdgeKind::Model,
                ));
            }

            if planned_agent.compact_agent_context().is_some() {
                let compact_model_name = planned_agent.compact_context_model_name().unwrap_or(model_name);

                edges.push(WorkflowExecutionGraphEdge::new(
                    &model_node_id(&compact_model_name),
                    &compact_context_node_id(&planned_agent.name),
                    AgentContextPropertyName::Model.as_str(),
                    WorkflowExecutionGraphEdgeKind::Model,
                ));
            }
        }

        let output_dependencies = self.workflow_output_agent_dependencies();

        if self.output_declaration.references_runtime() {
            edges.push(WorkflowExecutionGraphEdge::new(
                &input_node_id,
                &output_node_id,
                "runtime",
                WorkflowExecutionGraphEdgeKind::WorkflowOutput,
            ));
        }

        if !self.workflow_output_dynamic_dependencies().is_empty() {
            edges.push(WorkflowExecutionGraphEdge::new(
                &dynamic_node_id,
                &output_node_id,
                "dynamic values",
                WorkflowExecutionGraphEdgeKind::WorkflowOutput,
            ));
        }

        for dependency_name in output_dependencies {
            edges.push(WorkflowExecutionGraphEdge::new(
                &dependency_name,
                &output_node_id,
                "workflow output",
                WorkflowExecutionGraphEdgeKind::WorkflowOutput,
            ));
        }

        edges
    }
}

trait ProviderDeclarationExecutionGraphExt {
    fn execution_graph_node(&self) -> WorkflowExecutionGraphNode;
    fn execution_graph_details(&self) -> Vec<WorkflowExecutionGraphDetail>;
}

impl ProviderDeclarationExecutionGraphExt for ProviderDeclaration {
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

trait ModelDeclarationExecutionGraphExt {
    fn execution_graph_node(&self) -> WorkflowExecutionGraphNode;
    fn execution_graph_details(&self) -> Vec<WorkflowExecutionGraphDetail>;
}

impl ModelDeclarationExecutionGraphExt for ModelDeclaration {
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
    fn compact_context_execution_graph_node(
        &self,
        graph_lookups: &WorkflowExecutionGraphLookups<'_>,
        agent_index: usize,
    ) -> Option<WorkflowExecutionGraphNode> {
        let compact_agent_context = self.compact_agent_context()?;
        let source_agent_name = compact_agent_context.agent_name()?.to_string();
        let model_name = self.compact_context_model_name().or_else(|| self.model_name());
        let provider_name = model_name
            .as_ref()
            .and_then(|model_name| graph_lookups.model_declarations.get(model_name.as_str()))
            .map_or_else(
                || self.provider_name.clone(),
                |model_declaration| model_declaration.provider_name.clone(),
            );

        let mut details = vec![
            WorkflowExecutionGraphDetail {
                name: "source".to_string(),
                value: format!("agent.{source_agent_name}"),
                secret: false,
            },
            WorkflowExecutionGraphDetail {
                name: "target".to_string(),
                value: format!("agent.{}", self.name),
                secret: false,
            },
        ];

        if let Some(model_name) = &model_name {
            details.push(WorkflowExecutionGraphDetail {
                name: AgentContextPropertyName::Model.as_str().to_string(),
                value: model_name.clone(),
                secret: false,
            });
        }

        Some(WorkflowExecutionGraphNode {
            id: compact_context_node_id(&self.name),
            label: format!("Compact {source_agent_name}"),
            kind: WorkflowExecutionGraphNodeKind::Compact,
            inputs: vec![WorkflowExecutionGraphPort {
                name: format!("agent.{source_agent_name}"),
                schema: WorkflowExecutionGraphTool::open_object_schema(),
            }],
            outputs: vec![WorkflowExecutionGraphPort {
                name: "compacted context".to_string(),
                schema: WorkflowExecutionGraphTool::open_object_schema(),
            }],
            dependencies: vec![source_agent_name],
            provider_name: Some(provider_name),
            model: model_name,
            instruction: compact_agent_context.instruction().map(Expression::graph_label),
            details,
            bindings: compact_agent_context
                .properties
                .iter()
                .map(ObjectField::execution_graph_binding)
                .collect(),
            tools: Vec::new(),
            execution_index: Some(agent_index),
            loop_info: None,
        })
    }

    fn execution_graph_node(
        &self,
        execution_plan: &ExecutionPlan,
        workflow: &Workflow,
        graph_lookups: &WorkflowExecutionGraphLookups<'_>,
        agent_index: usize,
        schema_cache: &mut WorkflowSchemaCache,
    ) -> WorkflowExecutionGraphNode {
        WorkflowExecutionGraphNode {
            id: self.name.clone(),
            label: self.name.clone(),
            kind: WorkflowExecutionGraphNodeKind::Agent,
            inputs: self.execution_graph_inputs(execution_plan, workflow, schema_cache),
            outputs: vec![WorkflowExecutionGraphPort {
                name: format!("agent.{}", self.name),
                schema: self.final_output_schema_with_cache(schema_cache),
            }],
            dependencies: self.dependencies.clone(),
            provider_name: Some(self.provider_name.clone()),
            model: self.model_name(),
            instruction: self.instruction_label(),
            details: self.execution_graph_details(),
            bindings: self.execution_graph_bindings(),
            tools: self.execution_graph_tools(execution_plan, graph_lookups, schema_cache),
            execution_index: Some(agent_index),
            loop_info: self.execution_graph_loop_info(schema_cache),
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
                AgentProperty::Context(agent_context) => {
                    bindings.push(WorkflowExecutionGraphBinding {
                        name: AgentExpressionPropertyName::Context.as_str().to_string(),
                        expression: agent_context.graph_label(),
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

    fn execution_graph_loop_info(&self, schema_cache: &mut WorkflowSchemaCache) -> Option<WorkflowExecutionGraphLoopInfo> {
        let for_loop = self.declaration.for_loop.as_ref()?;

        Some(WorkflowExecutionGraphLoopInfo {
            pattern: for_loop.pattern_label(),
            iterable_schema: json!({ "type": "array" }),
            iteration_output_schema: self.iteration_output_schema_with_cache(schema_cache),
        })
    }

    fn execution_graph_inputs(
        &self,
        execution_plan: &ExecutionPlan,
        workflow: &Workflow,
        schema_cache: &mut WorkflowSchemaCache,
    ) -> Vec<WorkflowExecutionGraphPort> {
        let mut inputs = self
            .dependencies
            .iter()
            .filter_map(|dependency_name| {
                let planned_agent = execution_plan.planned_agents.get(dependency_name)?;

                Some(WorkflowExecutionGraphPort {
                    name: format!("agent.{dependency_name}"),
                    schema: planned_agent.final_output_schema_with_cache(schema_cache),
                })
            })
            .collect::<Vec<_>>();

        if let Some(loop_input) = self.execution_graph_loop_input(execution_plan, workflow, schema_cache) {
            inputs.push(loop_input);
        }

        inputs
    }

    fn execution_graph_loop_input(
        &self,
        execution_plan: &ExecutionPlan,
        workflow: &Workflow,
        schema_cache: &mut WorkflowSchemaCache,
    ) -> Option<WorkflowExecutionGraphPort> {
        let for_loop = self.declaration.for_loop.as_ref()?;
        let mut inference_context = execution_plan.type_inference_context();
        inference_context.local_binding_types = execution_plan.workflow_dynamic_field_types_for_workflow(workflow);
        let iterable_type = for_loop
            .iterable
            .infer_type(&inference_context, "agent loop graph input inference")
            .ok()?;
        let item_type = match iterable_type {
            WorkflowType::Array {
                item_type,
                fixed_length: _,
            } => *item_type,
            WorkflowType::Tuple(item_types) => WorkflowType::Union(item_types).normalize(),
            _ => return None,
        };

        Some(WorkflowExecutionGraphPort {
            name: for_loop.pattern_label(),
            schema: item_type.json_schema_value_with_cache(schema_cache),
        })
    }

    fn model_name(&self) -> Option<String> {
        self.declaration.model_usage()?.model_name().map(str::to_string)
    }

    fn compact_agent_context(&self) -> Option<&CompactAgentContext> {
        let agent_context = self.declaration.context_property()?;

        let AgentContext::Compact(compact_agent_context) = agent_context else {
            return None;
        };

        Some(compact_agent_context)
    }

    fn compact_context_model_name(&self) -> Option<String> {
        self.compact_agent_context()?.model_name().map(str::to_string)
    }

    fn references_runtime(&self) -> bool {
        if self
            .declaration
            .for_loop
            .as_ref()
            .is_some_and(|for_loop| for_loop.iterable.references_runtime())
        {
            return true;
        }

        for agent_property in &self.declaration.properties {
            match agent_property {
                AgentProperty::InvalidModel(expression) | AgentProperty::Instruction(expression) | AgentProperty::Uses(expression) => {
                    if expression.references_runtime() {
                        return true;
                    }
                }
                AgentProperty::Context(agent_context) => {
                    if agent_context.references_runtime() {
                        return true;
                    }
                }
                AgentProperty::Model(model_usage) => {
                    if model_usage.properties.iter().any(ObjectField::references_runtime) {
                        return true;
                    }
                }
                AgentProperty::Dynamic(dynamic_block) => {
                    if dynamic_block.fields.iter().any(ObjectField::references_runtime) {
                        return true;
                    }
                }
                AgentProperty::Output { fields: _, span: _ } | AgentProperty::Unknown { name: _, span: _ } => {}
            }
        }

        false
    }

    fn workflow_dynamic_dependencies(&self) -> Vec<String> {
        let mut dependencies = HashSet::new();
        let mut local_dynamic_fields = HashSet::new();

        if let Some(for_loop) = &self.declaration.for_loop {
            for_loop.iterable.collect_dynamic_dependencies(&mut dependencies);
        }

        for agent_property in &self.declaration.properties {
            match agent_property {
                AgentProperty::InvalidModel(expression) | AgentProperty::Instruction(expression) | AgentProperty::Uses(expression) => {
                    expression.collect_dynamic_dependencies(&mut dependencies);
                }
                AgentProperty::Context(agent_context) => {
                    agent_context.collect_dynamic_dependencies(&mut dependencies);
                }
                AgentProperty::Model(model_usage) => {
                    for model_property in &model_usage.properties {
                        model_property.value.collect_dynamic_dependencies(&mut dependencies);
                    }
                }
                AgentProperty::Dynamic(dynamic_block) => {
                    for dynamic_field in &dynamic_block.fields {
                        local_dynamic_fields.insert(dynamic_field.name.clone());
                        dynamic_field.value.collect_dynamic_dependencies(&mut dependencies);
                    }
                }
                AgentProperty::Output { fields: _, span: _ } | AgentProperty::Unknown { name: _, span: _ } => {}
            }
        }

        for local_dynamic_field in local_dynamic_fields {
            dependencies.remove(&local_dynamic_field);
        }

        let mut sorted_dependencies = dependencies.into_iter().collect::<Vec<_>>();
        sorted_dependencies.sort();

        sorted_dependencies
    }

    fn execution_graph_tools(
        &self,
        execution_plan: &ExecutionPlan,
        graph_lookups: &WorkflowExecutionGraphLookups<'_>,
        schema_cache: &mut WorkflowSchemaCache,
    ) -> Vec<WorkflowExecutionGraphTool> {
        let Some(Expression::ArrayLiteral(use_expressions)) = self.declaration.expression_property(AgentExpressionPropertyName::Uses)
        else {
            return Vec::new();
        };
        let mut tools = Vec::new();

        for use_expression in use_expressions {
            if let Some(tool) = self.execution_graph_tool(use_expression, execution_plan, graph_lookups, schema_cache) {
                tools.push(tool);
            }
        }

        tools
    }

    fn execution_graph_tool(
        &self,
        use_expression: &Expression,
        execution_plan: &ExecutionPlan,
        graph_lookups: &WorkflowExecutionGraphLookups<'_>,
        schema_cache: &mut WorkflowSchemaCache,
    ) -> Option<WorkflowExecutionGraphTool> {
        let reference = use_expression.direct_reference()?;

        match reference.root_keyword()? {
            ReferenceKeyword::Tool => self.execution_graph_declared_tool(use_expression, reference, execution_plan, schema_cache),
            ReferenceKeyword::Prompt => self.execution_graph_prompt(reference, graph_lookups),
            ReferenceKeyword::Resource => self.execution_graph_resource(reference, graph_lookups),
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
        schema_cache: &mut WorkflowSchemaCache,
    ) -> Option<WorkflowExecutionGraphTool> {
        let tool_name = reference.tool_name()?;
        let typed_tool = execution_plan.tools.get(tool_name)?;

        Some(typed_tool.execution_graph_tool_from_expression(use_expression, schema_cache))
    }

    fn execution_graph_prompt(
        &self,
        reference: &Reference,
        graph_lookups: &WorkflowExecutionGraphLookups<'_>,
    ) -> Option<WorkflowExecutionGraphTool> {
        let prompt_name = reference.import_name(ReferenceKeyword::Prompt)?;
        let prompt_import = graph_lookups.prompt_imports.get(prompt_name)?;

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

    fn execution_graph_resource(
        &self,
        reference: &Reference,
        graph_lookups: &WorkflowExecutionGraphLookups<'_>,
    ) -> Option<WorkflowExecutionGraphTool> {
        let resource_name = reference.import_name(ReferenceKeyword::Resource)?;
        let resource_import = graph_lookups.resource_imports.get(resource_name)?;

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

impl TypedToolIr {
    fn execution_graph_tool_from_expression(
        &self,
        use_expression: &Expression,
        schema_cache: &mut WorkflowSchemaCache,
    ) -> WorkflowExecutionGraphTool {
        self.execution_graph_tool_with_max_calls(use_expression.max_calls_override(), schema_cache)
    }

    fn execution_graph_tool(&self, tool_call: &ToolCall, schema_cache: &mut WorkflowSchemaCache) -> WorkflowExecutionGraphTool {
        self.execution_graph_tool_with_max_calls(tool_call.max_calls, schema_cache)
    }

    fn execution_graph_tool_with_max_calls(
        &self,
        max_calls_override: Option<u64>,
        schema_cache: &mut WorkflowSchemaCache,
    ) -> WorkflowExecutionGraphTool {
        let (kind, server_name, item_name) = match &self.declaration.source {
            Some(ToolSource::Mcp(mcp_tool_source)) => (
                WorkflowExecutionGraphToolKind::McpTool,
                mcp_tool_source.server_name.clone(),
                Some(mcp_tool_source.tool_name.clone()),
            ),
            None => (WorkflowExecutionGraphToolKind::LocalTool, None, None),
        };

        WorkflowExecutionGraphTool {
            name: self.name.clone(),
            kind,
            server_name,
            item_name,
            description: self.declaration.description.clone(),
            max_calls: max_calls_override.or(self.declaration.max_calls),
            input_schema: self.input_schema_with_cache(schema_cache),
            output_schema: self.output_schema_with_cache(schema_cache),
        }
    }
}

trait OutputDeclarationExecutionGraphExt {
    fn references_runtime(&self) -> bool;
    fn dynamic_dependencies(&self) -> Vec<String>;
}

impl OutputDeclarationExecutionGraphExt for OutputDeclaration {
    fn references_runtime(&self) -> bool {
        self.fields.iter().any(ObjectField::references_runtime)
    }

    fn dynamic_dependencies(&self) -> Vec<String> {
        let mut dependencies = HashSet::new();

        for output_field in &self.fields {
            output_field.value.collect_dynamic_dependencies(&mut dependencies);
        }

        let mut sorted_dependencies = dependencies.into_iter().collect::<Vec<_>>();
        sorted_dependencies.sort();

        sorted_dependencies
    }
}

trait ObjectFieldExecutionGraphExt {
    fn execution_graph_detail(&self) -> WorkflowExecutionGraphDetail;
    fn execution_graph_binding(&self) -> WorkflowExecutionGraphBinding;
    fn references_runtime(&self) -> bool;
}

impl ObjectFieldExecutionGraphExt for ObjectField {
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

    fn references_runtime(&self) -> bool {
        self.value.references_runtime()
    }
}

trait AgentForLoopExecutionGraphExt {
    fn pattern_label(&self) -> String;
}

impl AgentForLoopExecutionGraphExt for AgentForLoop {
    fn pattern_label(&self) -> String {
        self.pattern.label()
    }
}

trait AgentForLoopPatternExecutionGraphExt {
    fn label(&self) -> String;
}

impl AgentForLoopPatternExecutionGraphExt for AgentForLoopPattern {
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
            Self::Dynamic => ReferenceKeyword::Dynamic.as_str().to_string(),
            Self::Compact => "compact".to_string(),
            Self::Agent => ReferenceKeyword::Agent.as_str().to_string(),
            Self::Output => "output".to_string(),
        }
    }
}

trait ExpressionExecutionGraphExt {
    fn graph_label(&self) -> String;
}

trait AgentContextExecutionGraphExt {
    fn graph_label(&self) -> String;
}

impl AgentContextExecutionGraphExt for AgentContext {
    fn graph_label(&self) -> String {
        match self {
            Self::Direct(agent_context_reference) => {
                if agent_context_reference.explicit {
                    return format!("context {}", agent_context_reference.reference.render_path());
                }

                agent_context_reference.reference.render_path()
            }
            Self::Compact(compact_agent_context) => format!("compact {}", compact_agent_context.reference.render_path()),
        }
    }
}

impl ExpressionExecutionGraphExt for Expression {
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
            Self::AgentContext(agent_context) => agent_context.graph_label(),
            Self::Asset(asset) => format!("asset {}", asset.source.graph_label()),
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

trait CallArgumentExecutionGraphExt {
    fn graph_label(&self) -> String;
}

impl CallArgumentExecutionGraphExt for CallArgument {
    fn graph_label(&self) -> String {
        match self {
            Self::Positional(expression) => expression.graph_label(),
            Self::Named(named_argument) => format!("{}: {}", named_argument.name, named_argument.value.graph_label()),
        }
    }
}

trait MatchBranchExecutionGraphExt {
    fn graph_label(&self) -> String;
}

impl MatchBranchExecutionGraphExt for MatchBranch {
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

fn compact_context_node_id(agent_name: &str) -> String {
    format!("compact:{agent_name}")
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

#[cfg(test)]
mod tests {
    use super::WorkflowExecutionGraphNodeKind;
    use crate::semantic::{build_dynamic_typed_workflow_ir, build_execution_plan};
    use superwire_macros::parse_inline_workflow;

    #[test]
    fn graph_connects_workflow_dynamic_values_to_loop_agents_without_runtime_fallback() {
        let workflow = parse_inline_workflow! {
            provider openai from openai {
                endpoint: "https://api.openai.com/v1"
                api_key: "test-api-key"
            }

            model plus from openai {
                id: "model-a"
            }

            tool fetch_answers {
                output {
                    answers: [string]
                }
            }

            dynamic {
                all_answers: call tool.fetch_answers
            }

            agent stage_1 for transcript in dynamic.all_answers.answers {
                model: model.plus
                instruction: transcript
                output {
                    summary: string
                }
            }

            output {
                example: 123
            }
        };

        let typed_workflow_ir = build_dynamic_typed_workflow_ir(&workflow).expect("workflow should typecheck");
        let execution_plan = build_execution_plan(&workflow, &typed_workflow_ir).expect("workflow should plan");
        let graph = execution_plan.execution_graph(&workflow);

        assert!(graph
            .nodes
            .iter()
            .any(|node| node.id == "dynamic" && node.outputs.iter().any(|output| output.name == "all_answers")));
        assert!(graph
            .nodes
            .iter()
            .any(|node| node.id == "stage_1" && node.inputs.iter().any(|input| input.name == "transcript")));
        assert!(graph.edges.iter().any(|edge| edge.source == "dynamic" && edge.target == "stage_1"));
        assert!(!graph.edges.iter().any(|edge| edge.source == "input" && edge.target == "stage_1"));
        assert!(graph
            .nodes
            .iter()
            .any(|node| node.id == "dynamic" && node.tools.iter().any(|tool| tool.name == "fetch_answers")));
    }

    #[test]
    fn graph_represents_compact_context_as_dedicated_node() {
        let workflow = parse_inline_workflow! {
            provider openai from openai {
                endpoint: "https://api.openai.com/v1"
                api_key: "test-api-key"
            }

            model plus from openai {
                id: "model-a"
            }

            model flash from openai {
                id: "model-b"
            }

            agent research {
                model: model.plus
                instruction: "Research this"
                output {
                    value: string
                }
            }

            agent summarize {
                model: model.plus
                context: compact agent.research {
                    model: model.flash
                    instruction: "Compact this"
                }
                instruction: "Summarize this"
                output {
                    value: string
                }
            }

            output {
                result: agent.summarize.value
            }
        };

        let typed_workflow_ir = build_dynamic_typed_workflow_ir(&workflow).expect("workflow should typecheck");
        let execution_plan = build_execution_plan(&workflow, &typed_workflow_ir).expect("workflow should plan");
        let graph = execution_plan.execution_graph(&workflow);

        assert!(graph
            .nodes
            .iter()
            .any(|node| node.id == "compact:summarize" && node.kind == WorkflowExecutionGraphNodeKind::Compact));
        assert!(graph
            .edges
            .iter()
            .any(|edge| edge.source == "research" && edge.target == "compact:summarize"));
        assert!(graph
            .edges
            .iter()
            .any(|edge| edge.source == "model:flash" && edge.target == "compact:summarize"));
        assert!(graph
            .edges
            .iter()
            .any(|edge| edge.source == "compact:summarize" && edge.target == "summarize"));
    }
}
