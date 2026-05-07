use superwire_core::dsl::{
    parse_workflow, AgentExpressionPropertyName, AgentPropertyName, AgentResponseFormat, DeclarationKeyword, ForClauseKeyword,
    ImportKeyword, ReferenceKeyword, ToolCallKeyword, ToolPropertyName,
};

use crate::protocol::{Position, Range};

use super::completion_context::{
    AgentPropertyValueCompletionContext, ArrayFixedLengthCompletionContext, DeclarationHeaderCompletionContext,
    ForLoopDestructuringBindingCompletionContext, ForLoopIterableValueCompletionContext, InferenceSettingValueCompletionContext,
    ModelCallCompletionContext, OutputValueCompletionContext, ToolCallCompletionContext, ValueCompletionContext,
};
use super::position::byte_offset_for_position;
use super::reference::{ReferenceCompletionConstraint, ReferenceCompletionPath};
use super::scope::{
    agent_property_scope_suggestions, completion_scope_at_offset, inference_setting_scope_suggestions,
    mcp_tool_batch_import_scope_suggestions, tool_property_scope_suggestions, CompletionScope,
};
use super::semantic_index::SemanticIndex;
use super::text_utils::{
    is_inside_interpolation_expression, is_inside_multiline_string_literal, trailing_identifier, trailing_reference_token,
};
use super::{CompletionKind, CompletionSuggestion, DocumentState, RenderTypeExpression};
use superwire_core::semantic::InferenceSetting;

const COMPLETION_RECOVERY_PLACEHOLDER: &str = "__completion_placeholder";

struct ToolCallBindingCompletionContext {
    tool_name: String,
    binding_prefix: String,
    existing_binding_names: Vec<String>,
}

struct McpToolBatchItemCompletionContext {
    server_name: String,
    tool_prefix: String,
}

pub(super) enum McpToolSchemaSource {
    LocalTool(String),
    McpTool { server_name: Option<String>, tool_name: String },
    McpToolBatch { server_name: String, tool_names: Vec<String> },
}

struct ReferenceCompletionInputs<'completion> {
    line_prefix: &'completion str,
    line_suffix: &'completion str,
    position: Position,
    completion_scope: CompletionScope,
    inside_interpolation_expression: bool,
    inference_setting_value_completion_context: Option<&'completion InferenceSettingValueCompletionContext>,
}

impl DocumentState {
    #[must_use]
    pub fn completion_suggestions(&self, position: Position) -> Vec<CompletionSuggestion> {
        let Some(line_prefix) = self.line_prefix(position) else {
            return Vec::new();
        };
        let line_suffix = self.line_suffix(position).unwrap_or_default();

        let inside_interpolation_expression = is_inside_interpolation_expression(&line_prefix);

        if self.is_inside_multiline_string_literal(position) && !inside_interpolation_expression {
            return Vec::new();
        }

        let completion_scope = self.completion_scope(position);
        let semantic_index = self.semantic_index_for_completion(position);

        if Self::is_typed_description_string_literal_context(&line_prefix, completion_scope, &semantic_index, position) {
            return Vec::new();
        }

        if !inside_interpolation_expression {
            if let Some(tool_call_binding_completion_context) = self.tool_call_binding_completion_context(position, &line_prefix) {
                return semantic_index.tool_bounded_argument_suggestions(
                    &tool_call_binding_completion_context.tool_name,
                    &tool_call_binding_completion_context.binding_prefix,
                    &tool_call_binding_completion_context.existing_binding_names,
                );
            }
        }

        if !inside_interpolation_expression && !line_prefix.contains(':') {
            if let Some(mcp_tool_schema_field_suggestions) = self.mcp_tool_schema_field_suggestions(position, &line_prefix, &semantic_index)
            {
                return mcp_tool_schema_field_suggestions;
            }
        }

        if !inside_interpolation_expression {
            if let Some(typed_declaration_suggestions) =
                self.typed_declaration_scope_suggestions(completion_scope, &line_prefix, position, &semantic_index)
            {
                return typed_declaration_suggestions;
            }
        }

        let line_has_property_separator = line_prefix.trim_start().contains(':');
        let should_include_builtin_function_suggestions = line_has_property_separator || inside_interpolation_expression;
        let inference_setting_value_completion_context =
            self.inference_setting_value_completion_context(line_has_property_separator, &line_prefix);

        if let Some(non_reference_suggestions) = self.non_reference_suggestions(
            &semantic_index,
            &line_prefix,
            position,
            completion_scope,
            line_has_property_separator,
            inside_interpolation_expression,
        ) {
            return non_reference_suggestions;
        }

        if let Some(reference_suggestions) = self.reference_completion_suggestions(
            &semantic_index,
            ReferenceCompletionInputs {
                line_prefix: &line_prefix,
                line_suffix: &line_suffix,
                position,
                completion_scope,
                inside_interpolation_expression,
                inference_setting_value_completion_context: inference_setting_value_completion_context.as_ref(),
            },
        ) {
            return reference_suggestions;
        }

        if inside_interpolation_expression {
            return semantic_index.interpolation_root_suggestions("", position);
        }

        if semantic_index.is_type_position(position, &line_prefix) {
            let current_schema_name = semantic_index.schema_name_at_position(position);
            let type_suggestions = semantic_index.type_suggestions(&line_prefix, current_schema_name);

            if !type_suggestions.is_empty() {
                return type_suggestions;
            }
        }

        semantic_index.default_suggestions(should_include_builtin_function_suggestions)
    }

    fn is_typed_description_string_literal_context(
        line_prefix: &str,
        completion_scope: CompletionScope,
        semantic_index: &SemanticIndex,
        position: Position,
    ) -> bool {
        let trimmed_line_prefix = line_prefix.trim_start();
        let Some((line_before_value, value_prefix)) = trimmed_line_prefix.rsplit_once(':') else {
            return false;
        };

        let value_completion_context = ValueCompletionContext::from_value_prefix(value_prefix);

        if !value_completion_context.inside_string_literal {
            return false;
        }

        let property_name_identifier = trailing_identifier(line_before_value).unwrap_or_default();
        let inside_agent_output_type = semantic_index.agent_name_at_position(position).is_some()
            && AgentPropertyName::from_identifier(property_name_identifier) == Some(AgentPropertyName::Output);

        if completion_scope != CompletionScope::TypedDeclarations && !inside_agent_output_type {
            return false;
        }

        let trimmed_value_prefix = value_prefix.trim_start();
        let Some(last_quote_index) = trimmed_value_prefix.rfind('"') else {
            return false;
        };

        let value_before_open_quote = trimmed_value_prefix[..last_quote_index].trim_end();

        !value_before_open_quote.is_empty()
    }

