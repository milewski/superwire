use superwire_core::dsl::{
    parse_workflow, AgentExpressionPropertyName, DeclarationKeyword, ImportKeyword, ReferenceKeyword, ToolPropertyName,
};

use lsp_types::{Position, Range};

use super::super::completion_context::{
    AgentPropertyValueCompletionContext, DeclarationHeaderCompletionContext, ForLoopDestructuringBindingCompletionContext,
    ForLoopIterableValueCompletionContext, InferenceSettingValueCompletionContext, ModelCallCompletionContext,
    OutputValueCompletionContext, ToolCallCompletionContext, ValueCompletionContext,
};
use super::super::position::byte_offset_for_position;
use super::super::reference::ReferenceCompletionPath;
use super::super::scope::{
    agent_property_scope_suggestions, completion_scope_at_offset, inference_setting_scope_suggestions, mcp_prompt_import_scope_suggestions,
    mcp_server_property_scope_suggestions, mcp_tool_batch_import_scope_suggestions, model_property_scope_suggestions,
    model_usage_property_scope_suggestions, CompletionScope,
};
use super::super::semantic_index::SemanticIndex;
use super::super::text_utils::{
    is_inside_interpolation_expression, is_inside_multiline_string_literal, trailing_identifier, trailing_reference_token,
};
use super::super::{CompletionSuggestion, DocumentState};
use super::ReferenceCompletionInputs;

const COMPLETION_RECOVERY_PLACEHOLDER: &str = "__completion_placeholder";

impl DocumentState {
    #[must_use]
    pub fn completion_suggestions(&self, position: Position) -> Vec<CompletionSuggestion> {
        self.completion_suggestions_inner(position)
            .into_iter()
            .filter(|completion_suggestion| !Self::completion_suggestion_contains_recovery_placeholder(completion_suggestion))
            .collect()
    }

