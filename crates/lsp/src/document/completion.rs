use superwire_core::dsl::{
    parse_workflow, AgentExpressionPropertyName, AgentPropertyName, DeclarationKeyword, ForClauseKeyword, ReferenceKeyword,
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
    agent_property_scope_suggestions, completion_scope_at_offset, inference_setting_scope_suggestions, tool_property_scope_suggestions,
    CompletionScope,
};
use super::semantic_index::SemanticIndex;
use super::text_utils::{
    is_inside_interpolation_expression, is_inside_multiline_string_literal, trailing_identifier, trailing_reference_token,
};
use super::{CompletionSuggestion, DocumentState};
use superwire_core::runtime::InferenceSetting;

const COMPLETION_RECOVERY_PLACEHOLDER: &str = "__completion_placeholder";

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

        if let Some(typed_declaration_suggestions) =
            self.typed_declaration_scope_suggestions(completion_scope, &line_prefix, position, &semantic_index)
        {
            return typed_declaration_suggestions;
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
            &line_prefix,
            &line_suffix,
            position,
            inside_interpolation_expression,
            inference_setting_value_completion_context.as_ref(),
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
        }

        if completion_scope == CompletionScope::General
            && !line_has_property_separator
            && !inside_interpolation_expression
            && semantic_index.is_output_position(position)
        {
            return Some(Vec::new());
        }

        if line_has_property_separator {
            if let Some(inference_value_completion_context) = InferenceSettingValueCompletionContext::from_line_prefix(line_prefix) {
                if inference_value_completion_context.inside_string_literal {
                    return Some(Vec::new());
                }

                if ReferenceCompletionPath::from_line_prefix(line_prefix).is_none() {
                    if inference_value_completion_context.value_prefix.is_empty() {
                        return Some(semantic_index.inference_value_root_suggestions(""));
                    }

                    return Some(Vec::new());
                }
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

            if let Some(scope_suggestions) = Self::property_scope_suggestions(completion_scope, line_prefix) {
                return Some(scope_suggestions);
            }

            if completion_scope == CompletionScope::General && semantic_index.is_root_declaration_position(position) {
                return Some(semantic_index.root_declaration_suggestions(line_prefix));
            }
        }

        self.provider_non_reference_suggestions(semantic_index, line_prefix, position)
    }

    fn property_scope_suggestions(completion_scope: CompletionScope, line_prefix: &str) -> Option<Vec<CompletionSuggestion>> {
        match completion_scope {
            CompletionScope::InferenceSettings => Some(inference_setting_scope_suggestions(line_prefix)),
            CompletionScope::AgentProperties => Some(agent_property_scope_suggestions(line_prefix)),
            CompletionScope::ToolProperties => Some(tool_property_scope_suggestions(line_prefix)),
            CompletionScope::General | CompletionScope::TypedDeclarations => None,
        }
    }

    fn should_defer_to_reference_completion(line_prefix: &str) -> bool {
        let Some(reference_completion_path) = ReferenceCompletionPath::from_line_prefix(line_prefix) else {
            return false;
        };

        reference_completion_path.root_keyword().is_some()
            || reference_completion_path.is_schema_root()
            || !reference_completion_path.complete_accesses.is_empty()
    }

    fn has_existing_tool_call_parentheses(line_suffix: &str) -> bool {
        line_suffix.trim_start().starts_with('(')
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

    fn reference_completion_suggestions(
        &self,
        semantic_index: &SemanticIndex,
        line_prefix: &str,
        line_suffix: &str,
        position: Position,
        inside_interpolation_expression: bool,
        inference_setting_value_completion_context: Option<&InferenceSettingValueCompletionContext>,
    ) -> Option<Vec<CompletionSuggestion>> {
        let line_has_property_separator = line_prefix.trim_start().contains(':');
        let reference_completion_path = ReferenceCompletionPath::from_line_prefix(line_prefix)?;
        let for_loop_iterable_reference_context = matches!(
            reference_completion_path.root_keyword(),
            Some(ReferenceKeyword::Input | ReferenceKeyword::Agent | ReferenceKeyword::Dynamic | ReferenceKeyword::Secrets)
        ) && Self::is_for_loop_iterable_clause_context(line_prefix);

        let reference_completion_constraint = if for_loop_iterable_reference_context {
            ReferenceCompletionConstraint::ForLoopIterable
        } else {
            self.reference_completion_constraint(line_prefix, inference_setting_value_completion_context)
        };

        let reference_suggestions = semantic_index.reference_path_suggestions(
            &reference_completion_path,
            reference_completion_constraint,
            position,
            Self::has_existing_tool_call_parentheses(line_suffix),
        );

        if let Some(inference_suggestions) = self.inference_reference_completion_suggestions(
            semantic_index,
            line_prefix,
            &reference_completion_path,
            &reference_suggestions,
            inference_setting_value_completion_context,
        ) {
            return Some(inference_suggestions);
        }

        if let Some(interpolation_suggestions) = self.interpolation_reference_completion_suggestions(
            semantic_index,
            line_prefix,
            position,
            &reference_completion_path,
            &reference_suggestions,
            inside_interpolation_expression,
        ) {
            return Some(interpolation_suggestions);
        }

        if let Some(output_suggestions) = self.output_reference_completion_suggestions(
            semantic_index,
            line_prefix,
            position,
            &reference_completion_path,
            &reference_suggestions,
            line_has_property_separator,
        ) {
            return Some(output_suggestions);
        }

        if let Some(prompt_suggestions) = self.prompt_reference_completion_suggestions(
            semantic_index,
            line_prefix,
            &reference_completion_path,
            &reference_suggestions,
            line_has_property_separator,
            inside_interpolation_expression,
        ) {
            return Some(prompt_suggestions);
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
            Some(ReferenceKeyword::Secrets | ReferenceKeyword::Tool) | None => {
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
            Some(ReferenceKeyword::Secrets | ReferenceKeyword::Tool) | None => {
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
            Some(ReferenceKeyword::Tool) | None => {
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
            Some(ReferenceKeyword::Secrets | ReferenceKeyword::Tool) | None => {
                if can_suggest_prompt_roots {
                    return Some(semantic_index.prompt_value_root_suggestions(reference_completion_path.root_identifier()));
                }

                Some(Vec::new())
            }
        }
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

        if reference_root_keyword == Some(ReferenceKeyword::Tool) {
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

    fn semantic_index_for_completion(&self, position: Position) -> SemanticIndex {
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

        Some(SemanticIndex::from_workflow(&workflow))
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
