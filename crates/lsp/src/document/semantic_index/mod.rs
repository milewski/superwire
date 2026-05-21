mod construction;
mod definitions;
mod mcp;
mod scopes;
mod type_helpers;
mod types;

use superwire_core::dsl::{
    BuiltinFunctionName, DeclarationKeyword, ImportKeyword, McpCallOperation, ReferenceKeyword, SingletonDeclarationKind, ToolCallKeyword,
};
use superwire_core::semantic::ProviderDriver;

use lsp_types::{CompletionItemKind, Position};

use super::completion_context::{ModelCallCompletionContext, ValueCompletionContext};
use super::hover::builtin_symbol_suggestions;
use super::position::source_span_contains_position;
use super::text_utils::trailing_identifier;
use super::{all_provider_property_names, CompletionSuggestion, RenderTypeExpression};
pub use types::{FieldMetadata, SemanticIndex};

impl SemanticIndex {
    pub fn model_profile_suggestions(&self, model_prefix: &str) -> Vec<CompletionSuggestion> {
        let mut completion_suggestions = self
            .models
            .iter()
            .filter(|(model_name, _)| model_name.starts_with(model_prefix))
            .map(|(model_name, model_summary)| CompletionSuggestion {
                label: model_name.clone(),
                kind: CompletionItemKind::VALUE,
                detail: model_summary.completion_detail(),
                documentation: "Model profile used in `model` properties.".to_string(),
                insert_text: format!("model.{model_name}"),
            })
            .collect::<Vec<_>>();

        completion_suggestions.sort_by(|left_suggestion, right_suggestion| left_suggestion.label.cmp(&right_suggestion.label));

        completion_suggestions
    }

    pub fn model_value_root_suggestions(&self, root_prefix: &str) -> Vec<CompletionSuggestion> {
        [
            ReferenceKeyword::Agent,
            ReferenceKeyword::Dynamic,
            ReferenceKeyword::Input,
            ReferenceKeyword::Secrets,
        ]
        .into_iter()
        .filter(|reference_keyword| reference_keyword.as_str().starts_with(root_prefix))
        .map(|reference_keyword| CompletionSuggestion {
            label: reference_keyword.as_str().to_string(),
            kind: CompletionItemKind::MODULE,
            detail: "Model expression reference root".to_string(),
            documentation: format!("Use `{}.<path>` inside model expressions.", reference_keyword.as_str()),
            insert_text: format!("{}.", reference_keyword.as_str()),
        })
        .collect()
    }

    pub fn inference_value_root_suggestions(&self, root_prefix: &str) -> Vec<CompletionSuggestion> {
        [ReferenceKeyword::Agent, ReferenceKeyword::Dynamic, ReferenceKeyword::Input]
            .into_iter()
            .filter(|reference_keyword| reference_keyword.as_str().starts_with(root_prefix))
            .map(|reference_keyword| CompletionSuggestion {
                label: reference_keyword.as_str().to_string(),
                kind: CompletionItemKind::MODULE,
                detail: "Inference value reference root".to_string(),
                documentation: format!("Use `{}.<path>` for inference values.", reference_keyword.as_str()),
                insert_text: format!("{}.", reference_keyword.as_str()),
            })
            .collect()
    }

    pub fn for_loop_iterable_value_suggestions(&self, value_prefix: &str) -> Vec<CompletionSuggestion> {
        let root_prefix = trailing_identifier(value_prefix).unwrap_or_default();
        let mut completion_suggestions = [
            ReferenceKeyword::Agent,
            ReferenceKeyword::Dynamic,
            ReferenceKeyword::Input,
            ReferenceKeyword::Secrets,
        ]
        .into_iter()
        .filter(|reference_keyword| reference_keyword.as_str().starts_with(root_prefix))
        .map(|reference_keyword| CompletionSuggestion {
            label: reference_keyword.as_str().to_string(),
            kind: CompletionItemKind::MODULE,
            detail: "For-loop iterable reference root".to_string(),
            documentation: format!("Use `{}.<path>` in for-loop iterable expressions.", reference_keyword.as_str()),
            insert_text: format!("{}.", reference_keyword.as_str()),
        })
        .collect::<Vec<_>>();

        if value_prefix.trim().is_empty() {
            completion_suggestions.push(CompletionSuggestion {
                label: "[]".to_string(),
                kind: CompletionItemKind::VALUE,
                detail: "Array literal".to_string(),
                documentation: "Inline array literal iterable expression.".to_string(),
                insert_text: "[]".to_string(),
            });
        }

        completion_suggestions
    }