    #[must_use]
    pub fn completion_text_edit_range(&self, position: Position) -> Option<Range> {
        let line_prefix = self.line_prefix(position)?;

        if let Some(model_call_completion_context) = ModelCallCompletionContext::from_line_prefix(&line_prefix) {
            return Some(Self::text_edit_range_for_prefix(
                position,
                &model_call_completion_context.model_prefix,
            ));
        }

        let trimmed_line_prefix = line_prefix.trim_start();

        if let Some((_, value_prefix)) = trimmed_line_prefix.rsplit_once(':') {
            let value_completion_context = ValueCompletionContext::from_value_prefix(value_prefix);
            let semantic_index = self.semantic_index_for_completion(position);

            if value_completion_context.value_prefix.is_empty() {
                return Some(Self::text_edit_range_for_prefix(position, ""));
            }

            if let Some(reference_completion_path) = ReferenceCompletionPath::from_line_prefix(&line_prefix) {
                let reference_token = trailing_reference_token(&line_prefix).unwrap_or_default();

                if reference_token.ends_with('.') {
                    return Some(Self::text_edit_range_for_prefix(
                        position,
                        &reference_completion_path.pending_prefix,
                    ));
                }

                if reference_completion_path.complete_accesses.is_empty() && reference_completion_path.pending_prefix.is_empty() {
                    return Some(Self::text_edit_range_for_prefix(
                        position,
                        reference_completion_path.root_identifier(),
                    ));
                }

                return Some(Self::text_edit_range_for_prefix(
                    position,
                    &reference_completion_path.pending_prefix,
                ));
            }

            if semantic_index.is_type_position(position, &line_prefix) {
                let type_prefix = trailing_reference_token(&value_completion_context.value_prefix).unwrap_or_default();

                return Some(Self::text_edit_range_for_prefix(position, type_prefix));
            }

            return Some(Self::text_edit_range_for_prefix(position, &value_completion_context.value_prefix));
        }

        let identifier_prefix = trailing_identifier(&line_prefix).unwrap_or_default();

        Some(Self::text_edit_range_for_prefix(position, identifier_prefix))
    }

    fn text_edit_range_for_prefix(position: Position, value_prefix: &str) -> Range {
        let value_prefix_character_count = u32::try_from(value_prefix.chars().count()).unwrap_or_default();
        let start_character = position.character.saturating_sub(value_prefix_character_count);

        Range {
            start: Position {
                line: position.line,
                character: start_character,
            },
            end: position,
        }
    }

    fn typed_declaration_scope_suggestions(
        &self,
        completion_scope: CompletionScope,
        line_prefix: &str,
        position: Position,
        semantic_index: &SemanticIndex,
    ) -> Option<Vec<CompletionSuggestion>> {
        if completion_scope != CompletionScope::TypedDeclarations {
            return None;
        }

        if !line_prefix.contains(':') {
            let trimmed_prefix = line_prefix.trim_start();

            if let Some(mcp_tool_schema_source) = self.mcp_tool_schema_source_at_position(position, semantic_index) {
                let mcp_suggestions =
                    self.mcp_tool_schema_field_suggestions_for_source(semantic_index, position, trimmed_prefix, mcp_tool_schema_source);

                if !mcp_suggestions.is_empty() {
                    return Some(mcp_suggestions);
                }
            }

            return Some(Vec::new());
        }

        if ArrayFixedLengthCompletionContext::from_line_prefix(line_prefix).is_some() {
            return Some(Vec::new());
        }

        let current_schema_name = semantic_index.schema_name_at_position(position);

        Some(semantic_index.type_suggestions(line_prefix, current_schema_name))
    }

    fn non_reference_suggestions(
        &self,
        semantic_index: &SemanticIndex,
        line_prefix: &str,
        position: Position,
        completion_scope: CompletionScope,
        line_has_property_separator: bool,
        inside_interpolation_expression: bool,
    ) -> Option<Vec<CompletionSuggestion>> {
        if !inside_interpolation_expression {
            if let Some(for_loop_destructuring_binding_completion_context) =
                ForLoopDestructuringBindingCompletionContext::from_line_prefix(line_prefix)
            {
                return Some(semantic_index.for_loop_destructuring_binding_suggestions(
                    position,
                    &for_loop_destructuring_binding_completion_context.field_prefix,
                    &for_loop_destructuring_binding_completion_context.existing_field_names,
                ));
            }

            if let Some(batch_item_suggestions) = self.mcp_tool_batch_item_suggestions(semantic_index, position, line_prefix) {
                return Some(batch_item_suggestions);
            }
        }

        if completion_scope == CompletionScope::General
            && !line_has_property_separator
            && !inside_interpolation_expression
            && semantic_index.is_output_position(position)
        {
            return Some(Vec::new());
        }

        if line_has_property_separator {
            if let Some(property_value_suggestions) =
                Self::property_value_non_reference_suggestions(semantic_index, line_prefix, completion_scope)
            {
                return Some(property_value_suggestions);
            }

            if !inside_interpolation_expression && Self::should_suppress_prompt_string_literal_suggestions(line_prefix) {
                return Some(Vec::new());
            }

            if semantic_index.is_output_position(position) && !inside_interpolation_expression {
                if let Some(output_value_completion_context) = OutputValueCompletionContext::from_line_prefix(line_prefix) {
                    if ReferenceCompletionPath::from_line_prefix(line_prefix).is_none() {
                        return Some(semantic_index.output_value_suggestions(&output_value_completion_context.value_prefix));
                    }
                }
            }
        }

        if let Some(tool_call_completion_context) = ToolCallCompletionContext::from_line_prefix(line_prefix) {
            return Some(semantic_index.tool_bounded_argument_suggestions(
                &tool_call_completion_context.tool_name,
                &tool_call_completion_context.argument_prefix,
                &tool_call_completion_context.existing_argument_names,
            ));
        }

        if let Some(model_call_context) = ModelCallCompletionContext::from_line_prefix(line_prefix) {
            let mut model_suggestions = semantic_index.model_call_suggestions(&model_call_context);

            if !model_call_context.inside_string_literal {
                model_suggestions.extend(semantic_index.model_value_root_suggestions(&model_call_context.model_prefix));
                model_suggestions.sort_by(|left_suggestion, right_suggestion| {
                    left_suggestion
                        .label
                        .cmp(&right_suggestion.label)
                        .then_with(|| left_suggestion.insert_text.cmp(&right_suggestion.insert_text))
                });
                model_suggestions.dedup_by(|left_suggestion, right_suggestion| {
                    left_suggestion.label == right_suggestion.label
                        && left_suggestion.insert_text == right_suggestion.insert_text
                        && left_suggestion.detail == right_suggestion.detail
                });
            }

            if !model_suggestions.is_empty() {
                return Some(model_suggestions);
            }
        }

        if semantic_index.agent_name_at_position(position).is_some() && line_has_property_separator && !inside_interpolation_expression {
            if let Some(agent_property_suggestions) =
                self.agent_property_value_suggestions(semantic_index, line_prefix, inside_interpolation_expression)
            {
                return Some(agent_property_suggestions);
            }
        }

        if !line_has_property_separator && !inside_interpolation_expression {
            if completion_scope == CompletionScope::DynamicValues {
                return Some(Vec::new());
            }

            if let Some(for_loop_iterable_value_completion_context) = ForLoopIterableValueCompletionContext::from_line_prefix(line_prefix) {
                if ReferenceCompletionPath::from_line_prefix(line_prefix).is_none() {
                    return Some(
                        semantic_index.for_loop_iterable_value_suggestions(&for_loop_iterable_value_completion_context.value_prefix),
                    );
                }
            }

            if let Some(declaration_header_completion_context) = DeclarationHeaderCompletionContext::from_line_prefix(line_prefix) {
                return Some(declaration_header_completion_context.completion_suggestions());
            }

            if Self::should_defer_to_reference_completion(line_prefix) {
                return None;
            }

            if let Some(scope_suggestions) = self.property_scope_suggestions(semantic_index, completion_scope, line_prefix, position) {
                return Some(scope_suggestions);
            }

            if completion_scope == CompletionScope::General && semantic_index.is_root_declaration_position(position) {
                return Some(semantic_index.root_declaration_suggestions(line_prefix));
            }
        }

        self.provider_non_reference_suggestions(semantic_index, line_prefix, position)
    }

