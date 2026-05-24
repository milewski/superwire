use std::collections::{BTreeMap, HashMap};

use superwire_core::mcp::{McpLock, McpServerToolLookup};
use superwire_dsl::{SourceSpan, TypeExpression};
use superwire_semantic::{ProviderDriver, SemanticToolingSnapshot, WorkflowSemanticIndex};

#[derive(Debug, Clone)]
pub struct SemanticIndex {
    pub providers: HashMap<String, ProviderSummary>,
    pub provider_locations: Vec<NamedSpan>,
    pub(super) provider_location_index: HashMap<String, SourceSpan>,
    pub models: HashMap<String, ModelSummary>,
    pub model_locations: Vec<NamedSpan>,
    pub(super) model_location_index: HashMap<String, SourceSpan>,
    pub schemas: HashMap<String, SchemaSummary>,
    pub schema_names: Vec<String>,
    pub schema_locations: Vec<NamedSpan>,
    pub(super) schema_location_index: HashMap<String, SourceSpan>,
    pub(super) schema_field_locations: HashMap<String, SourceSpan>,
    pub tools: HashMap<String, ToolSummary>,
    pub tool_names: Vec<String>,
    pub tool_locations: Vec<NamedSpan>,
    pub(super) tool_location_index: HashMap<String, SourceSpan>,
    pub resource_names: Vec<String>,
    pub resource_locations: Vec<NamedSpan>,
    pub(super) resource_location_index: HashMap<String, SourceSpan>,
    pub prompt_names: Vec<String>,
    pub prompt_locations: Vec<NamedSpan>,
    pub(super) prompt_location_index: HashMap<String, SourceSpan>,
    pub mcp_server_names: Vec<String>,
    pub mcp_server_locations: Vec<NamedSpan>,
    pub input_fields: BTreeMap<String, TypeExpression>,
    pub input_field_metadata: BTreeMap<String, FieldMetadata>,
    pub(super) input_field_locations: HashMap<String, SourceSpan>,
    pub secrets_fields: BTreeMap<String, TypeExpression>,
    pub secrets_field_metadata: BTreeMap<String, FieldMetadata>,
    pub(super) secrets_field_locations: HashMap<String, SourceSpan>,
    pub dynamic_fields: BTreeMap<String, TypeExpression>,
    pub dynamic_field_metadata: BTreeMap<String, FieldMetadata>,
    pub(super) dynamic_field_locations: HashMap<String, SourceSpan>,
    pub agents: HashMap<String, AgentSummary>,
    pub agent_dynamic_fields: HashMap<String, BTreeMap<String, TypeExpression>>,
    pub agent_dynamic_field_metadata: HashMap<String, BTreeMap<String, FieldMetadata>>,
    pub(super) agent_dynamic_field_locations: HashMap<String, HashMap<String, SourceSpan>>,
    pub(super) agent_output_field_locations: HashMap<String, SourceSpan>,
    pub agent_for_loop_bindings: HashMap<String, BTreeMap<String, Vec<TypeExpression>>>,
    pub agent_for_loop_iterable_item_types: HashMap<String, TypeExpression>,
    pub agent_names: Vec<String>,
    pub model_usage_locations: Vec<SourceSpan>,
    pub inference_setting_locations: Vec<NamedSpan>,
    pub output_locations: Vec<SourceSpan>,
    pub typed_declaration_locations: Vec<SourceSpan>,
    pub agent_output_locations: Vec<SourceSpan>,
    pub agent_locations: Vec<NamedSpan>,
    pub(super) agent_location_index: HashMap<String, SourceSpan>,
    pub(super) has_input_declaration: bool,
    pub(super) has_secrets_declaration: bool,
    pub(super) has_output_declaration: bool,
    pub tooling_snapshot: SemanticToolingSnapshot,
    pub mcp_lock: Option<McpLock>,
    pub(super) mcp_server_tool_lookups: HashMap<String, McpServerToolLookup>,
    pub workflow_semantics: Option<WorkflowSemanticIndex>,
}

#[derive(Debug, Clone)]
pub struct ProviderSummary {
    pub driver: Option<ProviderDriver>,
}

#[derive(Debug, Clone)]
pub struct ModelSummary {
    pub provider_name: String,
    pub model_identifier: Option<String>,
}

impl ModelSummary {
    pub(in crate::document) fn completion_detail(&self) -> String {
        self.model_identifier
            .clone()
            .unwrap_or_else(|| "Declared model profile".to_string())
    }
}

#[derive(Debug, Clone)]
pub struct SchemaSummary {
    pub fields: BTreeMap<String, TypeExpression>,
    pub field_metadata: BTreeMap<String, FieldMetadata>,
}

#[derive(Debug, Clone)]
pub struct ToolSummary {
    pub description: Option<String>,
    pub bounded_fields: BTreeMap<String, TypeExpression>,
    pub bounded_field_metadata: BTreeMap<String, FieldMetadata>,
    pub output_type_expression: Option<TypeExpression>,
    pub mcp_server_name: Option<String>,
    pub mcp_tool_name: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AgentSummary {
    pub output_type: Option<TypeExpression>,
}

#[derive(Debug, Clone)]
pub struct FieldMetadata {
    pub field_type: TypeExpression,
    pub description: Option<String>,
}

#[derive(Debug, Clone)]
pub struct NamedSpan {
    pub name: String,
    pub span: SourceSpan,
}

impl NamedSpan {
    pub(super) fn push_indexed(
        location_collection: &mut Vec<Self>,
        location_index: &mut HashMap<String, SourceSpan>,
        name: impl Into<String>,
        span: SourceSpan,
    ) {
        let location_name = name.into();

        location_index.entry(location_name.clone()).or_insert(span);

        location_collection.push(Self { name: location_name, span });
    }

    pub(super) fn first_span_map(locations: &[Self]) -> HashMap<String, SourceSpan> {
        let mut location_index = HashMap::new();

        for location in locations {
            location_index.entry(location.name.clone()).or_insert(location.span);
        }

        location_index
    }
}
