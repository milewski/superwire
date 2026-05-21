use std::collections::{BTreeMap, HashMap};

use lsp_types::Position;
use superwire_core::dsl::{ReferenceKeyword, SourceSpan, TypeExpression};

use super::super::reference::ReferenceCompletionPath;
use super::types::SemanticIndex;

impl SemanticIndex {
    pub fn definition_span_for_symbol_at_cursor(
        &self,
        symbol_token: &str,
        cursor_character_offset: usize,
        position: Position,
    ) -> Option<SourceSpan> {
        if let Some(provider_span) = self.provider_span(symbol_token) {
            return Some(provider_span);
        }

        if let Some(schema_span) = self.schema_span(symbol_token) {
            return Some(schema_span);
        }

        if let Some(agent_span) = self.agent_span(symbol_token) {
            return Some(agent_span);
        }

        let reference_completion_path = ReferenceCompletionPath::from_token(symbol_token)?;
        let selected_segment_index = ReferenceCompletionPath::segment_index_at_cursor(symbol_token, cursor_character_offset)?;

        if reference_completion_path.is_schema_root() {
            return self.schema_reference_definition_span(&reference_completion_path, selected_segment_index);
        }

        if let Some(reference_root_keyword) = reference_completion_path.root_keyword() {
            return self.keyword_reference_definition_span(
                position,
                reference_root_keyword,
                &reference_completion_path,
                selected_segment_index,
            );
        }

        if let Some(for_loop_binding_definition_span) =
            self.for_loop_binding_reference_definition_span(position, &reference_completion_path, selected_segment_index)
        {
            return Some(for_loop_binding_definition_span);
        }

        self.provider_span(reference_completion_path.root_identifier())
    }

    fn for_loop_binding_reference_definition_span(
        &self,
        position: Position,
        reference_completion_path: &ReferenceCompletionPath,
        selected_segment_index: usize,
    ) -> Option<SourceSpan> {
        let binding_name = reference_completion_path.root_identifier();
        let binding_types = self.for_loop_binding_types_at_position(position, binding_name)?;

        if selected_segment_index == 0 {
            return None;
        }

        let selected_accesses = reference_completion_path.resolved_accesses_through_segment(selected_segment_index)?;

        self.field_span_for_type_set_access_path(binding_types, selected_accesses.as_slice())
    }

    fn keyword_reference_definition_span(
        &self,
        position: Position,
        reference_root_keyword: ReferenceKeyword,
        reference_completion_path: &ReferenceCompletionPath,
        selected_segment_index: usize,
    ) -> Option<SourceSpan> {
        match reference_root_keyword {
            ReferenceKeyword::Dynamic => {
                self.dynamic_reference_definition_span(position, reference_completion_path, selected_segment_index)
            }
            ReferenceKeyword::Input => self.singleton_reference_definition_span(
                reference_completion_path,
                selected_segment_index,
                &self.input_fields,
                &self.input_field_locations,
            ),
            ReferenceKeyword::Secrets => self.singleton_reference_definition_span(
                reference_completion_path,
                selected_segment_index,
                &self.secrets_fields,
                &self.secrets_field_locations,
            ),
            ReferenceKeyword::Agent => self.agent_reference_definition_span(reference_completion_path, selected_segment_index),
            ReferenceKeyword::Tool => self.tool_reference_definition_span(reference_completion_path, selected_segment_index),
            ReferenceKeyword::Model => None,
            ReferenceKeyword::Resource => self.resource_reference_definition_span(reference_completion_path, selected_segment_index),
            ReferenceKeyword::Prompt => self.prompt_reference_definition_span(reference_completion_path, selected_segment_index),
        }
    }

    fn dynamic_reference_definition_span(
        &self,
        position: Position,
        reference_completion_path: &ReferenceCompletionPath,
        selected_segment_index: usize,
    ) -> Option<SourceSpan> {
        let selected_accesses = reference_completion_path.resolved_accesses_through_segment(selected_segment_index)?;
        let dynamic_field_name = selected_accesses.first()?;
        let dynamic_field_locations = self.dynamic_field_locations_at_position(position);
        let dynamic_field_span = dynamic_field_locations.get(dynamic_field_name).copied()?;

        if selected_accesses.len() == 1 {
            return Some(dynamic_field_span);
        }

        let (dynamic_fields, _) = self.dynamic_scope_at_position(position);
        let dynamic_field_type = dynamic_fields.get(dynamic_field_name)?;

        self.field_span_for_type_access_path(dynamic_field_type, &selected_accesses[1..])
            .or(Some(dynamic_field_span))
    }