    fn mcp_tool_schema_field_suggestions(
        &self,
        position: Position,
        line_prefix: &str,
        semantic_index: &SemanticIndex,
    ) -> Option<Vec<CompletionSuggestion>> {
        self.tool_schema_property_name_at_position(position)?;
        let mcp_tool_schema_source = self.mcp_tool_schema_source_at_position(position, semantic_index)?;

        Some(self.mcp_tool_schema_field_suggestions_for_source(semantic_index, position, line_prefix.trim_start(), mcp_tool_schema_source))
    }

    fn mcp_tool_schema_field_suggestions_for_source(
        &self,
        semantic_index: &SemanticIndex,
        position: Position,
        trimmed_prefix: &str,
        mcp_tool_schema_source: McpToolSchemaSource,
    ) -> Vec<CompletionSuggestion> {
        let existing_fields = self.existing_typed_field_names(position);
        let field_prefix = super::text_utils::trailing_identifier(trimmed_prefix).unwrap_or_default();
        let Some(tool_property_name) = self.tool_schema_property_name_at_position(position) else {
            return Vec::new();
        };

        match mcp_tool_schema_source {
            McpToolSchemaSource::LocalTool(tool_name) => {
                semantic_index.mcp_tool_schema_field_suggestions(&tool_name, tool_property_name, field_prefix, &existing_fields)
            }
            McpToolSchemaSource::McpTool { server_name, tool_name } => semantic_index
                .mcp_tool_schema_fields_for_source(server_name.as_deref(), &tool_name, tool_property_name)
                .iter()
                .filter(|typed_field| typed_field.name.starts_with(field_prefix))
                .filter(|typed_field| !existing_fields.contains(&typed_field.name))
                .map(|typed_field| Self::mcp_tool_schema_field_suggestion(typed_field, tool_property_name))
                .collect(),
            McpToolSchemaSource::McpToolBatch { server_name, tool_names } => semantic_index
                .mcp_tool_batch_common_schema_fields(&server_name, &tool_names, tool_property_name)
                .iter()
                .filter(|typed_field| typed_field.name.starts_with(field_prefix))
                .filter(|typed_field| !existing_fields.contains(&typed_field.name))
                .map(|typed_field| Self::mcp_tool_schema_field_suggestion(typed_field, tool_property_name))
                .collect(),
        }
    }

    fn mcp_tool_schema_field_suggestion(
        typed_field: &superwire_core::dsl::TypedField,
        tool_property_name: ToolPropertyName,
    ) -> CompletionSuggestion {
        let rendered_type = typed_field.field_type.render_type();
        let insert_text = if tool_property_name == ToolPropertyName::Bindings {
            format!("{}: $1", typed_field.name)
        } else {
            format!("{}: {rendered_type}", typed_field.name)
        };

        CompletionSuggestion {
            label: typed_field.name.clone(),
            kind: CompletionKind::Property,
            detail: typed_field.description.clone().unwrap_or_else(|| rendered_type.clone()),
            documentation: typed_field
                .description
                .clone()
                .unwrap_or_else(|| format!("MCP tool {} field of type `{rendered_type}`.", tool_property_name.as_str())),
            insert_text,
        }
    }

    pub(super) fn tool_schema_property_name_at_position(&self, position: Position) -> Option<ToolPropertyName> {
        let cursor_offset = byte_offset_for_position(&self.text, position)?;
        let source_prefix = &self.text[..cursor_offset];

        [ToolPropertyName::Input, ToolPropertyName::Bindings, ToolPropertyName::Output]
            .into_iter()
            .filter_map(|tool_property_name| {
                let property_name = tool_property_name.as_str();
                let property_name_index = source_prefix.rfind(property_name)?;

                if !Self::is_keyword_boundary(source_prefix, property_name_index, property_name.len()) {
                    return None;
                }

                let after_property_name = &source_prefix[property_name_index + property_name.len()..];
                let leading_whitespace_length = after_property_name.len() - after_property_name.trim_start().len();
                let trimmed_after_property_name = &after_property_name[leading_whitespace_length..];

                if !trimmed_after_property_name.starts_with('{') {
                    return None;
                }

                let open_brace_index = property_name_index + property_name.len() + leading_whitespace_length;

                if !Self::block_is_still_open(&source_prefix[open_brace_index..]) {
                    return None;
                }

                Some((open_brace_index, tool_property_name))
            })
            .max_by_key(|(open_brace_index, _)| *open_brace_index)
            .map(|(_, tool_property_name)| tool_property_name)
    }

    pub(super) fn mcp_tool_schema_source_at_position(
        &self,
        position: Position,
        semantic_index: &SemanticIndex,
    ) -> Option<McpToolSchemaSource> {
        if let Some(tool_name) = semantic_index.tool_name_at_position(position) {
            return Some(McpToolSchemaSource::LocalTool(tool_name.to_string()));
        }

        let mcp_tool_batch_schema_source = self.mcp_tool_batch_schema_source_at_position(position);
        let cursor_offset = byte_offset_for_position(&self.text, position)?;
        let source_prefix = &self.text[..cursor_offset];
        let tool_keyword = DeclarationKeyword::Tool.as_str();
        let tool_keyword_index = source_prefix.rfind(tool_keyword)?;

        if !Self::is_keyword_boundary(source_prefix, tool_keyword_index, tool_keyword.len()) {
            return None;
        }

        let after_tool_keyword = source_prefix[tool_keyword_index + tool_keyword.len()..].trim_start();
        let tool_name = after_tool_keyword
            .chars()
            .take_while(|character| character.is_ascii_alphanumeric() || *character == '_' || *character == '-')
            .collect::<String>();

        if tool_name.is_empty() {
            return mcp_tool_batch_schema_source;
        }

        let before_tool_keyword = &source_prefix[..tool_keyword_index];
        let import_keyword = ImportKeyword::From.as_str();
        let import_keyword_index = before_tool_keyword.rfind(import_keyword)?;
        let import_header = &before_tool_keyword[import_keyword_index + import_keyword.len()..];
        let import_header = import_header
            .chars()
            .take_while(|character| *character != '{')
            .filter(|character| !character.is_whitespace())
            .collect::<String>();
        let mut import_segments = import_header.split('.');

        if import_segments.next()? != DeclarationKeyword::Mcp.as_str() {
            return None;
        }

        let server_name = import_segments.next()?.to_string();

        if import_segments.next()? != DeclarationKeyword::Tool.as_str() || import_segments.next().is_some() {
            return None;
        }

        Some(McpToolSchemaSource::McpTool {
            server_name: Some(server_name),
            tool_name,
        })
    }