    pub fn output_value_root_suggestions(&self, root_prefix: &str) -> Vec<CompletionSuggestion> {
        [
            ReferenceKeyword::Agent,
            ReferenceKeyword::Dynamic,
            ReferenceKeyword::Input,
            ReferenceKeyword::Secrets,
        ]
        .into_iter()
        .filter(|reference_keyword| reference_keyword.as_str().starts_with(root_prefix))
        .map(|reference_keyword| CompletionSuggestion {
            label: reference_keyword.as_str().to_string(),
            kind: CompletionItemKind::MODULE,
            detail: "Output value reference root".to_string(),
            documentation: format!("Use `{}.<path>` in output expressions.", reference_keyword.as_str()),
            insert_text: reference_keyword.as_str().to_string(),
        })
        .collect()
    }

    pub fn output_value_suggestions(&self, value_prefix: &str) -> Vec<CompletionSuggestion> {
        let mut completion_suggestions = self.output_value_root_suggestions(value_prefix);
        let literal_suggestion_specs = [
            ("\"\"", "String literal"),
            ("0", "Number literal"),
            ("[]", "Array literal"),
            ("{}", "Object literal"),
            ("true", "Boolean literal"),
            ("false", "Boolean literal"),
            ("null", "Null literal"),
        ];

        completion_suggestions.extend(
            literal_suggestion_specs
                .into_iter()
                .filter(|(literal_label, _)| literal_label.starts_with(value_prefix))
                .map(|(literal_label, literal_detail)| CompletionSuggestion {
                    label: literal_label.to_string(),
                    kind: CompletionItemKind::VALUE,
                    detail: literal_detail.to_string(),
                    documentation: "Literal output value expression.".to_string(),
                    insert_text: literal_label.to_string(),
                }),
        );

        completion_suggestions
    }

    pub fn dynamic_value_suggestions(&self, value_prefix: &str) -> Vec<CompletionSuggestion> {
        let mut completion_suggestions = [
            ReferenceKeyword::Agent,
            ReferenceKeyword::Dynamic,
            ReferenceKeyword::Input,
            ReferenceKeyword::Secrets,
        ]
        .into_iter()
        .filter(|reference_keyword| reference_keyword.as_str().starts_with(value_prefix))
        .map(|reference_keyword| CompletionSuggestion {
            label: reference_keyword.as_str().to_string(),
            kind: CompletionItemKind::MODULE,
            detail: "Dynamic value reference root".to_string(),
            documentation: format!("Use `{}.<path>` in dynamic value expressions.", reference_keyword.as_str()),
            insert_text: format!("{}.", reference_keyword.as_str()),
        })
        .collect::<Vec<_>>();

        let function_suggestion_specs = [
            (
                ToolCallKeyword::Call.as_str(),
                "Tool call expression",
                "Calls a declared tool and stores its output.",
                "call tool.",
            ),
            (
                McpCallOperation::Read.as_str(),
                "MCP resource read expression",
                "Reads an imported MCP resource and stores its rendered content.",
                "read resource.",
            ),
            (
                McpCallOperation::Render.as_str(),
                "MCP prompt render expression",
                "Renders an imported MCP prompt and stores its content.",
                "render prompt.",
            ),
            (
                BuiltinFunctionName::Compact.as_str(),
                "Builtin function",
                "Compacts nullable values in object-like data.",
                "compact($1)",
            ),
            (
                BuiltinFunctionName::Template.as_str(),
                "Builtin function",
                "Renders a string template from source and bindings.",
                "template($1)",
            ),
        ];

        completion_suggestions.extend(
            function_suggestion_specs
                .into_iter()
                .filter(|(label, _, _, _)| label.starts_with(value_prefix))
                .map(|(label, detail, documentation, insert_text)| CompletionSuggestion {
                    label: label.to_string(),
                    kind: CompletionItemKind::FUNCTION,
                    detail: detail.to_string(),
                    documentation: documentation.to_string(),
                    insert_text: insert_text.to_string(),
                }),
        );

        completion_suggestions.sort_by(|left_suggestion, right_suggestion| left_suggestion.label.cmp(&right_suggestion.label));

        completion_suggestions
    }

