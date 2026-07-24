use std::collections::{BTreeMap, HashMap};

use superwire_dsl::{SourceSpan, TypeExpression};

use super::super::position::DocumentPosition;
use super::super::scope::CompletionScope;
use super::types::{FieldMetadata, SemanticIndex};

impl SemanticIndex {
    pub fn provider_name_at_position(&self, position: DocumentPosition<'_>) -> Option<&str> {
        self.provider_locations
            .iter()
            .find(|provider_location| position.contains(provider_location.span))
            .map(|provider_location| provider_location.name.as_str())
    }

    pub fn model_name_at_position(&self, position: DocumentPosition<'_>) -> Option<&str> {
        self.model_locations
            .iter()
            .find(|model_location| position.contains(model_location.span))
            .map(|model_location| model_location.name.as_str())
    }

    pub fn schema_name_at_position(&self, position: DocumentPosition<'_>) -> Option<&str> {
        self.schema_locations
            .iter()
            .find(|schema_location| position.contains(schema_location.span))
            .map(|schema_location| schema_location.name.as_str())
    }

    pub fn agent_name_at_position(&self, position: DocumentPosition<'_>) -> Option<&str> {
        self.agent_locations
            .iter()
            .find(|agent_location| position.contains(agent_location.span))
            .map(|agent_location| agent_location.name.as_str())
    }

    pub fn tool_name_at_position(&self, position: DocumentPosition<'_>) -> Option<&str> {
        self.tool_locations
            .iter()
            .find(|tool_location| position.contains(tool_location.span))
            .map(|tool_location| tool_location.name.as_str())
    }

    pub fn completion_scope_at_position(&self, position: DocumentPosition<'_>) -> Option<CompletionScope> {
        if self
            .inference_setting_locations
            .iter()
            .any(|inference_setting_location| position.contains(inference_setting_location.span))
        {
            return Some(CompletionScope::InferenceSettings);
        }

        if self
            .agent_output_locations
            .iter()
            .copied()
            .any(|agent_output_location| position.contains(agent_output_location))
        {
            return Some(CompletionScope::TypedDeclarations);
        }

        if self
            .model_usage_locations
            .iter()
            .copied()
            .any(|model_usage_location| position.contains(model_usage_location))
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
            .any(|mcp_server_location| position.contains(mcp_server_location.span))
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

    pub(in crate::document) fn for_loop_binding_names_at_position(&self, position: DocumentPosition<'_>) -> Option<Vec<&str>> {
        let agent_name = self.agent_name_at_position(position)?;
        let for_loop_bindings = self.agent_for_loop_bindings.get(agent_name)?;

        Some(for_loop_bindings.keys().map(String::as_str).collect())
    }

    pub fn for_loop_binding_types_at_position(&self, position: DocumentPosition<'_>, binding_name: &str) -> Option<&[TypeExpression]> {
        let agent_name = self.agent_name_at_position(position)?;

        self.agent_for_loop_bindings.get(agent_name)?.get(binding_name).map(Vec::as_slice)
    }

    pub fn dynamic_scope_at_position(
        &self,
        position: DocumentPosition<'_>,
    ) -> (&BTreeMap<String, TypeExpression>, &BTreeMap<String, FieldMetadata>) {
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

    pub(in crate::document) fn dynamic_field_locations_at_position(&self, position: DocumentPosition<'_>) -> &HashMap<String, SourceSpan> {
        let Some(agent_name) = self.agent_name_at_position(position) else {
            return &self.dynamic_field_locations;
        };

        self.agent_dynamic_field_locations
            .get(agent_name)
            .unwrap_or(&self.dynamic_field_locations)
    }

    pub fn has_for_loop_binding_at_position(&self, position: DocumentPosition<'_>, binding_name: &str) -> bool {
        self.for_loop_binding_types_at_position(position, binding_name).is_some()
    }
}