    fn mcp_tool_batch_schema_source_at_position(&self, position: Position) -> Option<McpToolSchemaSource> {
        let cursor_offset = byte_offset_for_position(&self.text, position)?;
        let workflow = parse_workflow(&self.text).ok()?;

        workflow.declarations().iter().find_map(|declaration| {
            let superwire_core::dsl::Declaration::McpToolBatch(mcp_tool_batch) = declaration else {
                return None;
            };
            let span_range = mcp_tool_batch.span.to_byte_range(&self.text)?;

            if !span_range.contains(&cursor_offset) {
                return None;
            }

            Some(McpToolSchemaSource::McpToolBatch {
                server_name: mcp_tool_batch.server_name.clone(),
                tool_names: mcp_tool_batch.items.iter().map(|item| item.source_name.clone()).collect(),
            })
        })
    }

    fn mcp_tool_batch_item_completion_context(&self, position: Position, line_prefix: &str) -> Option<McpToolBatchItemCompletionContext> {
        let trimmed_line_prefix = line_prefix.trim_start();
        let tool_keyword = DeclarationKeyword::Tool.as_str();
        let tool_keyword_start_index = trimmed_line_prefix.rfind(tool_keyword)?;

        if !Self::is_keyword_boundary(trimmed_line_prefix, tool_keyword_start_index, tool_keyword.len()) {
            return None;
        }

        let after_tool_keyword = trimmed_line_prefix[tool_keyword_start_index + tool_keyword.len()..].trim_start();
        let after_tool_keyword = if after_tool_keyword.is_empty() { "" } else { after_tool_keyword };

        if after_tool_keyword.split_whitespace().nth(1).is_some() {
            return None;
        }

        let cursor_offset = byte_offset_for_position(&self.text, position)?;
        let source_prefix = &self.text[..cursor_offset];
        let header_start = source_prefix.rfind(ImportKeyword::From.as_str())?;
        let batch_prefix = &source_prefix[header_start..];
        let open_brace_index = batch_prefix.find('{')?;

        if !Self::block_is_still_open(&batch_prefix[open_brace_index..]) {
            return None;
        }

        let header = batch_prefix[..open_brace_index].trim();
        let import_path = header.strip_prefix(ImportKeyword::From.as_str())?.trim();
        let import_path = import_path
            .chars()
            .filter(|character| !character.is_whitespace())
            .collect::<String>();
        let mut import_segments = import_path.split('.');

        if import_segments.next()? != DeclarationKeyword::Mcp.as_str() {
            return None;
        }

        let server_name = import_segments.next()?.to_string();

        if import_segments.next()? != DeclarationKeyword::Tool.as_str() || import_segments.next().is_some() {
            return None;
        }

        Some(McpToolBatchItemCompletionContext {
            server_name,
            tool_prefix: after_tool_keyword.trim().to_string(),
        })
    }

    fn mcp_tool_batch_item_suggestions(
        &self,
        semantic_index: &SemanticIndex,
        position: Position,
        line_prefix: &str,
    ) -> Option<Vec<CompletionSuggestion>> {
        let batch_item_completion_context = self.mcp_tool_batch_item_completion_context(position, line_prefix)?;

        Some(semantic_index.mcp_tool_batch_item_suggestions(
            &batch_item_completion_context.server_name,
            &batch_item_completion_context.tool_prefix,
        ))
    }

    fn tool_call_binding_completion_context(&self, position: Position, line_prefix: &str) -> Option<ToolCallBindingCompletionContext> {
        if line_prefix.contains(':') {
            return None;
        }

        let cursor_offset = byte_offset_for_position(&self.text, position)?;
        let source_prefix = &self.text[..cursor_offset];
        let bindings_keyword = "bindings";
        let bindings_keyword_index = source_prefix.rfind(bindings_keyword)?;

        if !Self::is_keyword_boundary(source_prefix, bindings_keyword_index, bindings_keyword.len()) {
            return None;
        }

        let after_bindings_keyword = &source_prefix[bindings_keyword_index + bindings_keyword.len()..];
        let bindings_open_brace_relative_index = after_bindings_keyword.find('{')?;
        let bindings_open_brace_index = bindings_keyword_index + bindings_keyword.len() + bindings_open_brace_relative_index;
        let bindings_block_prefix = &source_prefix[bindings_open_brace_index..];

        if Self::block_balance(bindings_block_prefix) <= 0 {
            return None;
        }

        let before_bindings_keyword = &source_prefix[..bindings_keyword_index];
        let tool_namespace = format!("{}.", ReferenceKeyword::Tool.as_str());
        let tool_namespace_index = before_bindings_keyword.rfind(tool_namespace.as_str())?;
        let before_tool_namespace = before_bindings_keyword[..tool_namespace_index].trim_end();
        let call_keyword = ToolCallKeyword::Call.as_str();
        let inside_deterministic_tool_call = before_tool_namespace.ends_with(call_keyword);
        let inside_agent_tool_binding = before_tool_namespace.ends_with('[') || before_tool_namespace.ends_with(',');

        if !inside_deterministic_tool_call && !inside_agent_tool_binding {
            return None;
        }

        let tool_name_start_index = tool_namespace_index + tool_namespace.len();
        let tool_name = before_bindings_keyword[tool_name_start_index..]
            .chars()
            .take_while(|character| character.is_ascii_alphanumeric() || *character == '_')
            .collect::<String>();

        if tool_name.is_empty() {
            return None;
        }

        let binding_prefix = trailing_identifier(line_prefix).unwrap_or_default().to_string();
        let existing_binding_names = Self::existing_object_field_names(&source_prefix[bindings_open_brace_index + 1..]);

        Some(ToolCallBindingCompletionContext {
            tool_name,
            binding_prefix,
            existing_binding_names,
        })
    }

    pub(super) fn is_keyword_boundary(source_text: &str, keyword_index: usize, keyword_length: usize) -> bool {
        let before_keyword = source_text[..keyword_index].chars().next_back();
        let after_keyword = source_text[keyword_index + keyword_length..].chars().next();

        !before_keyword.is_some_and(|character| character.is_ascii_alphanumeric() || character == '_')
            && !after_keyword.is_some_and(|character| character.is_ascii_alphanumeric() || character == '_')
    }

    pub(super) fn block_balance(source_prefix: &str) -> i32 {
        source_prefix.chars().fold(0, |balance, character| match character {
            '{' => balance + 1,
            '}' => balance - 1,
            _ => balance,
        })
    }