    pub fn prompt_value_root_suggestions(&self, root_prefix: &str) -> Vec<CompletionSuggestion> {
        [ReferenceKeyword::Agent, ReferenceKeyword::Dynamic, ReferenceKeyword::Input]
            .into_iter()
            .filter(|reference_keyword| reference_keyword.as_str().starts_with(root_prefix))
            .map(|reference_keyword| CompletionSuggestion {
                label: reference_keyword.as_str().to_string(),
                kind: CompletionItemKind::MODULE,
                detail: "Prompt value reference root".to_string(),
                documentation: format!("Use `{}.<path>` in prompt expressions.", reference_keyword.as_str()),
                insert_text: format!("{}.", reference_keyword.as_str()),
            })
            .collect()
    }

    pub fn prompt_interpolation_root_suggestions(&self, root_prefix: &str, position: Position) -> Vec<CompletionSuggestion> {
        let mut completion_suggestions = self.prompt_value_root_suggestions(root_prefix);

        if let Some(for_loop_binding_names) = self.for_loop_binding_names_at_position(position) {
            for for_loop_binding_name in for_loop_binding_names {
                if !for_loop_binding_name.starts_with(root_prefix) {
                    continue;
                }

                completion_suggestions.push(CompletionSuggestion {
                    label: for_loop_binding_name.to_string(),
                    kind: CompletionItemKind::VARIABLE,
                    detail: "For-loop iterator variable".to_string(),
                    documentation: "Iterator binding declared in the current agent for-clause.".to_string(),
                    insert_text: for_loop_binding_name.to_string(),
                });
            }
        }

        completion_suggestions
    }

    pub fn prompt_value_suggestions(&self, value_prefix: &str, line_prefix: &str) -> Vec<CompletionSuggestion> {
        let mut completion_suggestions = self.prompt_value_root_suggestions(value_prefix);
        let single_line_literal = "\"\"";

        if single_line_literal.starts_with(value_prefix) {
            completion_suggestions.push(CompletionSuggestion {
                label: single_line_literal.to_string(),
                kind: CompletionItemKind::VALUE,
                detail: "String literal".to_string(),
                documentation: "Literal prompt expression.".to_string(),
                insert_text: single_line_literal.to_string(),
            });
        }

        let multiline_literal_label = "\"\"\"";
        let multiline_literal_indentation = Self::line_indentation(line_prefix);
        let multiline_literal_insert_text = format!("\"\"\"\n{multiline_literal_indentation}\"\"\"");

        if multiline_literal_label.starts_with(value_prefix) {
            completion_suggestions.push(CompletionSuggestion {
                label: multiline_literal_label.to_string(),
                kind: CompletionItemKind::VALUE,
                detail: "Multiline string literal".to_string(),
                documentation: "Literal prompt expression.".to_string(),
                insert_text: multiline_literal_insert_text,
            });
        }

        let mcp_call_suggestion_specs = [
            (
                McpCallOperation::Read.as_str(),
                "MCP resource read expression",
                "Reads an imported MCP resource as prompt text.",
                "read resource.",
            ),
            (
                McpCallOperation::Render.as_str(),
                "MCP prompt render expression",
                "Renders an imported MCP prompt as prompt text.",
                "render prompt.",
            ),
        ];

        completion_suggestions.extend(
            mcp_call_suggestion_specs
                .into_iter()
                .filter(|(label, _, _, _)| label.starts_with(value_prefix))
                .map(|(label, detail, documentation, insert_text)| CompletionSuggestion {
                    label: label.to_string(),
                    kind: CompletionItemKind::FUNCTION,
                    detail: detail.to_string(),
                    documentation: documentation.to_string(),
                    insert_text: insert_text.to_string(),
                }),
        );

        completion_suggestions
    }

