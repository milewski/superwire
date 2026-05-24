use std::collections::{BTreeMap, HashMap};

use lsp_types::Position;
use superwire_dsl::{SourceSpan, TypeExpression};

use super::super::position::source_span_contains_position;
use super::super::scope::CompletionScope;
use super::types::{FieldMetadata, SemanticIndex};

impl SemanticIndex {
    pub fn provider_name_at_position(&self, position: Position) -> Option<&str> {
        self.provider_locations
            .iter()
            .find(|provider_location| source_span_contains_position(provider_location.span, position))
            .map(|provider_location| provider_location.name.as_str())
    }

    pub fn model_name_at_position(&self, position: Position) -> Option<&str> {
        self.model_locations
            .iter()
            .find(|model_location| source_span_contains_position(model_location.span, position))
            .map(|model_location| model_location.name.as_str())
    }

    pub fn schema_name_at_position(&self, position: Position) -> Option<&str> {
        self.schema_locations
            .iter()
            .find(|schema_location| source_span_contains_position(schema_location.span, position))
            .map(|schema_location| schema_location.name.as_str())
    }

    pub fn agent_name_at_position(&self, position: Position) -> Option<&str> {
        self.agent_locations
            .iter()
            .find(|agent_location| source_span_contains_position(agent_location.span, position))
            .map(|agent_location| agent_location.name.as_str())
    }

    pub fn tool_name_at_position(&self, position: Position) -> Option<&str> {
        self.tool_locations
            .iter()
            .find(|tool_location| source_span_contains_position(tool_location.span, position))
            .map(|tool_location| tool_location.name.as_str())
    }

    pub fn completion_scope_at_position(&self, position: Position) -> Option<CompletionScope> {
        if self
            .inference_setting_locations
            .iter()
            .any(|inference_setting_location| source_span_contains_position(inference_setting_location.span, position))
        {
            return Some(CompletionScope::InferenceSettings);
        }

        if self
            .agent_output_locations
            .iter()
            .copied()
            .any(|agent_output_location| source_span_contains_position(agent_output_location, position))
        {
            return Some(CompletionScope::TypedDeclarations);
        }

        if self
            .model_usage_locations
            .iter()
            .copied()
            .any(|model_usage_location| source_span_contains_position(model_usage_location, position))
        {
            return Some(CompletionScope::ModelUsageProperties);
        }

        if self.model_name_at_position(position).is_some() {
            return Some(CompletionScope::ModelProperties);
        }

        if self.provider_name_at_position(position).is_some() {
            return Some(CompletionScope::ProviderProperties);
        }

        if self
            .mcp_server_locations
            .iter()
            .any(|mcp_server_location| source_span_contains_position(mcp_server_location.span, position))
        {
            return Some(CompletionScope::McpServerProperties);
        }

        if self.tool_name_at_position(position).is_some() {
            return Some(CompletionScope::ToolProperties);
        }

        if self.agent_name_at_position(position).is_some() {
            return Some(CompletionScope::AgentProperties);
        }

        None
    }

    pub(in crate::document) fn for_loop_binding_names_at_position(&self, position: Position) -> Option<Vec<&str>> {
        let agent_name = self.agent_name_at_position(position)?;
        let for_loop_bindings = self.agent_for_loop_bindings.get(agent_name)?;

        Some(for_loop_bindings.keys().map(String::as_str).collect())
    }

    pub fn for_loop_binding_types_at_position(&self, position: Position, binding_name: &str) -> Option<&[TypeExpression]> {
        let agent_name = self.agent_name_at_position(position)?;

        self.agent_for_loop_bindings.get(agent_name)?.get(binding_name).map(Vec::as_slice)
    }

    pub fn dynamic_scope_at_position(&self, position: Position) -> (&BTreeMap<String, TypeExpression>, &BTreeMap<String, FieldMetadata>) {
        let Some(agent_name) = self.agent_name_at_position(position) else {
            return (&self.dynamic_fields, &self.dynamic_field_metadata);
        };

        let scoped_dynamic_fields = self.agent_dynamic_fields.get(agent_name).unwrap_or(&self.dynamic_fields);
        let scoped_dynamic_field_metadata = self
            .agent_dynamic_field_metadata
            .get(agent_name)
            .unwrap_or(&self.dynamic_field_metadata);

        (scoped_dynamic_fields, scoped_dynamic_field_metadata)
    }

    pub(in crate::document) fn dynamic_field_locations_at_position(&self, position: Position) -> &HashMap<String, SourceSpan> {
        let Some(agent_name) = self.agent_name_at_position(position) else {
            return &self.dynamic_field_locations;
        };

        self.agent_dynamic_field_locations
            .get(agent_name)
            .unwrap_or(&self.dynamic_field_locations)
    }

    pub fn has_for_loop_binding_at_position(&self, position: Position, binding_name: &str) -> bool {
        self.for_loop_binding_types_at_position(position, binding_name).is_some()
    }
}