    fn block_is_still_open(source_prefix: &str) -> bool {
        let mut balance = 0_i32;

        for character in source_prefix.chars() {
            match character {
                '{' => balance += 1,
                '}' => balance -= 1,
                _ => {}
            }

            if balance <= 0 {
                return false;
            }
        }

        balance > 0
    }

    fn existing_object_field_names(object_prefix: &str) -> Vec<String> {
        object_prefix
            .lines()
            .filter_map(|source_line| {
                let (field_name_segment, _) = source_line.split_once(':')?;
                let field_name = trailing_identifier(field_name_segment.trim_end())?;

                Some(field_name.to_string())
            })
            .collect()
    }

    fn existing_typed_field_names(&self, position: Position) -> Vec<String> {
        let source_text = &self.text;
        let Some(cursor_offset) = byte_offset_for_position(source_text, position) else {
            return Vec::new();
        };

        let source_before_cursor = &source_text[..cursor_offset];
        let last_open_brace = source_before_cursor.rfind('{').unwrap_or(0);
        let block_content = &source_before_cursor[last_open_brace + 1..];

        Self::existing_object_field_names(block_content)
    }

    fn dynamic_value_non_reference_suggestions(
        semantic_index: &SemanticIndex,
        line_prefix: &str,
        completion_scope: CompletionScope,
    ) -> Option<Vec<CompletionSuggestion>> {
        if completion_scope != CompletionScope::DynamicValues {
            return None;
        }

        let dynamic_value_completion_context = OutputValueCompletionContext::from_line_prefix(line_prefix)?;

        if ReferenceCompletionPath::from_line_prefix(line_prefix).is_some() {
            return None;
        }

        Some(semantic_index.dynamic_value_suggestions(&dynamic_value_completion_context.value_prefix))
    }

    fn inference_value_non_reference_suggestions(semantic_index: &SemanticIndex, line_prefix: &str) -> Option<Vec<CompletionSuggestion>> {
        let inference_value_completion_context = InferenceSettingValueCompletionContext::from_line_prefix(line_prefix)?;

        if inference_value_completion_context.inside_string_literal {
            return Some(Vec::new());
        }

        if ReferenceCompletionPath::from_line_prefix(line_prefix).is_some() {
            return None;
        }

        if inference_value_completion_context.value_prefix.is_empty() {
            return Some(semantic_index.inference_value_root_suggestions(""));
        }

        Some(Vec::new())
    }

    fn property_scope_suggestions(
        &self,
        semantic_index: &SemanticIndex,
        completion_scope: CompletionScope,
        line_prefix: &str,
        position: Position,
    ) -> Option<Vec<CompletionSuggestion>> {
        match completion_scope {
            CompletionScope::InferenceSettings => Some(inference_setting_scope_suggestions(line_prefix)),
            CompletionScope::AgentProperties => Some(agent_property_scope_suggestions(line_prefix)),
            CompletionScope::ToolProperties => Some(self.tool_property_suggestions(semantic_index, line_prefix, position)),
            CompletionScope::McpToolBatchImport => Some(mcp_tool_batch_import_scope_suggestions(line_prefix)),
            CompletionScope::General | CompletionScope::TypedDeclarations | CompletionScope::DynamicValues => None,
        }
    }

    fn tool_property_suggestions(
        &self,
        semantic_index: &SemanticIndex,
        line_prefix: &str,
        position: Position,
    ) -> Vec<CompletionSuggestion> {
        let Some(mcp_tool_schema_source) = self.mcp_tool_schema_source_at_position(position, semantic_index) else {
            return tool_property_scope_suggestions(line_prefix);
        };
        let property_prefix = trailing_identifier(line_prefix).unwrap_or_default();

        ToolPropertyName::all()
            .into_iter()
            .filter(|property_name| property_name.as_str().starts_with(property_prefix))
            .map(|property_name| {
                let schema_fields = match &mcp_tool_schema_source {
                    McpToolSchemaSource::LocalTool(tool_name) => semantic_index.mcp_tool_schema_fields(tool_name, property_name),
                    McpToolSchemaSource::McpTool { server_name, tool_name } => {
                        semantic_index.mcp_tool_schema_fields_for_source(server_name.as_deref(), tool_name, property_name)
                    }
                    McpToolSchemaSource::McpToolBatch { server_name, tool_names } => {
                        semantic_index.mcp_tool_batch_common_schema_fields(server_name, tool_names, property_name)
                    }
                };

                if schema_fields.is_empty() {
                    return tool_property_scope_suggestions(property_name.as_str())
                        .into_iter()
                        .find(|completion_suggestion| completion_suggestion.label == property_name.as_str())
                        .unwrap_or_else(|| CompletionSuggestion {
                            label: property_name.as_str().to_string(),
                            kind: CompletionKind::Property,
                            detail: "Tool declaration property".to_string(),
                            documentation: "Property available inside a `tool` declaration.".to_string(),
                            insert_text: property_name.as_str().to_string(),
                        });
                }

                CompletionSuggestion {
                    label: property_name.as_str().to_string(),
                    kind: CompletionKind::Property,
                    detail: "MCP schema property".to_string(),
                    documentation: format!("Insert `{}` with fields discovered from the MCP lock file.", property_name.as_str()),
                    insert_text: Self::render_schema_property_snippet(property_name, &schema_fields),
                }
            })
            .collect()
    }

    fn render_schema_property_snippet(property_name: ToolPropertyName, schema_fields: &[superwire_core::dsl::TypedField]) -> String {
        let rendered_fields = schema_fields
            .iter()
            .map(|typed_field| format!("    {}: {}", typed_field.name, typed_field.field_type.render_type()))
            .collect::<Vec<_>>()
            .join("\n");

        format!("{} {{\n{rendered_fields}\n}}", property_name.as_str())
    }

    fn should_defer_to_reference_completion(line_prefix: &str) -> bool {
        let Some(reference_completion_path) = ReferenceCompletionPath::from_line_prefix(line_prefix) else {
            return false;
        };

        reference_completion_path.root_keyword().is_some()
            || reference_completion_path.is_schema_root()
            || !reference_completion_path.complete_accesses.is_empty()
    }

    fn has_existing_tool_binding_block(line_suffix: &str) -> bool {
        matches!(line_suffix.trim_start().chars().next(), Some('{' | '('))
    }

    fn provider_non_reference_suggestions(
        &self,
        semantic_index: &SemanticIndex,
        line_prefix: &str,
        position: Position,
    ) -> Option<Vec<CompletionSuggestion>> {
        if let Some(provider_driver_suggestions) = semantic_index.provider_driver_value_suggestions(position, line_prefix) {
            return Some(provider_driver_suggestions);
        }

        if let Some(provider_models_suggestions) = semantic_index.provider_models_value_suggestions(line_prefix) {
            return Some(provider_models_suggestions);
        }

        semantic_index.provider_property_suggestions(position, line_prefix)
    }