    fn line_indentation(line_prefix: &str) -> &str {
        let indentation_length = line_prefix
            .char_indices()
            .find_map(|(character_offset, character)| (!character.is_whitespace()).then_some(character_offset))
            .unwrap_or(line_prefix.len());

        &line_prefix[..indentation_length]
    }

    pub fn interpolation_root_suggestions(&self, root_prefix: &str, position: Position) -> Vec<CompletionSuggestion> {
        let mut completion_suggestions = [
            ReferenceKeyword::Agent,
            ReferenceKeyword::Dynamic,
            ReferenceKeyword::Input,
            ReferenceKeyword::Secrets,
        ]
        .into_iter()
        .filter(|reference_keyword| reference_keyword.as_str().starts_with(root_prefix))
        .map(|reference_keyword| CompletionSuggestion {
            label: reference_keyword.as_str().to_string(),
            kind: CompletionItemKind::MODULE,
            detail: "Interpolation reference root".to_string(),
            documentation: format!("Use `{}.<path>` inside interpolation expressions.", reference_keyword.as_str()),
            insert_text: format!("{}.", reference_keyword.as_str()),
        })
        .collect::<Vec<_>>();

        if let Some(for_loop_binding_names) = self.for_loop_binding_names_at_position(position) {
            for for_loop_binding_name in for_loop_binding_names {
                if !for_loop_binding_name.starts_with(root_prefix) {
                    continue;
                }

                completion_suggestions.push(CompletionSuggestion {
                    label: for_loop_binding_name.to_string(),
                    kind: CompletionItemKind::VARIABLE,
                    detail: "For-loop iterator variable".to_string(),
                    documentation: "Iterator binding declared in the current agent for-clause.".to_string(),
                    insert_text: for_loop_binding_name.to_string(),
                });
            }
        }

        completion_suggestions
    }

    pub fn context_function_suggestions(&self, value_prefix: &str) -> Vec<CompletionSuggestion> {
        let context_function_label = BuiltinFunctionName::Context.as_str();

        if !context_function_label.starts_with(value_prefix) {
            return Vec::new();
        }

        builtin_symbol_suggestions(true)
            .into_iter()
            .filter(|completion_suggestion| completion_suggestion.label == context_function_label)
            .collect()
    }

    pub fn tool_reference_suggestions(&self, tool_prefix: &str, existing_tool_binding_block: bool) -> Vec<CompletionSuggestion> {
        self.tool_names
            .iter()
            .filter(|tool_name| tool_name.starts_with(tool_prefix))
            .map(|tool_name| {
                let tool_summary = self.tools.get(tool_name);
                let has_bounded_fields = tool_summary.is_some_and(|summary| !summary.bounded_fields.is_empty());
                let insert_text = if has_bounded_fields && !existing_tool_binding_block {
                    format!("{tool_name} {{\n    bindings {{\n        $1\n    }}\n}}")
                } else {
                    tool_name.clone()
                };

                CompletionSuggestion {
                    label: tool_name.clone(),
                    kind: CompletionItemKind::FUNCTION,
                    detail: "Declared tool".to_string(),
                    documentation: tool_summary
                        .and_then(|summary| summary.description.clone())
                        .unwrap_or_else(|| "Tool declared in this document.".to_string()),
                    insert_text,
                }
            })
            .collect()
    }