    fn tool_reference_definition_span(
        &self,
        reference_completion_path: &ReferenceCompletionPath,
        selected_segment_index: usize,
    ) -> Option<SourceSpan> {
        let selected_accesses = reference_completion_path.resolved_accesses_through_segment(selected_segment_index)?;
        let tool_name = selected_accesses.first()?;

        if selected_accesses.len() == 1 {
            return self.tool_span(tool_name);
        }

        let tool_summary = self.tools.get(tool_name)?;
        let output_type_expression = tool_summary.output_type_expression.as_ref()?;

        self.field_span_for_type_access_path(output_type_expression, &selected_accesses[1..])
    }

    fn resource_reference_definition_span(
        &self,
        reference_completion_path: &ReferenceCompletionPath,
        selected_segment_index: usize,
    ) -> Option<SourceSpan> {
        let selected_accesses = reference_completion_path.resolved_accesses_through_segment(selected_segment_index)?;
        let resource_name = selected_accesses.first()?;

        if selected_accesses.len() == 1 {
            return self.resource_span(resource_name);
        }

        None
    }

    fn prompt_reference_definition_span(
        &self,
        reference_completion_path: &ReferenceCompletionPath,
        selected_segment_index: usize,
    ) -> Option<SourceSpan> {
        let selected_accesses = reference_completion_path.resolved_accesses_through_segment(selected_segment_index)?;
        let prompt_name = selected_accesses.first()?;

        if selected_accesses.len() == 1 {
            return self.prompt_span(prompt_name);
        }

        None
    }

    fn schema_reference_definition_span(
        &self,
        reference_completion_path: &ReferenceCompletionPath,
        selected_segment_index: usize,
    ) -> Option<SourceSpan> {
        let selected_accesses = reference_completion_path.resolved_accesses_through_segment(selected_segment_index)?;
        let schema_name = selected_accesses.first()?;

        if selected_accesses.len() == 1 {
            return self.schema_span(schema_name);
        }

        self.schema_field_span(schema_name, &selected_accesses[1..])
    }

    fn singleton_reference_definition_span(
        &self,
        reference_completion_path: &ReferenceCompletionPath,
        selected_segment_index: usize,
        root_fields: &BTreeMap<String, TypeExpression>,
        root_field_locations: &HashMap<String, SourceSpan>,
    ) -> Option<SourceSpan> {
        let selected_accesses = reference_completion_path.resolved_accesses_through_segment(selected_segment_index)?;

        if selected_accesses.is_empty() {
            return None;
        }

        let field_location_key = Self::field_location_key(selected_accesses.as_slice());

        if let Some(field_span) = root_field_locations.get(&field_location_key) {
            return Some(*field_span);
        }

        let root_field_name = selected_accesses.first()?;
        let root_field_type = root_fields.get(root_field_name)?;

        if selected_accesses.len() == 1 {
            return None;
        }

        self.field_span_for_type_access_path(root_field_type, &selected_accesses[1..])
    }

    fn agent_reference_definition_span(
        &self,
        reference_completion_path: &ReferenceCompletionPath,
        selected_segment_index: usize,
    ) -> Option<SourceSpan> {
        let selected_accesses = reference_completion_path.resolved_accesses_through_segment(selected_segment_index)?;
        let agent_name = selected_accesses.first()?;

        if selected_accesses.len() == 1 {
            return self.agent_span(agent_name);
        }

        let agent_field_location_key = Self::field_location_key(selected_accesses.as_slice());

        if let Some(field_span) = self.agent_output_field_locations.get(&agent_field_location_key) {
            return Some(*field_span);
        }

        let agent_output_type = self.agents.get(agent_name)?.output_type.as_ref()?;

        self.field_span_for_type_access_path(agent_output_type, &selected_accesses[1..])
    }