    fn agent_property_value_suggestions(
        &self,
        semantic_index: &SemanticIndex,
        line_prefix: &str,
        inside_interpolation_expression: bool,
    ) -> Option<Vec<CompletionSuggestion>> {
        let agent_property_value_completion_context = AgentPropertyValueCompletionContext::from_line_prefix(line_prefix)?;

        match agent_property_value_completion_context.property_name {
            AgentExpressionPropertyName::Context => {
                Some(semantic_index.context_function_suggestions(&agent_property_value_completion_context.value_prefix))
            }
            AgentExpressionPropertyName::Model => {
                Some(semantic_index.provider_call_suggestions(&agent_property_value_completion_context.value_prefix))
            }
            AgentExpressionPropertyName::Prompt => {
                if inside_interpolation_expression || ReferenceCompletionPath::from_line_prefix(line_prefix).is_some() {
                    return None;
                }

                if agent_property_value_completion_context.inside_string_literal {
                    return Some(Vec::new());
                }

                Some(semantic_index.prompt_value_suggestions(&agent_property_value_completion_context.value_prefix, line_prefix))
            }
            AgentExpressionPropertyName::Inference => {
                if agent_property_value_completion_context.inside_string_literal {
                    return Some(Vec::new());
                }

                if ReferenceCompletionPath::from_line_prefix(line_prefix).is_some() {
                    return Some(Vec::new());
                }

                Some(semantic_index.inference_object_suggestions(&agent_property_value_completion_context.value_prefix))
            }
            AgentExpressionPropertyName::Tools => None,
        }
    }

    fn response_format_value_suggestions(line_prefix: &str) -> Option<Vec<CompletionSuggestion>> {
        let trimmed_line_prefix = line_prefix.trim_start();
        let (line_before_value, value_prefix) = trimmed_line_prefix.rsplit_once(':')?;
        let property_name_identifier = trailing_identifier(line_before_value)?;

        if AgentPropertyName::from_identifier(property_name_identifier) != Some(AgentPropertyName::ResponseFormat) {
            return None;
        }

        let value_completion_context = ValueCompletionContext::from_value_prefix(value_prefix);

        if value_completion_context.inside_string_literal {
            return Some(Vec::new());
        }

        let response_format_suggestions = AgentResponseFormat::all()
            .into_iter()
            .filter(|response_format| response_format.as_str().starts_with(&value_completion_context.value_prefix))
            .map(|response_format| CompletionSuggestion {
                label: response_format.as_str().to_string(),
                kind: super::CompletionKind::Value,
                detail: "Agent response format".to_string(),
                documentation: "Configures provider response mode for this agent.".to_string(),
                insert_text: response_format.as_str().to_string(),
            })
            .collect::<Vec<_>>();

        Some(response_format_suggestions)
    }

    fn property_value_non_reference_suggestions(
        semantic_index: &SemanticIndex,
        line_prefix: &str,
        completion_scope: CompletionScope,
    ) -> Option<Vec<CompletionSuggestion>> {
        if let Some(response_format_suggestions) = Self::response_format_value_suggestions(line_prefix) {
            return Some(response_format_suggestions);
        }

        if let Some(dynamic_value_suggestions) =
            Self::dynamic_value_non_reference_suggestions(semantic_index, line_prefix, completion_scope)
        {
            return Some(dynamic_value_suggestions);
        }

        Self::inference_value_non_reference_suggestions(semantic_index, line_prefix)
    }

    fn reference_completion_suggestions(
        &self,
        semantic_index: &SemanticIndex,
        inputs: ReferenceCompletionInputs<'_>,
    ) -> Option<Vec<CompletionSuggestion>> {
        let line_has_property_separator = inputs.line_prefix.trim_start().contains(':');
        let reference_completion_path = ReferenceCompletionPath::from_line_prefix(inputs.line_prefix)?;
        let for_loop_iterable_reference_context = matches!(
            reference_completion_path.root_keyword(),
            Some(ReferenceKeyword::Input | ReferenceKeyword::Agent | ReferenceKeyword::Dynamic | ReferenceKeyword::Secrets)
        ) && Self::is_for_loop_iterable_clause_context(inputs.line_prefix);

        let reference_completion_constraint = if for_loop_iterable_reference_context {
            ReferenceCompletionConstraint::ForLoopIterable
        } else {
            self.reference_completion_constraint(inputs.line_prefix, inputs.inference_setting_value_completion_context)
        };

        let reference_suggestions = semantic_index.reference_path_suggestions(
            &reference_completion_path,
            reference_completion_constraint,
            inputs.position,
            Self::has_existing_tool_binding_block(inputs.line_suffix),
        );

        if Self::is_mcp_call_callee_context(inputs.line_prefix, &reference_completion_path) {
            return Some(reference_suggestions);
        }

        if let Some(inference_suggestions) = self.inference_reference_completion_suggestions(
            semantic_index,
            inputs.line_prefix,
            &reference_completion_path,
            &reference_suggestions,
            inputs.inference_setting_value_completion_context,
        ) {
            return Some(inference_suggestions);
        }

        if let Some(interpolation_suggestions) = self.interpolation_reference_completion_suggestions(
            semantic_index,
            inputs.line_prefix,
            inputs.position,
            &reference_completion_path,
            &reference_suggestions,
            inputs.inside_interpolation_expression,
        ) {
            return Some(interpolation_suggestions);
        }

        if let Some(output_suggestions) = self.output_reference_completion_suggestions(
            semantic_index,
            inputs.line_prefix,
            inputs.position,
            &reference_completion_path,
            &reference_suggestions,
            line_has_property_separator,
        ) {
            return Some(output_suggestions);
        }

        if let Some(prompt_suggestions) = self.prompt_reference_completion_suggestions(
            semantic_index,
            inputs.line_prefix,
            &reference_completion_path,
            &reference_suggestions,
            line_has_property_separator,
            inputs.inside_interpolation_expression,
        ) {
            return Some(prompt_suggestions);
        }

        if let Some(dynamic_value_suggestions) = Self::dynamic_value_reference_completion_suggestions(
            semantic_index,
            inputs.completion_scope,
            inputs.line_prefix,
            &reference_completion_path,
            &reference_suggestions,
        ) {
            return Some(dynamic_value_suggestions);
        }

        self.default_reference_completion_suggestions(&reference_completion_path, &reference_suggestions, reference_completion_constraint)
    }

    fn inference_reference_completion_suggestions(
        &self,
        semantic_index: &SemanticIndex,
        line_prefix: &str,
        reference_completion_path: &ReferenceCompletionPath,
        reference_suggestions: &[CompletionSuggestion],
        inference_setting_value_completion_context: Option<&InferenceSettingValueCompletionContext>,
    ) -> Option<Vec<CompletionSuggestion>> {
        inference_setting_value_completion_context?;

        let can_suggest_inference_roots = Self::can_suggest_reference_roots(line_prefix, reference_completion_path);

        match reference_completion_path.root_keyword() {
            Some(ReferenceKeyword::Input | ReferenceKeyword::Agent | ReferenceKeyword::Dynamic) => {
                if can_suggest_inference_roots {
                    return Some(semantic_index.inference_value_root_suggestions(reference_completion_path.root_identifier()));
                }

                Some(reference_suggestions.to_vec())
            }
            Some(ReferenceKeyword::Secrets | ReferenceKeyword::Tool | ReferenceKeyword::Resource | ReferenceKeyword::Prompt) | None => {
                if can_suggest_inference_roots {
                    return Some(semantic_index.inference_value_root_suggestions(reference_completion_path.root_identifier()));
                }

                Some(Vec::new())
            }
        }
    }