    pub fn tool_bounded_argument_suggestions(
        &self,
        tool_name: &str,
        argument_prefix: &str,
        existing_argument_names: &[String],
    ) -> Vec<CompletionSuggestion> {
        let Some(tool_summary) = self.tools.get(tool_name) else {
            return Vec::new();
        };

        tool_summary
            .bounded_fields
            .iter()
            .filter(|(field_name, _)| field_name.starts_with(argument_prefix))
            .filter(|(field_name, _)| !existing_argument_names.contains(field_name))
            .map(|(field_name, field_type)| CompletionSuggestion {
                label: field_name.clone(),
                kind: CompletionItemKind::PROPERTY,
                detail: tool_summary
                    .bounded_field_metadata
                    .get(field_name)
                    .and_then(|field_metadata| field_metadata.description.clone())
                    .unwrap_or_else(|| format!("Bound tool argument: {}", field_type.render_type())),
                documentation: tool_summary
                    .bounded_field_metadata
                    .get(field_name)
                    .and_then(|field_metadata| field_metadata.description.clone())
                    .unwrap_or_else(|| "Bound argument for this tool call.".to_string()),
                insert_text: format!("{field_name}: $1"),
            })
            .collect()
    }

    pub fn model_call_suggestions(&self, model_call_context: &ModelCallCompletionContext) -> Vec<CompletionSuggestion> {
        let mut completion_suggestions = self
            .models
            .iter()
            .filter(|(model_name, model_summary)| {
                model_summary.provider_name == model_call_context.provider_name && model_name.starts_with(&model_call_context.model_prefix)
            })
            .map(|(model_name, model_summary)| CompletionSuggestion {
                label: model_name.clone(),
                kind: CompletionItemKind::VALUE,
                detail: model_summary.completion_detail(),
                documentation: "Declared model profile using this provider.".to_string(),
                insert_text: model_name.clone(),
            })
            .collect::<Vec<_>>();

        completion_suggestions.sort_by(|left_suggestion, right_suggestion| left_suggestion.label.cmp(&right_suggestion.label));

        completion_suggestions
    }

    pub fn provider_driver_value_suggestions(&self, position: Position, line_prefix: &str) -> Option<Vec<CompletionSuggestion>> {
        let provider_name = self.provider_name_at_position(position)?;
        let _ = provider_name;

        let trimmed_line_prefix = line_prefix.trim_start();
        let (line_before_value, property_value_prefix) = trimmed_line_prefix.rsplit_once(':')?;
        let property_name_identifier = trailing_identifier(line_before_value)?;

        if property_name_identifier != "driver" {
            return None;
        }

        let value_completion_context = ValueCompletionContext::from_value_prefix(property_value_prefix);
        let completion_suggestions = Self::provider_driver_suggestions(
            &value_completion_context.value_prefix,
            "Provider driver",
            "Valid provider driver value.",
        )
        .into_iter()
        .map(|completion_suggestion| {
            let insert_text = if value_completion_context.inside_string_literal {
                completion_suggestion.label.clone()
            } else {
                format!("\"{}\"", completion_suggestion.label)
            };

            CompletionSuggestion {
                insert_text,
                ..completion_suggestion
            }
        })
        .collect::<Vec<_>>();

        Some(completion_suggestions)
    }

    pub fn provider_driver_suggestions(
        driver_prefix: &str,
        detail: &'static str,
        documentation: &'static str,
    ) -> Vec<CompletionSuggestion> {
        let mut completion_suggestions = ProviderDriver::all()
            .into_iter()
            .map(superwire_core::semantic::ProviderDriver::as_str)
            .filter(|driver_name| driver_name.starts_with(driver_prefix))
            .map(|driver_name| CompletionSuggestion {
                label: driver_name.to_string(),
                kind: CompletionItemKind::VALUE,
                detail: detail.to_string(),
                documentation: documentation.to_string(),
                insert_text: driver_name.to_string(),
            })
            .collect::<Vec<_>>();

        completion_suggestions.sort_by(|left_suggestion, right_suggestion| left_suggestion.label.cmp(&right_suggestion.label));

        completion_suggestions
    }