    fn schema_field_span(&self, schema_name: &str, field_accesses: &[String]) -> Option<SourceSpan> {
        if field_accesses.is_empty() {
            return self.schema_span(schema_name);
        }

        let mut schema_field_location_segments = Self::schema_field_location_prefix(schema_name);
        schema_field_location_segments.extend(field_accesses.iter().cloned());

        let schema_field_location_key = Self::field_location_key(schema_field_location_segments.as_slice());

        if let Some(field_span) = self.schema_field_locations.get(&schema_field_location_key) {
            return Some(*field_span);
        }

        let schema_summary = self.schemas.get(schema_name)?;
        let first_field_name = field_accesses.first()?;
        let first_field_type = schema_summary.fields.get(first_field_name)?;

        if field_accesses.len() == 1 {
            return None;
        }

        self.field_span_for_type_access_path(first_field_type, &field_accesses[1..])
    }

    fn field_span_for_type_access_path(&self, root_type_expression: &TypeExpression, field_accesses: &[String]) -> Option<SourceSpan> {
        if field_accesses.is_empty() {
            return None;
        }

        match root_type_expression {
            TypeExpression::Object(typed_fields) => {
                let first_field_name = field_accesses.first()?;
                let typed_field = typed_fields.iter().find(|typed_field| typed_field.name == *first_field_name)?;

                if field_accesses.len() == 1 {
                    return Some(typed_field.span);
                }

                self.field_span_for_type_access_path(&typed_field.field_type, &field_accesses[1..])
            }
            TypeExpression::SchemaReference(schema_name) => self.schema_field_span(schema_name, field_accesses),
            TypeExpression::Variant { discriminator, cases } => {
                if field_accesses.len() == 1 && field_accesses.first().is_some_and(|field_access| field_access == discriminator) {
                    return cases.first().map(|variant_case| variant_case.span);
                }

                None
            }
            TypeExpression::Union(union_members) => {
                for union_member in union_members {
                    if let Some(field_span) = self.field_span_for_type_access_path(union_member, field_accesses) {
                        return Some(field_span);
                    }
                }

                None
            }
            TypeExpression::String
            | TypeExpression::Number
            | TypeExpression::Float
            | TypeExpression::Boolean
            | TypeExpression::Null
            | TypeExpression::AnyObject
            | TypeExpression::StringEnum(_)
            | TypeExpression::StringEnumReference(_)
            | TypeExpression::Array {
                item_type: _,
                fixed_length: _,
            }
            | TypeExpression::Tuple(_) => None,
        }
    }

    fn field_span_for_type_set_access_path(
        &self,
        root_type_expressions: &[TypeExpression],
        field_accesses: &[String],
    ) -> Option<SourceSpan> {
        for root_type_expression in root_type_expressions {
            if let Some(field_span) = self.field_span_for_type_access_path(root_type_expression, field_accesses) {
                return Some(field_span);
            }
        }

        None
    }

    fn provider_span(&self, provider_name: &str) -> Option<SourceSpan> {
        self.provider_locations
            .iter()
            .find(|provider_location| provider_location.name == provider_name)
            .map(|provider_location| provider_location.span)
    }

    fn schema_span(&self, schema_name: &str) -> Option<SourceSpan> {
        self.schema_locations
            .iter()
            .find(|schema_location| schema_location.name == schema_name)
            .map(|schema_location| schema_location.span)
    }

    fn agent_span(&self, agent_name: &str) -> Option<SourceSpan> {
        self.agent_locations
            .iter()
            .find(|agent_location| agent_location.name == agent_name)
            .map(|agent_location| agent_location.span)
    }

    fn tool_span(&self, tool_name: &str) -> Option<SourceSpan> {
        self.tool_locations
            .iter()
            .find(|tool_location| tool_location.name == tool_name)
            .map(|tool_location| tool_location.span)
    }

    fn resource_span(&self, resource_name: &str) -> Option<SourceSpan> {
        self.resource_locations
            .iter()
            .find(|resource_location| resource_location.name == resource_name)
            .map(|resource_location| resource_location.span)
    }

    fn prompt_span(&self, prompt_name: &str) -> Option<SourceSpan> {
        self.prompt_locations
            .iter()
            .find(|prompt_location| prompt_location.name == prompt_name)
            .map(|prompt_location| prompt_location.span)
    }
}