    fn interpolation_reference_completion_suggestions(
        &self,
        semantic_index: &SemanticIndex,
        line_prefix: &str,
        position: Position,
        reference_completion_path: &ReferenceCompletionPath,
        reference_suggestions: &[CompletionSuggestion],
        inside_interpolation_expression: bool,
    ) -> Option<Vec<CompletionSuggestion>> {
        if !inside_interpolation_expression {
            return None;
        }

        let can_suggest_interpolation_roots = Self::can_suggest_reference_roots(line_prefix, reference_completion_path);
        let for_loop_iterator_reference_root =
            semantic_index.has_for_loop_binding_at_position(position, reference_completion_path.root_identifier());

        match reference_completion_path.root_keyword() {
            Some(ReferenceKeyword::Input | ReferenceKeyword::Agent | ReferenceKeyword::Dynamic) => {
                if can_suggest_interpolation_roots {
                    return Some(semantic_index.interpolation_root_suggestions(reference_completion_path.root_identifier(), position));
                }

                Some(reference_suggestions.to_vec())
            }
            Some(ReferenceKeyword::Secrets | ReferenceKeyword::Tool | ReferenceKeyword::Resource | ReferenceKeyword::Prompt) | None => {
                if for_loop_iterator_reference_root {
                    return Some(reference_suggestions.to_vec());
                }

                if can_suggest_interpolation_roots {
                    return Some(semantic_index.interpolation_root_suggestions(reference_completion_path.root_identifier(), position));
                }

                Some(Vec::new())
            }
        }
    }

    fn output_reference_completion_suggestions(
        &self,
        semantic_index: &SemanticIndex,
        line_prefix: &str,
        position: Position,
        reference_completion_path: &ReferenceCompletionPath,
        reference_suggestions: &[CompletionSuggestion],
        line_has_property_separator: bool,
    ) -> Option<Vec<CompletionSuggestion>> {
        if !line_has_property_separator || !semantic_index.is_output_position(position) {
            return None;
        }

        let can_suggest_output_roots = Self::can_suggest_reference_roots(line_prefix, reference_completion_path);

        match reference_completion_path.root_keyword() {
            Some(ReferenceKeyword::Input | ReferenceKeyword::Agent | ReferenceKeyword::Dynamic | ReferenceKeyword::Secrets) => {
                if can_suggest_output_roots {
                    return Some(semantic_index.output_value_root_suggestions(reference_completion_path.root_identifier()));
                }

                Some(reference_suggestions.to_vec())
            }
            Some(ReferenceKeyword::Tool | ReferenceKeyword::Resource | ReferenceKeyword::Prompt) | None => {
                if can_suggest_output_roots {
                    return Some(semantic_index.output_value_root_suggestions(reference_completion_path.root_identifier()));
                }

                Some(Vec::new())
            }
        }
    }

    fn prompt_reference_completion_suggestions(
        &self,
        semantic_index: &SemanticIndex,
        line_prefix: &str,
        reference_completion_path: &ReferenceCompletionPath,
        reference_suggestions: &[CompletionSuggestion],
        line_has_property_separator: bool,
        inside_interpolation_expression: bool,
    ) -> Option<Vec<CompletionSuggestion>> {
        let is_prompt_property_reference = line_has_property_separator
            && !inside_interpolation_expression
            && AgentPropertyValueCompletionContext::from_line_prefix(line_prefix)
                .is_some_and(|completion_context| completion_context.property_name == AgentExpressionPropertyName::Prompt);

        if !is_prompt_property_reference {
            return None;
        }

        let can_suggest_prompt_roots = Self::can_suggest_reference_roots(line_prefix, reference_completion_path);

        match reference_completion_path.root_keyword() {
            Some(ReferenceKeyword::Input | ReferenceKeyword::Agent | ReferenceKeyword::Dynamic) => {
                if can_suggest_prompt_roots {
                    return Some(semantic_index.prompt_value_root_suggestions(reference_completion_path.root_identifier()));
                }

                Some(reference_suggestions.to_vec())
            }
            Some(ReferenceKeyword::Secrets | ReferenceKeyword::Tool | ReferenceKeyword::Resource | ReferenceKeyword::Prompt) | None => {
                if can_suggest_prompt_roots {
                    return Some(semantic_index.prompt_value_root_suggestions(reference_completion_path.root_identifier()));
                }

                Some(Vec::new())
            }
        }
    }

    fn dynamic_value_reference_completion_suggestions(
        semantic_index: &SemanticIndex,
        completion_scope: CompletionScope,
        line_prefix: &str,
        reference_completion_path: &ReferenceCompletionPath,
        reference_suggestions: &[CompletionSuggestion],
    ) -> Option<Vec<CompletionSuggestion>> {
        if completion_scope != CompletionScope::DynamicValues || !line_prefix.trim_start().contains(':') {
            return None;
        }

        if Self::can_suggest_reference_roots(line_prefix, reference_completion_path) {
            return Some(semantic_index.dynamic_value_suggestions(reference_completion_path.root_identifier()));
        }

        match reference_completion_path.root_keyword() {
            Some(ReferenceKeyword::Agent | ReferenceKeyword::Dynamic | ReferenceKeyword::Input | ReferenceKeyword::Secrets) => {
                Some(reference_suggestions.to_vec())
            }
            Some(ReferenceKeyword::Tool) => {
                if Self::is_tool_call_callee_context(line_prefix, reference_completion_path) {
                    return Some(reference_suggestions.to_vec());
                }

                Some(Vec::new())
            }
            Some(ReferenceKeyword::Resource | ReferenceKeyword::Prompt) => {
                if Self::is_mcp_call_callee_context(line_prefix, reference_completion_path) {
                    return Some(reference_suggestions.to_vec());
                }

                Some(Vec::new())
            }
            None => Some(Vec::new()),
        }
    }

    fn is_tool_call_callee_context(line_prefix: &str, reference_completion_path: &ReferenceCompletionPath) -> bool {
        if reference_completion_path.root_keyword() != Some(ReferenceKeyword::Tool) {
            return false;
        }

        let Some(reference_token) = trailing_reference_token(line_prefix) else {
            return false;
        };

        let Some(reference_start_index) = line_prefix.rfind(reference_token) else {
            return false;
        };

        let value_prefix = line_prefix[..reference_start_index].trim_end();
        let Some((_, value_prefix_after_separator)) = value_prefix.rsplit_once(':') else {
            return false;
        };

        value_prefix_after_separator.trim() == ToolCallKeyword::Call.as_str()
    }