    pub fn provider_models_value_suggestions(&self, line_prefix: &str) -> Option<Vec<CompletionSuggestion>> {
        let trimmed_line_prefix = line_prefix.trim_start();
        let (line_before_value, property_value_prefix) = trimmed_line_prefix.rsplit_once(':')?;
        let property_name_identifier = trailing_identifier(line_before_value)?;

        if property_name_identifier != "models" {
            return None;
        }

        let value_completion_context = ValueCompletionContext::from_value_prefix(property_value_prefix);

        if value_completion_context.inside_string_literal {
            return Some(Vec::new());
        }

        let mut completion_suggestions = [
            CompletionSuggestion {
                label: "[]".to_string(),
                kind: CompletionItemKind::VALUE,
                detail: "Model list".to_string(),
                documentation: "Array of supported model identifiers.".to_string(),
                insert_text: "[]".to_string(),
            },
            CompletionSuggestion {
                label: "[\"\"]".to_string(),
                kind: CompletionItemKind::VALUE,
                detail: "Model list template".to_string(),
                documentation: "Array literal with one model placeholder string.".to_string(),
                insert_text: "[\"$1\"]".to_string(),
            },
        ]
        .into_iter()
        .filter(|completion_suggestion| completion_suggestion.label.starts_with(&value_completion_context.value_prefix))
        .collect::<Vec<_>>();

        completion_suggestions.sort_by(|left_suggestion, right_suggestion| left_suggestion.label.cmp(&right_suggestion.label));

        Some(completion_suggestions)
    }

    pub fn provider_property_suggestions(&self, position: Position, line_prefix: &str) -> Option<Vec<CompletionSuggestion>> {
        let provider_name = self.provider_name_at_position(position)?;

        if line_prefix.trim_start().contains(':') {
            return None;
        }

        let property_prefix = trailing_identifier(line_prefix).unwrap_or_default();
        let mut provider_property_names = self.provider_property_names(provider_name);

        provider_property_names.retain(|property_name| property_name.starts_with(property_prefix));

        if provider_property_names.is_empty() {
            return None;
        }

        let completion_suggestions = provider_property_names
            .into_iter()
            .map(|property_name| CompletionSuggestion {
                label: property_name.to_string(),
                kind: CompletionItemKind::PROPERTY,
                detail: format!("`{provider_name}` provider property"),
                documentation: "Provider configuration property.".to_string(),
                insert_text: property_name.to_string(),
            })
            .collect::<Vec<_>>();

        Some(completion_suggestions)
    }

    fn provider_property_names(&self, provider_name: &str) -> Vec<&'static str> {
        let Some(provider_summary) = self.providers.get(provider_name) else {
            return all_provider_property_names();
        };

        if let Some(provider_driver) = provider_summary.driver {
            return provider_driver.available_property_names().to_vec();
        }