    fn completion_suggestions_inner(&self, position: Position) -> Vec<CompletionSuggestion> {
        let Some(line_prefix) = self.line_prefix(position) else {
            return Vec::new();
        };
        let line_suffix = self.line_suffix(position).unwrap_or_default();

        let inside_interpolation_expression = is_inside_interpolation_expression(&line_prefix);

        if self.is_inside_multiline_string_literal(position) && !inside_interpolation_expression {
            return Vec::new();
        }

        let semantic_index = self.semantic_index_for_completion(position);
        let completion_scope = self.completion_scope(position, &line_prefix, &semantic_index);

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

            if let Some(prompt_import_binding_completion_context) = self.prompt_import_binding_completion_context(position, &line_prefix) {
                return semantic_index.mcp_prompt_binding_suggestions(
                    &prompt_import_binding_completion_context.server_name,
                    &prompt_import_binding_completion_context.prompt_name,
                    &prompt_import_binding_completion_context.binding_prefix,
                    &prompt_import_binding_completion_context.existing_binding_names,
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
            return Self::interpolation_root_suggestions_for_context(
                &semantic_index,
                &line_prefix,
                position,
                "",
                inside_interpolation_expression,
            );
        }

        let inside_bindings_value = self.tool_schema_property_name_at_position(position) == Some(ToolPropertyName::Bindings);

        if !inside_bindings_value && semantic_index.is_type_position(position, &line_prefix) {
            let current_schema_name = semantic_index.schema_name_at_position(position);
            let type_suggestions = semantic_index.type_suggestions(&line_prefix, current_schema_name);

            if !type_suggestions.is_empty() {
                return type_suggestions;
            }
        }

        if semantic_index.is_inside_agent_output_declaration(position) {
            return Vec::new();
        }

        if inside_bindings_value {
            let value_prefix = line_prefix
                .split_once(':')
                .map_or(line_prefix.as_str(), |(_, value_prefix)| value_prefix)
                .trim_start();

            return semantic_index.output_value_suggestions(value_prefix);
        }

        semantic_index.default_suggestions(should_include_builtin_function_suggestions)
    }

    fn completion_suggestion_contains_recovery_placeholder(completion_suggestion: &CompletionSuggestion) -> bool {
        completion_suggestion.label.contains(COMPLETION_RECOVERY_PLACEHOLDER)
            || completion_suggestion.insert_text.contains(COMPLETION_RECOVERY_PLACEHOLDER)
            || completion_suggestion.detail.contains(COMPLETION_RECOVERY_PLACEHOLDER)
            || completion_suggestion.documentation.contains(COMPLETION_RECOVERY_PLACEHOLDER)
    }

    #[must_use]
    pub fn completion_text_edit_range(&self, position: Position) -> Option<Range> {
        let line_prefix = self.line_prefix(position)?;
        let line_suffix = self.line_suffix(position).unwrap_or_default();

        if let Some(model_call_completion_context) = ModelCallCompletionContext::from_line_prefix(&line_prefix) {
            if model_call_completion_context.replaces_empty_string_literal && line_suffix.starts_with('"') {
                return Some(Range {
                    start: Position {
                        line: position.line,
                        character: position.character.saturating_sub(1),
                    },
                    end: Position {
                        line: position.line,
                        character: position.character + 1,
                    },
                });
            }

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

    #[allow(clippy::too_many_lines)]
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

        if let Some(property_suggestions) = self.scoped_property_suggestions_after_completed_value(
            semantic_index,
            line_prefix,
            position,
            completion_scope,
            line_has_property_separator,
            inside_interpolation_expression,
        ) {
            return Some(property_suggestions);
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
                if let DeclarationHeaderCompletionContext::ProviderDriver { driver_prefix } = declaration_header_completion_context {
                    return Some(SemanticIndex::provider_driver_suggestions(
                        &driver_prefix,
                        "Provider driver",
                        "Built-in provider driver available for provider declarations.",
                    ));
                }

                if let DeclarationHeaderCompletionContext::ModelProvider { provider_prefix } = declaration_header_completion_context {
                    return Some(semantic_index.provider_reference_suggestions(&provider_prefix));
                }

                return Some(declaration_header_completion_context.completion_suggestions());
            }

            if let Some(model_property_suggestions) = Self::model_property_suggestions_at_position(
                semantic_index,
                line_prefix,
                position,
                completion_scope,
                line_has_property_separator,
                inside_interpolation_expression,
            ) {
                return Some(model_property_suggestions);
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

    fn scoped_property_suggestions_after_completed_value(
        &self,
        semantic_index: &SemanticIndex,
        line_prefix: &str,
        position: Position,
        completion_scope: CompletionScope,
        line_has_property_separator: bool,
        inside_interpolation_expression: bool,
    ) -> Option<Vec<CompletionSuggestion>> {
        if !line_has_property_separator
            || inside_interpolation_expression
            || !Self::can_continue_property_scope_after_value(completion_scope, line_prefix)
            || Self::line_prefix_has_open_property_string_value(line_prefix)
        {
            return None;
        }

        self.property_scope_suggestions(semantic_index, completion_scope, line_prefix, position)
    }

    fn can_continue_property_scope_after_value(completion_scope: CompletionScope, line_prefix: &str) -> bool {
        if !matches!(
            completion_scope,
            CompletionScope::ProviderProperties
                | CompletionScope::ModelProperties
                | CompletionScope::ModelUsageProperties
                | CompletionScope::McpServerProperties
                | CompletionScope::AgentProperties
                | CompletionScope::ToolProperties
                | CompletionScope::McpToolBatchImport
                | CompletionScope::McpPromptImport
                | CompletionScope::InferenceSettings
        ) {
            return false;
        }

        Self::line_prefix_ends_after_property_value(line_prefix)
    }

    pub(super) fn line_prefix_ends_after_property_value(line_prefix: &str) -> bool {
        let trimmed_line_prefix = line_prefix.trim_end();

        matches!(trimmed_line_prefix.chars().next_back(), Some('"' | '}' | ']' | ')' | '0'..='9'))
    }

    pub(super) fn line_prefix_has_open_property_string_value(line_prefix: &str) -> bool {
        let Some((_, property_value_prefix)) = line_prefix.trim_start().rsplit_once(':') else {
            return false;
        };

        ValueCompletionContext::from_value_prefix(property_value_prefix).inside_string_literal
    }

    pub(in crate::document) fn is_keyword_boundary(source_text: &str, keyword_index: usize, keyword_length: usize) -> bool {
        let before_keyword = source_text[..keyword_index].chars().next_back();
        let after_keyword = source_text[keyword_index + keyword_length..].chars().next();

        !before_keyword.is_some_and(|character| character.is_ascii_alphanumeric() || character == '_')
            && !after_keyword.is_some_and(|character| character.is_ascii_alphanumeric() || character == '_')
    }

    pub(in crate::document) fn block_balance(source_prefix: &str) -> i32 {
        source_prefix.chars().fold(0, |balance, character| match character {
            '{' => balance + 1,
            '}' => balance - 1,
            _ => balance,
        })
    }

    pub(super) fn block_is_still_open(source_prefix: &str) -> bool {
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

    pub(super) fn existing_object_field_names(object_prefix: &str) -> Vec<String> {
        object_prefix
            .lines()
            .filter_map(|source_line| {
                let (field_name_segment, _) = source_line.split_once(':')?;
                let field_name = trailing_identifier(field_name_segment.trim_end())?;

                Some(field_name.to_string())
            })
            .collect()
    }

    pub(super) fn existing_typed_field_names(&self, position: Position) -> Vec<String> {
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
            CompletionScope::ProviderProperties => Some(
                semantic_index
                    .provider_property_suggestions(position, line_prefix)
                    .unwrap_or_default(),
            ),
            CompletionScope::ModelProperties => Some(model_property_scope_suggestions(line_prefix)),
            CompletionScope::ModelUsageProperties => Some(model_usage_property_scope_suggestions(line_prefix)),
            CompletionScope::McpServerProperties => Some(mcp_server_property_scope_suggestions(line_prefix)),
            CompletionScope::InferenceSettings => Some(inference_setting_scope_suggestions(line_prefix)),
            CompletionScope::AgentProperties => Some(agent_property_scope_suggestions(line_prefix)),
            CompletionScope::ToolProperties => Some(self.tool_property_suggestions(semantic_index, line_prefix, position)),
            CompletionScope::McpToolBatchImport => {
                let allowed_keywords = self.mcp_batch_import_allowed_keywords_at_position(position)?;

                Some(mcp_tool_batch_import_scope_suggestions(line_prefix, &allowed_keywords))
            }
            CompletionScope::McpPromptImport => Some(mcp_prompt_import_scope_suggestions(line_prefix)),
            CompletionScope::General | CompletionScope::TypedDeclarations | CompletionScope::DynamicValues => None,
        }
    }

    fn should_defer_to_reference_completion(line_prefix: &str) -> bool {
        let Some(reference_completion_path) = ReferenceCompletionPath::from_line_prefix(line_prefix) else {
            return false;
        };

        reference_completion_path.root_keyword().is_some()
            || DeclarationKeyword::from_identifier(reference_completion_path.root_identifier()) == Some(DeclarationKeyword::Mcp)
            || reference_completion_path.is_schema_root()
            || !reference_completion_path.complete_accesses.is_empty()
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
                if ReferenceCompletionPath::from_line_prefix(line_prefix)
                    .is_some_and(|reference_completion_path| reference_completion_path.root_keyword() == Some(ReferenceKeyword::Model))
                {
                    return None;
                }

                Some(semantic_index.model_profile_suggestions(&agent_property_value_completion_context.value_prefix))
            }
            AgentExpressionPropertyName::Instruction => {
                if inside_interpolation_expression || ReferenceCompletionPath::from_line_prefix(line_prefix).is_some() {
                    return None;
                }

                if agent_property_value_completion_context.inside_string_literal {
                    return Some(Vec::new());
                }

                Some(semantic_index.prompt_value_suggestions(&agent_property_value_completion_context.value_prefix, line_prefix))
            }
            AgentExpressionPropertyName::Uses => None,
        }
    }

    fn property_value_non_reference_suggestions(
        semantic_index: &SemanticIndex,
        line_prefix: &str,
        completion_scope: CompletionScope,
    ) -> Option<Vec<CompletionSuggestion>> {
        if let Some(dynamic_value_suggestions) =
            Self::dynamic_value_non_reference_suggestions(semantic_index, line_prefix, completion_scope)
        {
            return Some(dynamic_value_suggestions);
        }

        Self::inference_value_non_reference_suggestions(semantic_index, line_prefix)
    }

    fn should_suppress_prompt_string_literal_suggestions(line_prefix: &str) -> bool {
        if let Some(agent_property_value_completion_context) = AgentPropertyValueCompletionContext::from_line_prefix(line_prefix) {
            return agent_property_value_completion_context.property_name == AgentExpressionPropertyName::Instruction
                && agent_property_value_completion_context.inside_string_literal;
        }

        let trimmed_line_prefix = line_prefix.trim_start();

        let Some((line_before_value, value_prefix)) = trimmed_line_prefix.rsplit_once(':') else {
            return false;
        };

        line_before_value
            .trim_end()
            .ends_with(AgentExpressionPropertyName::Instruction.as_str())
            && super::super::completion_context::ValueCompletionContext::from_value_prefix(value_prefix).inside_string_literal
    }

    pub(in crate::document) fn semantic_index_for_completion(&self, position: Position) -> SemanticIndex {
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

    fn completion_scope(&self, position: Position, line_prefix: &str, semantic_index: &SemanticIndex) -> CompletionScope {
        let Some(cursor_offset) = byte_offset_for_position(&self.text, position) else {
            return CompletionScope::General;
        };

        let line_prefix_scope = completion_scope_at_offset(&self.text, cursor_offset);

        if line_prefix_scope != CompletionScope::General || !Self::can_refine_general_completion_scope(line_prefix) {
            return line_prefix_scope;
        }

        semantic_index.completion_scope_at_position(position).unwrap_or(line_prefix_scope)
    }

    fn can_refine_general_completion_scope(line_prefix: &str) -> bool {
        if DeclarationHeaderCompletionContext::from_line_prefix(line_prefix).is_some() {
            return false;
        }

        let trimmed_line_prefix = line_prefix.trim_start();

        if trimmed_line_prefix.contains(ImportKeyword::From.as_str()) {
            return false;
        }

        trimmed_line_prefix.is_empty() || trailing_identifier(line_prefix).is_some()
    }

    fn is_inside_multiline_string_literal(&self, position: Position) -> bool {
        let Some(cursor_offset) = byte_offset_for_position(&self.text, position) else {
            return false;
        };

        is_inside_multiline_string_literal(&self.text, cursor_offset)
    }
}