    fn is_mcp_call_callee_context(line_prefix: &str, reference_completion_path: &ReferenceCompletionPath) -> bool {
        let expected_keyword = match reference_completion_path.root_keyword() {
            Some(ReferenceKeyword::Resource) => "read",
            Some(ReferenceKeyword::Prompt) => "render",
            _ => return false,
        };
        let Some(reference_token) = trailing_reference_token(line_prefix) else {
            return false;
        };
        let Some(reference_start_index) = line_prefix.rfind(reference_token) else {
            return false;
        };
        let value_prefix = line_prefix[..reference_start_index].trim_end();

        if let Some((_, value_prefix_after_separator)) = value_prefix.rsplit_once(':') {
            return value_prefix_after_separator.trim().ends_with(expected_keyword);
        }

        value_prefix.ends_with(expected_keyword)
    }

    fn default_reference_completion_suggestions(
        &self,
        reference_completion_path: &ReferenceCompletionPath,
        reference_suggestions: &[CompletionSuggestion],
        reference_completion_constraint: ReferenceCompletionConstraint,
    ) -> Option<Vec<CompletionSuggestion>> {
        let reference_root_keyword = reference_completion_path.root_keyword();
        let schema_reference_root = reference_completion_path.is_schema_root();

        if reference_completion_constraint == ReferenceCompletionConstraint::ForLoopIterable {
            return Some(reference_suggestions.to_vec());
        }

        if matches!(
            reference_root_keyword,
            Some(ReferenceKeyword::Tool | ReferenceKeyword::Resource | ReferenceKeyword::Prompt)
        ) {
            return Some(reference_suggestions.to_vec());
        }

        if schema_reference_root || reference_root_keyword.is_some() {
            return Some(reference_suggestions.to_vec());
        }

        if !reference_suggestions.is_empty() {
            return Some(reference_suggestions.to_vec());
        }

        None
    }

    fn can_suggest_reference_roots(line_prefix: &str, reference_completion_path: &ReferenceCompletionPath) -> bool {
        let reference_token_has_trailing_separator =
            trailing_reference_token(line_prefix).is_some_and(|reference_token| reference_token.ends_with('.'));

        !reference_token_has_trailing_separator && reference_completion_path.complete_accesses.is_empty()
    }

    fn should_suppress_prompt_string_literal_suggestions(line_prefix: &str) -> bool {
        if let Some(agent_property_value_completion_context) = AgentPropertyValueCompletionContext::from_line_prefix(line_prefix) {
            return agent_property_value_completion_context.property_name == AgentExpressionPropertyName::Prompt
                && agent_property_value_completion_context.inside_string_literal;
        }

        let trimmed_line_prefix = line_prefix.trim_start();

        let Some((line_before_value, value_prefix)) = trimmed_line_prefix.rsplit_once(':') else {
            return false;
        };

        line_before_value.trim_end().ends_with(AgentExpressionPropertyName::Prompt.as_str())
            && super::completion_context::ValueCompletionContext::from_value_prefix(value_prefix).inside_string_literal
    }

    fn inference_setting_value_completion_context(
        &self,
        line_has_property_separator: bool,
        line_prefix: &str,
    ) -> Option<InferenceSettingValueCompletionContext> {
        if !line_has_property_separator {
            return None;
        }

        InferenceSettingValueCompletionContext::from_line_prefix(line_prefix)
    }

    fn reference_completion_constraint(
        &self,
        line_prefix: &str,
        inference_setting_value_completion_context: Option<&InferenceSettingValueCompletionContext>,
    ) -> ReferenceCompletionConstraint {
        if Self::is_for_loop_iterable_clause_context(line_prefix) {
            return ReferenceCompletionConstraint::ForLoopIterable;
        }

        let line_reference_constraint = ReferenceCompletionConstraint::from_line_prefix(line_prefix);

        if line_reference_constraint == ReferenceCompletionConstraint::ForLoopIterable {
            return line_reference_constraint;
        }

        let Some(inference_value_completion_context) = inference_setting_value_completion_context else {
            return line_reference_constraint;
        };

        match inference_value_completion_context.inference_setting {
            InferenceSetting::MaxTokens
            | InferenceSetting::TopK
            | InferenceSetting::Seed
            | InferenceSetting::StuckThreshold
            | InferenceSetting::ProviderMaxRetries
            | InferenceSetting::ProviderRetryBaseDelayMs => ReferenceCompletionConstraint::InferenceIntegerValue,
            InferenceSetting::Temperature
            | InferenceSetting::TopP
            | InferenceSetting::FrequencyPenalty
            | InferenceSetting::PresencePenalty
            | InferenceSetting::RepeatPenalty => ReferenceCompletionConstraint::InferenceNumericValue,
        }
    }

    fn is_for_loop_iterable_clause_context(line_prefix: &str) -> bool {
        if ForLoopIterableValueCompletionContext::from_line_prefix(line_prefix).is_some() {
            return true;
        }

        let trimmed_line_prefix = line_prefix.trim_start();
        let agent_keyword_with_space = format!("{} ", DeclarationKeyword::Agent.as_str());
        let Some(after_agent_keyword) = trimmed_line_prefix.strip_prefix(agent_keyword_with_space.as_str()) else {
            return false;
        };

        let for_keyword_with_spaces = format!(" {} ", ForClauseKeyword::For.as_str());
        let in_keyword_with_spaces = format!(" {} ", ForClauseKeyword::In.as_str());

        after_agent_keyword.contains(for_keyword_with_spaces.as_str()) && after_agent_keyword.contains(in_keyword_with_spaces.as_str())
    }

    pub(super) fn semantic_index_for_completion(&self, position: Position) -> SemanticIndex {
        if self.semantic_snapshot.parse_error.is_none() {
            return self.semantic_snapshot.semantic_index.clone();
        }

        self.recovered_semantic_index(position)
            .unwrap_or_else(|| self.semantic_snapshot.semantic_index.clone())
    }

    fn recovered_semantic_index(&self, position: Position) -> Option<SemanticIndex> {
        let cursor_offset = byte_offset_for_position(&self.text, position)?;

        let mut recovered_source = String::with_capacity(self.text.len() + COMPLETION_RECOVERY_PLACEHOLDER.len());
        recovered_source.push_str(&self.text[..cursor_offset]);
        recovered_source.push_str(COMPLETION_RECOVERY_PLACEHOLDER);
        recovered_source.push_str(&self.text[cursor_offset..]);

        let workflow = parse_workflow(&recovered_source).ok()?;

        Some(SemanticIndex::from_workflow_with_mcp_lock(
            &workflow,
            self.semantic_snapshot.semantic_index.mcp_lock.clone(),
        ))
    }

    fn completion_scope(&self, position: Position) -> CompletionScope {
        let Some(cursor_offset) = byte_offset_for_position(&self.text, position) else {
            return CompletionScope::General;
        };

        completion_scope_at_offset(&self.text, cursor_offset)
    }

    fn is_inside_multiline_string_literal(&self, position: Position) -> bool {
        let Some(cursor_offset) = byte_offset_for_position(&self.text, position) else {
            return false;
        };

        is_inside_multiline_string_literal(&self.text, cursor_offset)
    }
}