        all_provider_property_names()
    }

    pub fn is_type_position(&self, position: Position, line_prefix: &str) -> bool {
        let trimmed_line_prefix = line_prefix.trim_end();

        if trimmed_line_prefix.ends_with("schema.") {
            return true;
        }

        let looks_like_type_trigger = trimmed_line_prefix.ends_with(':')
            || trimmed_line_prefix.ends_with('|')
            || trimmed_line_prefix.ends_with('[')
            || trimmed_line_prefix.ends_with('(')
            || trimmed_line_prefix.ends_with(',');

        if !looks_like_type_trigger {
            return false;
        }

        let inside_output = self
            .output_locations
            .iter()
            .copied()
            .any(|output_span| source_span_contains_position(output_span, position));

        if inside_output {
            return false;
        }

        let inside_typed_declaration = self
            .typed_declaration_locations
            .iter()
            .copied()
            .any(|typed_declaration_span| source_span_contains_position(typed_declaration_span, position));

        if inside_typed_declaration {
            return true;
        }

        false
    }

    pub fn is_inside_agent_output_declaration(&self, position: Position) -> bool {
        self.agent_output_locations
            .iter()
            .copied()
            .any(|typed_declaration_span| source_span_contains_position(typed_declaration_span, position))
    }

    pub fn root_declaration_suggestions(&self, line_prefix: &str) -> Vec<CompletionSuggestion> {
        let declaration_prefix = trailing_identifier(line_prefix).unwrap_or_default();

        let mut completion_suggestions = builtin_symbol_suggestions(false)
            .into_iter()
            .filter(|completion_suggestion| matches!(completion_suggestion.kind, CompletionItemKind::KEYWORD))
            .filter(|completion_suggestion| completion_suggestion.label.starts_with(declaration_prefix))
            .filter(|completion_suggestion| self.should_suggest_root_declaration_label(&completion_suggestion.label))
            .collect::<Vec<_>>();

        if ImportKeyword::From.as_str().starts_with(declaration_prefix) {
            completion_suggestions.push(CompletionSuggestion {
                label: ImportKeyword::From.as_str().to_string(),
                kind: CompletionItemKind::KEYWORD,
                detail: "MCP tool batch import".to_string(),
                documentation: "Batch imports MCP tools from a server and applies shared bindings.".to_string(),
                insert_text: "from mcp.$1.tool {\n    bindings {\n        $2\n    }\n\n    tool $3\n}".to_string(),
            });

            completion_suggestions.push(CompletionSuggestion {
                label: ImportKeyword::From.as_str().to_string(),
                kind: CompletionItemKind::KEYWORD,
                detail: "MCP resource batch import".to_string(),
                documentation: "Batch imports MCP resources from a server and applies shared bindings.".to_string(),
                insert_text: "from mcp.$1.resource {\n    bindings {\n        $2\n    }\n\n    resource $3\n}".to_string(),
            });

            completion_suggestions.push(CompletionSuggestion {
                label: ImportKeyword::From.as_str().to_string(),
                kind: CompletionItemKind::KEYWORD,
                detail: "MCP prompt batch import".to_string(),
                documentation: "Batch imports MCP prompts from a server and applies shared bindings.".to_string(),
                insert_text: "from mcp.$1.prompt {\n    bindings {\n        $2\n    }\n\n    prompt $3\n}".to_string(),
            });

            completion_suggestions.push(CompletionSuggestion {
                label: ImportKeyword::From.as_str().to_string(),
                kind: CompletionItemKind::KEYWORD,
                detail: "MCP batch import".to_string(),
                documentation: "Batch imports MCP tools, resources, and prompts from a server with shared bindings.".to_string(),
                insert_text: "from mcp.$1 {\n    bindings {\n        $2\n    }\n\n    resource $3\n    prompt $4\n    tool $5\n}"
                    .to_string(),
            });
        }

        completion_suggestions.sort_by(|left_suggestion, right_suggestion| left_suggestion.label.cmp(&right_suggestion.label));
        completion_suggestions
    }

    pub fn is_output_position(&self, position: Position) -> bool {
        self.output_locations
            .iter()
            .copied()
            .any(|output_span| source_span_contains_position(output_span, position))
    }

    fn should_suggest_root_declaration_label(&self, declaration_label: &str) -> bool {
        if declaration_label == DeclarationKeyword::Provider.as_str() {
            return true;
        }

        if declaration_label == DeclarationKeyword::Agent.as_str() {
            return true;
        }

        if declaration_label == DeclarationKeyword::Schema.as_str() {
            return true;
        }

        if declaration_label == DeclarationKeyword::Tool.as_str() {
            return true;
        }

        if declaration_label == DeclarationKeyword::Resource.as_str() {
            return true;
        }

        if declaration_label == DeclarationKeyword::Prompt.as_str() {
            return true;
        }

        if declaration_label == DeclarationKeyword::Dynamic.as_str() {
            return true;
        }

        if declaration_label == SingletonDeclarationKind::Input.as_str() {
            return !self.has_input_declaration;
        }

        if declaration_label == SingletonDeclarationKind::Secrets.as_str() {
            return !self.has_secrets_declaration;
        }

        if declaration_label == SingletonDeclarationKind::Output.as_str() {
            return !self.has_output_declaration;
        }

        false
    }

    pub fn default_suggestions(&self, include_builtin_function_suggestions: bool) -> Vec<CompletionSuggestion> {
        let mut completion_suggestions = builtin_symbol_suggestions(include_builtin_function_suggestions);

        completion_suggestions.extend(self.providers.keys().map(|provider_name| CompletionSuggestion {
            label: provider_name.clone(),
            kind: CompletionItemKind::FUNCTION,
            detail: "Declared provider".to_string(),
            documentation: "Provider call used in `model` properties.".to_string(),
            insert_text: provider_name.clone(),
        }));

        completion_suggestions.extend(self.agent_names.iter().map(|agent_name| CompletionSuggestion {
            label: agent_name.clone(),
            kind: CompletionItemKind::VARIABLE,
            detail: "Declared agent".to_string(),
            documentation: "Agent declared in this document.".to_string(),
            insert_text: agent_name.clone(),
        }));

        completion_suggestions.sort_by(|left_suggestion, right_suggestion| left_suggestion.label.cmp(&right_suggestion.label));

        completion_suggestions
    }

    pub fn provider_reference_suggestions(&self, provider_prefix: &str) -> Vec<CompletionSuggestion> {
        let mut completion_suggestions = self
            .providers
            .keys()
            .filter(|provider_name| provider_name.starts_with(provider_prefix))
            .map(|provider_name| CompletionSuggestion {
                label: provider_name.clone(),
                kind: CompletionItemKind::VALUE,
                detail: "Declared provider".to_string(),
                documentation: "Provider declared in this document.".to_string(),
                insert_text: provider_name.clone(),
            })
            .collect::<Vec<_>>();

        completion_suggestions.sort_by(|left_suggestion, right_suggestion| left_suggestion.label.cmp(&right_suggestion.label));

        completion_suggestions
    }

    pub fn is_root_declaration_position(&self, position: Position) -> bool {
        if self
            .provider_locations
            .iter()
            .any(|provider_location| source_span_contains_position(provider_location.span, position))
        {
            return false;
        }

        if self
            .schema_locations
            .iter()
            .any(|schema_location| source_span_contains_position(schema_location.span, position))
        {
            return false;
        }

        if self
            .agent_locations
            .iter()
            .any(|agent_location| source_span_contains_position(agent_location.span, position))
        {
            return false;
        }

        if self
            .tool_locations
            .iter()
            .any(|tool_location| source_span_contains_position(tool_location.span, position))
        {
            return false;
        }

        if self
            .typed_declaration_locations
            .iter()
            .copied()
            .any(|typed_declaration_span| source_span_contains_position(typed_declaration_span, position))
        {
            return false;
        }

        if self
            .output_locations
            .iter()
            .copied()
            .any(|output_span| source_span_contains_position(output_span, position))
        {
            return false;
        }

        true
    }

    pub fn for_loop_destructuring_binding_suggestions(
        &self,
        position: Position,
        field_prefix: &str,
        existing_field_names: &[String],
    ) -> Vec<CompletionSuggestion> {
        let Some(agent_name) = self.agent_name_at_position(position) else {
            return Vec::new();
        };

        let Some(iterable_item_type) = self.agent_for_loop_iterable_item_types.get(agent_name) else {
            return Vec::new();
        };

        let mut available_fields = self
            .tooling_snapshot
            .available_fields_for_types(std::slice::from_ref(iterable_item_type));

        for existing_field_name in existing_field_names {
            let _ = available_fields.remove(existing_field_name);
        }

        available_fields
            .into_iter()
            .filter(|(field_name, _)| field_name.starts_with(field_prefix))
            .map(|(field_name, _)| CompletionSuggestion {
                label: field_name.clone(),
                kind: CompletionItemKind::PROPERTY,
                detail: "Destructured for-loop field".to_string(),
                documentation: "Field available on each item in the for-loop iterable expression.".to_string(),
                insert_text: field_name,
            })
            .collect()
    }
}
