use std::collections::{BTreeMap, HashMap};

use superwire_core::dsl::{
    AgentForLoopPattern, AgentProperty, BuiltinFunctionName, Declaration, DeclarationKeyword, Expression, ImportKeyword, McpCallOperation,
    ModelDeclaration, ProviderDeclaration, ReferenceKeyword, SingletonDeclarationKind, SourceSpan, ToolCallKeyword, ToolPropertyName,
    TypeExpression, TypedField, Workflow,
};
use superwire_core::mcp::McpLock;
use superwire_core::mcp::McpServerLock;
use superwire_core::semantic::ProviderDriver;
use superwire_core::semantic::{SemanticToolingSnapshot, ToolingReferencePath, ToolingSymbolCategory};

use lsp_types::{CompletionItemKind, Position};

use super::completion_context::{ModelCallCompletionContext, ValueCompletionContext};
use super::hover::builtin_symbol_suggestions;
use super::position::source_span_contains_position;
use super::reference::ReferenceCompletionPath;
use super::text_utils::trailing_identifier;
use super::{all_provider_property_names, type_symbol_suggestions, CompletionSuggestion, RenderTypeExpression};

#[derive(Debug, Clone)]
pub struct SemanticIndex {
    pub providers: HashMap<String, ProviderSummary>,
    pub provider_locations: Vec<NamedSpan>,
    pub models: HashMap<String, ModelSummary>,
    pub model_locations: Vec<NamedSpan>,
    pub schemas: HashMap<String, SchemaSummary>,
    pub schema_names: Vec<String>,
    pub schema_locations: Vec<NamedSpan>,
    schema_field_locations: HashMap<String, SourceSpan>,
    pub tools: HashMap<String, ToolSummary>,
    pub tool_names: Vec<String>,
    pub tool_locations: Vec<NamedSpan>,
    pub resource_names: Vec<String>,
    pub resource_locations: Vec<NamedSpan>,
    pub prompt_names: Vec<String>,
    pub prompt_locations: Vec<NamedSpan>,
    pub mcp_server_names: Vec<String>,
    pub input_fields: BTreeMap<String, TypeExpression>,
    pub input_field_metadata: BTreeMap<String, FieldMetadata>,
    input_field_locations: HashMap<String, SourceSpan>,
    pub secrets_fields: BTreeMap<String, TypeExpression>,
    pub secrets_field_metadata: BTreeMap<String, FieldMetadata>,
    secrets_field_locations: HashMap<String, SourceSpan>,
    pub dynamic_fields: BTreeMap<String, TypeExpression>,
    pub dynamic_field_metadata: BTreeMap<String, FieldMetadata>,
    dynamic_field_locations: HashMap<String, SourceSpan>,
    pub agents: HashMap<String, AgentSummary>,
    pub agent_dynamic_fields: HashMap<String, BTreeMap<String, TypeExpression>>,
    pub agent_dynamic_field_metadata: HashMap<String, BTreeMap<String, FieldMetadata>>,
    agent_dynamic_field_locations: HashMap<String, HashMap<String, SourceSpan>>,
    agent_output_field_locations: HashMap<String, SourceSpan>,
    pub agent_for_loop_bindings: HashMap<String, BTreeMap<String, Vec<TypeExpression>>>,
    pub agent_for_loop_iterable_item_types: HashMap<String, TypeExpression>,
    pub agent_names: Vec<String>,
    pub output_locations: Vec<SourceSpan>,
    pub typed_declaration_locations: Vec<SourceSpan>,
    pub agent_output_locations: Vec<SourceSpan>,
    pub agent_locations: Vec<NamedSpan>,
    has_input_declaration: bool,
    has_secrets_declaration: bool,
    has_output_declaration: bool,
    pub tooling_snapshot: SemanticToolingSnapshot,
    pub mcp_lock: Option<McpLock>,
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

    pub fn mcp_tool_batch_item_suggestions(
        &self,
        server_name: &str,
        tool_prefix: &str,
        existing_tool_names: &[String],
    ) -> Vec<CompletionSuggestion> {
        let Some(server_lock) = self.mcp_lock.as_ref().and_then(|mcp_lock| mcp_lock.servers.get(server_name)) else {
            return Vec::new();
        };

        let mut normalized_tool_names = server_lock
            .tools
            .keys()
            .map(|tool_name| McpServerLock::normalize_item_name(tool_name))
            .filter(|normalized_tool_name| normalized_tool_name.starts_with(tool_prefix))
            .filter(|normalized_tool_name| !existing_tool_names.contains(normalized_tool_name))
            .collect::<Vec<_>>();

        normalized_tool_names.sort();
        normalized_tool_names.dedup();

        normalized_tool_names
            .into_iter()
            .map(|normalized_tool_name| CompletionSuggestion {
                label: normalized_tool_name.clone(),
                kind: CompletionItemKind::VALUE,
                detail: "MCP tool".to_string(),
                documentation: format!("Import MCP tool `{normalized_tool_name}` from server `{server_name}`."),
                insert_text: normalized_tool_name,
            })
            .collect()
    }

    pub fn mcp_resource_batch_item_suggestions(
        &self,
        server_name: &str,
        resource_prefix: &str,
        existing_resource_names: &[String],
    ) -> Vec<CompletionSuggestion> {
        let Some(server_lock) = self.mcp_lock.as_ref().and_then(|mcp_lock| mcp_lock.servers.get(server_name)) else {
            return Vec::new();
        };

        let mut normalized_resource_names = server_lock
            .resources
            .iter()
            .map(|resource_name| McpServerLock::normalize_item_name(resource_name))
            .filter(|normalized_resource_name| normalized_resource_name.starts_with(resource_prefix))
            .filter(|normalized_resource_name| !existing_resource_names.contains(normalized_resource_name))
            .collect::<Vec<_>>();

        normalized_resource_names.sort();
        normalized_resource_names.dedup();

        normalized_resource_names
            .into_iter()
            .map(|normalized_resource_name| CompletionSuggestion {
                label: normalized_resource_name.clone(),
                kind: CompletionItemKind::VALUE,
                detail: "MCP resource".to_string(),
                documentation: format!("Import MCP resource `{normalized_resource_name}` from server `{server_name}`."),
                insert_text: normalized_resource_name,
            })
            .collect()
    }

    pub fn mcp_prompt_batch_item_suggestions(
        &self,
        server_name: &str,
        prompt_prefix: &str,
        existing_prompt_names: &[String],
    ) -> Vec<CompletionSuggestion> {
        let Some(server_lock) = self.mcp_lock.as_ref().and_then(|mcp_lock| mcp_lock.servers.get(server_name)) else {
            return Vec::new();
        };

        let mut normalized_prompt_names = server_lock
            .prompts
            .iter()
            .map(|prompt_name| McpServerLock::normalize_item_name(prompt_name))
            .filter(|normalized_prompt_name| normalized_prompt_name.starts_with(prompt_prefix))
            .filter(|normalized_prompt_name| !existing_prompt_names.contains(normalized_prompt_name))
            .collect::<Vec<_>>();

        normalized_prompt_names.sort();
        normalized_prompt_names.dedup();

        normalized_prompt_names
            .into_iter()
            .map(|normalized_prompt_name| CompletionSuggestion {
                label: normalized_prompt_name.clone(),
                kind: CompletionItemKind::VALUE,
                detail: "MCP prompt".to_string(),
                documentation: format!("Import MCP prompt `{normalized_prompt_name}` from server `{server_name}`."),
                insert_text: normalized_prompt_name,
            })
            .collect()
    }

    pub fn mcp_prompt_binding_suggestions(
        &self,
        server_name: &str,
        prompt_name: &str,
        binding_prefix: &str,
        existing_binding_names: &[String],
    ) -> Vec<CompletionSuggestion> {
        let Some(server_lock) = self.mcp_lock.as_ref().and_then(|mcp_lock| mcp_lock.servers.get(server_name)) else {
            return Vec::new();
        };
        let Some(prompt_arguments) = server_lock.prompt_arguments_for_name(prompt_name) else {
            return Vec::new();
        };

        prompt_arguments
            .iter()
            .filter(|prompt_argument| prompt_argument.name.starts_with(binding_prefix))
            .filter(|prompt_argument| !existing_binding_names.contains(&prompt_argument.name))
            .map(|prompt_argument| {
                let requirement_detail = if prompt_argument.required {
                    "Required prompt argument"
                } else {
                    "Optional prompt argument"
                };
                let documentation = prompt_argument.description.clone().unwrap_or_else(|| {
                    format!(
                        "{} argument `{}` from MCP prompt `{}`.",
                        if prompt_argument.required { "Required" } else { "Optional" },
                        prompt_argument.name,
                        prompt_name,
                    )
                });

                CompletionSuggestion {
                    label: prompt_argument.name.clone(),
                    kind: CompletionItemKind::PROPERTY,
                    detail: requirement_detail.to_string(),
                    documentation,
                    insert_text: format!("{}: $1", prompt_argument.name),
                }
            })
            .collect()
    }

    pub fn mcp_tool_schema_field_suggestions(
        &self,
        tool_name: &str,
        property_name: ToolPropertyName,
        field_prefix: &str,
        existing_field_names: &[String],
    ) -> Vec<CompletionSuggestion> {
        self.mcp_tool_schema_fields(tool_name, property_name)
            .iter()
            .filter(|typed_field| typed_field.name.starts_with(field_prefix))
            .filter(|typed_field| !existing_field_names.contains(&typed_field.name))
            .map(|typed_field| {
                let rendered_type = typed_field.field_type.render_type();
                let insert_text = if property_name == ToolPropertyName::Bindings {
                    format!("{}: $1", typed_field.name)
                } else {
                    format!("{}: {rendered_type}", typed_field.name)
                };

                CompletionSuggestion {
                    label: typed_field.name.clone(),
                    kind: CompletionItemKind::PROPERTY,
                    detail: typed_field.description.clone().unwrap_or_else(|| rendered_type.clone()),
                    documentation: typed_field
                        .description
                        .clone()
                        .unwrap_or_else(|| format!("MCP tool {} field of type `{rendered_type}`.", property_name.as_str())),
                    insert_text,
                }
            })
            .collect()
    }

    pub fn mcp_tool_schema_fields(&self, tool_name: &str, property_name: ToolPropertyName) -> Vec<TypedField> {
        let Some(mcp_tool_lock) = self.mcp_tool_lock(tool_name) else {
            return Vec::new();
        };

        Self::schema_fields_from_mcp_tool_lock(mcp_tool_lock, property_name)
    }

    pub fn mcp_tool_schema_fields_for_source(
        &self,
        server_name: Option<&str>,
        mcp_tool_name: &str,
        property_name: ToolPropertyName,
    ) -> Vec<TypedField> {
        let Some(mcp_tool_lock) = self.mcp_tool_lock_for_source(server_name, mcp_tool_name) else {
            return Vec::new();
        };

        Self::schema_fields_from_mcp_tool_lock(mcp_tool_lock, property_name)
    }

    pub fn mcp_tool_batch_common_schema_fields(
        &self,
        server_name: &str,
        tool_names: &[String],
        property_name: ToolPropertyName,
    ) -> Vec<TypedField> {
        let Some(server_lock) = self.mcp_lock.as_ref().and_then(|mcp_lock| mcp_lock.servers.get(server_name)) else {
            return Vec::new();
        };
        let tool_locks = if tool_names.is_empty() {
            server_lock.tools.values().collect::<Vec<_>>()
        } else {
            tool_names
                .iter()
                .filter_map(|tool_name| {
                    server_lock
                        .find_tool_with_name(tool_name)
                        .map(|(_resolved_tool_name, mcp_tool_lock)| mcp_tool_lock)
                })
                .collect::<Vec<_>>()
        };
        let mut tool_locks = tool_locks.into_iter();
        let Some(first_tool_lock) = tool_locks.next() else {
            return Vec::new();
        };
        let mut common_fields = Self::schema_fields_from_mcp_tool_lock(first_tool_lock, property_name);

        for mcp_tool_lock in tool_locks {
            let tool_fields = Self::schema_fields_from_mcp_tool_lock(mcp_tool_lock, property_name);

            common_fields.retain(|common_field| {
                tool_fields
                    .iter()
                    .any(|tool_field| tool_field.name == common_field.name && tool_field.field_type == common_field.field_type)
            });
        }

        common_fields
    }

    fn schema_fields_from_mcp_tool_lock(
        mcp_tool_lock: &superwire_core::mcp::McpToolLock,
        property_name: ToolPropertyName,
    ) -> Vec<TypedField> {
        match property_name {
            ToolPropertyName::Input | ToolPropertyName::Bindings => mcp_tool_lock.input_fields_except(&[]),
            ToolPropertyName::Output => mcp_tool_lock.output_fields(),
            ToolPropertyName::Description | ToolPropertyName::MaxCalls => Vec::new(),
        }
    }

    fn mcp_tool_lock(&self, tool_name: &str) -> Option<&superwire_core::mcp::McpToolLock> {
        let tool_summary = self.tools.get(tool_name)?;
        let mcp_tool_name = tool_summary.mcp_tool_name.as_deref()?;

        self.mcp_tool_lock_for_source(tool_summary.mcp_server_name.as_deref(), mcp_tool_name)
    }

    fn mcp_tool_lock_for_source(&self, server_name: Option<&str>, mcp_tool_name: &str) -> Option<&superwire_core::mcp::McpToolLock> {
        let mcp_lock = self.mcp_lock.as_ref()?;

        if let Some(server_name) = server_name {
            let server_lock = mcp_lock.servers.get(server_name)?;

            return server_lock
                .find_tool_with_name(mcp_tool_name)
                .map(|(_resolved_tool_name, mcp_tool_lock)| mcp_tool_lock);
        }

        mcp_lock.servers.values().find_map(|server_lock| {
            server_lock
                .find_tool_with_name(mcp_tool_name)
                .map(|(_resolved_tool_name, mcp_tool_lock)| mcp_tool_lock)
        })
    }

    pub fn from_workflow_with_mcp_lock(workflow: &Workflow, mcp_lock: Option<McpLock>) -> Self {
        let tooling_snapshot = SemanticToolingSnapshot::from_workflow(workflow);
        let mut semantic_index = Self {
            providers: HashMap::new(),
            provider_locations: Vec::new(),
            models: HashMap::new(),
            model_locations: Vec::new(),
            schemas: HashMap::new(),
            schema_names: Vec::new(),
            schema_locations: Vec::new(),
            schema_field_locations: HashMap::new(),
            tools: HashMap::new(),
            tool_names: Vec::new(),
            tool_locations: Vec::new(),
            resource_names: Vec::new(),
            resource_locations: Vec::new(),
            prompt_names: Vec::new(),
            prompt_locations: Vec::new(),
            mcp_server_names: Vec::new(),
            input_fields: BTreeMap::new(),
            input_field_metadata: BTreeMap::new(),
            input_field_locations: HashMap::new(),
            secrets_fields: BTreeMap::new(),
            secrets_field_metadata: BTreeMap::new(),
            secrets_field_locations: HashMap::new(),
            dynamic_fields: BTreeMap::new(),
            dynamic_field_metadata: BTreeMap::new(),
            dynamic_field_locations: HashMap::new(),
            agents: HashMap::new(),
            agent_dynamic_fields: HashMap::new(),
            agent_dynamic_field_metadata: HashMap::new(),
            agent_dynamic_field_locations: HashMap::new(),
            agent_output_field_locations: HashMap::new(),
            agent_for_loop_bindings: HashMap::new(),
            agent_for_loop_iterable_item_types: HashMap::new(),
            agent_names: Vec::new(),
            output_locations: Vec::new(),
            typed_declaration_locations: Vec::new(),
            agent_output_locations: Vec::new(),
            agent_locations: Vec::new(),
            has_input_declaration: false,
            has_secrets_declaration: false,
            has_output_declaration: false,
            tooling_snapshot,
            mcp_lock,
        };

        for declaration in workflow.declarations() {
            semantic_index.insert_declaration(declaration);
        }

        semantic_index.schema_names.sort();
        semantic_index.schema_names.dedup();

        semantic_index.agent_names.sort();
        semantic_index.agent_names.dedup();

        semantic_index.tool_names.sort();
        semantic_index.tool_names.dedup();

        semantic_index.resource_names.sort();
        semantic_index.resource_names.dedup();

        semantic_index.prompt_names.sort();
        semantic_index.prompt_names.dedup();

        semantic_index
    }

    fn insert_declaration(&mut self, declaration: &Declaration) {
        match declaration {
            Declaration::Provider(provider_declaration) => {
                self.insert_provider(provider_declaration);
            }
            Declaration::Model(model_declaration) => {
                self.insert_model(model_declaration);
            }
            Declaration::McpServer(mcp_server_declaration) => {
                self.mcp_server_names.push(mcp_server_declaration.name.clone());
            }
            Declaration::Schema(schema_declaration) => {
                self.insert_schema_declaration(schema_declaration);
            }
            Declaration::Input(input_declaration) => {
                self.insert_input_declaration(input_declaration);
            }
            Declaration::Secrets(secrets_declaration) => {
                self.insert_secrets_declaration(secrets_declaration);
            }
            Declaration::Agent(agent_declaration) => {
                self.insert_agent_declaration(agent_declaration);
            }
            Declaration::Tool(_) | Declaration::McpToolBatch(_) => {
                for tool_declaration in declaration.tool_declarations() {
                    self.insert_tool_declaration(tool_declaration);
                }
            }
            Declaration::McpResource(resource_import_declaration) => {
                self.insert_resource_import_declaration(resource_import_declaration);
            }
            Declaration::McpBatch(batch_import_declaration) => {
                for tool_declaration in declaration.tool_declarations() {
                    self.insert_tool_declaration(tool_declaration);
                }

                for resource_import_declaration in &batch_import_declaration.resources {
                    self.insert_resource_import_declaration(resource_import_declaration);
                }

                for prompt_import_declaration in &batch_import_declaration.prompts {
                    self.insert_prompt_import_declaration(prompt_import_declaration);
                }
            }
            Declaration::McpResourceBatch(resource_batch_import_declaration) => {
                for resource_import_declaration in &resource_batch_import_declaration.resources {
                    self.insert_resource_import_declaration(resource_import_declaration);
                }
            }
            Declaration::McpPrompt(prompt_import_declaration) => {
                self.insert_prompt_import_declaration(prompt_import_declaration);
            }
            Declaration::McpPromptBatch(prompt_batch_import_declaration) => {
                for prompt_import_declaration in &prompt_batch_import_declaration.prompts {
                    self.insert_prompt_import_declaration(prompt_import_declaration);
                }
            }
            Declaration::Dynamic(dynamic_block) => {
                self.insert_workflow_dynamic_block(dynamic_block);
            }
            Declaration::Output(output_declaration) => {
                self.has_output_declaration = true;
                self.output_locations.push(output_declaration.span);
            }
        }
    }

    fn insert_schema_declaration(&mut self, schema_declaration: &superwire_core::dsl::SchemaDeclaration) {
        self.insert_schema_field_locations(schema_declaration.name.as_str(), &schema_declaration.fields);

        let schema_fields = schema_declaration
            .fields
            .iter()
            .map(|typed_field| (typed_field.name.clone(), typed_field.field_type.clone()))
            .collect::<BTreeMap<_, _>>();

        let schema_field_metadata = typed_fields_to_metadata_map(&schema_declaration.fields);

        self.schemas.insert(
            schema_declaration.name.clone(),
            SchemaSummary {
                fields: schema_fields,
                field_metadata: schema_field_metadata,
            },
        );

        self.schema_names.push(schema_declaration.name.clone());
        self.schema_locations.push(NamedSpan {
            name: schema_declaration.name.clone(),
            span: schema_declaration.span,
        });
        self.typed_declaration_locations.push(schema_declaration.span);
    }

    fn insert_input_declaration(&mut self, input_declaration: &superwire_core::dsl::InputDeclaration) {
        self.has_input_declaration = true;

        if self.input_fields.is_empty() {
            self.input_fields = typed_fields_to_map(&input_declaration.fields);
            self.input_field_metadata = typed_fields_to_metadata_map(&input_declaration.fields);
            self.insert_singleton_field_locations(SingletonDeclarationKind::Input, &input_declaration.fields);
        }

        self.typed_declaration_locations.push(input_declaration.span);
    }

    fn insert_secrets_declaration(&mut self, secrets_declaration: &superwire_core::dsl::SecretsDeclaration) {
        self.has_secrets_declaration = true;

        if self.secrets_fields.is_empty() {
            self.secrets_fields = typed_fields_to_map(&secrets_declaration.fields);
            self.secrets_field_metadata = typed_fields_to_metadata_map(&secrets_declaration.fields);
            self.insert_singleton_field_locations(SingletonDeclarationKind::Secrets, &secrets_declaration.fields);
        }

        self.typed_declaration_locations.push(secrets_declaration.span);
    }

    fn insert_tool_declaration(&mut self, tool_declaration: &superwire_core::dsl::ToolDeclaration) {
        let (mcp_server_name, mcp_tool_name) = match &tool_declaration.source {
            Some(superwire_core::dsl::ToolSource::Mcp(mcp_source)) => (mcp_source.server_name.clone(), Some(mcp_source.tool_name.clone())),
            None => (None, None),
        };

        let output_type_expression = if tool_declaration.has_untyped_mcp_output() {
            None
        } else {
            Some(TypeExpression::Object(tool_declaration.output_fields.clone()))
        };

        self.tools.insert(
            tool_declaration.name.clone(),
            ToolSummary {
                description: tool_declaration.description.clone(),
                bounded_fields: typed_fields_to_map(&tool_declaration.binding_fields),
                bounded_field_metadata: typed_fields_to_metadata_map(&tool_declaration.binding_fields),
                output_type_expression,
                mcp_server_name,
                mcp_tool_name,
            },
        );

        self.tool_names.push(tool_declaration.name.clone());
        self.tool_locations.push(NamedSpan {
            name: tool_declaration.name.clone(),
            span: tool_declaration.span,
        });
        self.typed_declaration_locations.push(tool_declaration.span);
    }

    fn insert_resource_import_declaration(&mut self, resource_import_declaration: &superwire_core::dsl::McpResourceImportDeclaration) {
        self.resource_names.push(resource_import_declaration.name.clone());
        self.resource_locations.push(NamedSpan {
            name: resource_import_declaration.name.clone(),
            span: resource_import_declaration.span,
        });
    }

    fn insert_prompt_import_declaration(&mut self, prompt_import_declaration: &superwire_core::dsl::McpPromptImportDeclaration) {
        self.prompt_names.push(prompt_import_declaration.name.clone());
        self.prompt_locations.push(NamedSpan {
            name: prompt_import_declaration.name.clone(),
            span: prompt_import_declaration.span,
        });
    }

    fn insert_agent_declaration(&mut self, agent_declaration: &superwire_core::dsl::AgentDeclaration) {
        let mut agent_dynamic_fields = self.dynamic_fields.clone();
        let mut agent_dynamic_field_metadata = self.dynamic_field_metadata.clone();
        let mut agent_dynamic_field_locations = self.dynamic_field_locations.clone();

        for dynamic_block in agent_declaration.dynamic_blocks() {
            self.insert_dynamic_block_fields(
                dynamic_block,
                &mut agent_dynamic_fields,
                &mut agent_dynamic_field_metadata,
                &mut agent_dynamic_field_locations,
            );
        }

        for agent_property in &agent_declaration.properties {
            if let AgentProperty::Output { fields: _, span } = agent_property {
                self.typed_declaration_locations.push(*span);
                self.agent_output_locations.push(*span);
            }
        }

        let output_type_expression = agent_declaration.output_type();

        if let Some(output_type_expression) = &output_type_expression {
            self.insert_agent_output_field_locations(agent_declaration.name.as_str(), output_type_expression);
        }

        let output_type = output_type_expression;

        self.agents.insert(
            agent_declaration.name.clone(),
            AgentSummary {
                output_type: if agent_declaration.for_loop.is_some() {
                    output_type.map(|agent_output_type| TypeExpression::Array {
                        item_type: Box::new(agent_output_type),
                        fixed_length: None,
                    })
                } else {
                    output_type
                },
            },
        );

        self.agent_dynamic_fields
            .insert(agent_declaration.name.clone(), agent_dynamic_fields);
        self.agent_dynamic_field_metadata
            .insert(agent_declaration.name.clone(), agent_dynamic_field_metadata);
        self.agent_dynamic_field_locations
            .insert(agent_declaration.name.clone(), agent_dynamic_field_locations);

        if let Some(agent_for_loop) = &agent_declaration.for_loop {
            if let Some(iterable_item_type) = self.iterable_item_type(&agent_for_loop.iterable) {
                self.agent_for_loop_iterable_item_types
                    .insert(agent_declaration.name.clone(), iterable_item_type.clone());

                let for_loop_binding_types = self.for_loop_binding_types(agent_for_loop, iterable_item_type);

                if !for_loop_binding_types.is_empty() {
                    self.agent_for_loop_bindings
                        .insert(agent_declaration.name.clone(), for_loop_binding_types);
                }
            }
        }

        self.agent_names.push(agent_declaration.name.clone());
        self.agent_locations.push(NamedSpan {
            name: agent_declaration.name.clone(),
            span: agent_declaration.span,
        });
    }

    pub fn from_text_fallback(source_text: &str) -> Self {
        let tooling_snapshot = SemanticToolingSnapshot::from_source_tolerant(source_text);
        let mut semantic_index = Self::from_tooling_snapshot(&tooling_snapshot);

        semantic_index.has_input_declaration = semantic_index.has_input_declaration
            || Self::source_has_named_block_declaration(source_text, DeclarationKeyword::Input.as_str());
        semantic_index.has_secrets_declaration = semantic_index.has_secrets_declaration
            || Self::source_has_named_block_declaration(source_text, DeclarationKeyword::Secrets.as_str());
        semantic_index.has_output_declaration = semantic_index.has_output_declaration
            || Self::source_has_named_block_declaration(source_text, DeclarationKeyword::Output.as_str());
        semantic_index.mcp_server_names = Self::mcp_server_names_from_text(source_text);

        semantic_index
    }

    #[allow(clippy::too_many_lines)]
    fn from_tooling_snapshot(tooling_snapshot: &SemanticToolingSnapshot) -> Self {
        let providers = tooling_snapshot
            .declaration_index()
            .symbols_by_category(ToolingSymbolCategory::Provider)
            .map(|named_symbol_span| (named_symbol_span.name.clone(), ProviderSummary { driver: None }))
            .collect::<HashMap<_, _>>();

        let provider_locations = tooling_snapshot
            .declaration_index()
            .symbols_by_category(ToolingSymbolCategory::Provider)
            .map(|named_symbol_span| NamedSpan {
                name: named_symbol_span.name.clone(),
                span: named_symbol_span.span,
            })
            .collect::<Vec<_>>();

        let schemas = tooling_snapshot
            .schemas()
            .iter()
            .map(|(schema_name, schema_fields)| {
                (
                    schema_name.clone(),
                    SchemaSummary {
                        fields: schema_fields.clone(),
                        field_metadata: field_metadata_from_type_map(schema_fields),
                    },
                )
            })
            .collect::<HashMap<_, _>>();

        let mut schema_names = tooling_snapshot.schemas().keys().cloned().collect::<Vec<_>>();
        schema_names.sort();
        schema_names.dedup();

        let schema_locations = tooling_snapshot
            .declaration_index()
            .symbols_by_category(ToolingSymbolCategory::Schema)
            .map(|named_symbol_span| NamedSpan {
                name: named_symbol_span.name.clone(),
                span: named_symbol_span.span,
            })
            .collect::<Vec<_>>();

        let (tools, tool_names, tool_locations) = Self::tool_index_from_snapshot(tooling_snapshot);

        let agents = tooling_snapshot
            .agents()
            .iter()
            .map(|(agent_name, agent_output_type)| {
                (
                    agent_name.clone(),
                    AgentSummary {
                        output_type: agent_output_type.clone(),
                    },
                )
            })
            .collect::<HashMap<_, _>>();

        let mut agent_names = tooling_snapshot.agents().keys().cloned().collect::<Vec<_>>();
        agent_names.sort();
        agent_names.dedup();

        let agent_locations = tooling_snapshot
            .declaration_index()
            .symbols_by_category(ToolingSymbolCategory::Agent)
            .map(|named_symbol_span| NamedSpan {
                name: named_symbol_span.name.clone(),
                span: named_symbol_span.span,
            })
            .collect::<Vec<_>>();

        Self {
            providers,
            provider_locations,
            models: HashMap::new(),
            model_locations: Vec::new(),
            schemas,
            schema_names,
            schema_locations,
            schema_field_locations: HashMap::new(),
            tools,
            tool_names,
            tool_locations,
            resource_names: Vec::new(),
            resource_locations: Vec::new(),
            prompt_names: Vec::new(),
            prompt_locations: Vec::new(),
            mcp_server_names: Vec::new(),
            input_fields: tooling_snapshot.input_fields().clone(),
            input_field_metadata: field_metadata_from_type_map(tooling_snapshot.input_fields()),
            input_field_locations: HashMap::new(),
            secrets_fields: tooling_snapshot.secrets_fields().clone(),
            secrets_field_metadata: field_metadata_from_type_map(tooling_snapshot.secrets_fields()),
            secrets_field_locations: HashMap::new(),
            dynamic_fields: BTreeMap::new(),
            dynamic_field_metadata: BTreeMap::new(),
            dynamic_field_locations: HashMap::new(),
            agents,
            agent_dynamic_fields: HashMap::new(),
            agent_dynamic_field_metadata: HashMap::new(),
            agent_dynamic_field_locations: HashMap::new(),
            agent_output_field_locations: HashMap::new(),
            agent_for_loop_bindings: HashMap::new(),
            agent_for_loop_iterable_item_types: HashMap::new(),
            agent_names,
            output_locations: Vec::new(),
            typed_declaration_locations: Vec::new(),
            agent_output_locations: Vec::new(),
            agent_locations,
            has_input_declaration: !tooling_snapshot.input_fields().is_empty(),
            has_secrets_declaration: !tooling_snapshot.secrets_fields().is_empty(),
            has_output_declaration: false,
            tooling_snapshot: tooling_snapshot.clone(),
            mcp_lock: None,
        }
    }

    fn tool_index_from_snapshot(tooling_snapshot: &SemanticToolingSnapshot) -> (HashMap<String, ToolSummary>, Vec<String>, Vec<NamedSpan>) {
        let tools = tooling_snapshot
            .tools()
            .iter()
            .map(|(tool_name, tool_schema_summary)| {
                let (mcp_server_name, mcp_tool_name) = match &tool_schema_summary.source {
                    Some(superwire_core::dsl::ToolSource::Mcp(mcp_source)) => {
                        (mcp_source.server_name.clone(), Some(mcp_source.tool_name.clone()))
                    }
                    None => (None, None),
                };

                (
                    tool_name.clone(),
                    ToolSummary {
                        description: tool_schema_summary.description.clone(),
                        bounded_fields: tool_schema_summary.bounded_fields.clone(),
                        bounded_field_metadata: field_metadata_from_type_map(&tool_schema_summary.bounded_fields),
                        output_type_expression: None,
                        mcp_server_name,
                        mcp_tool_name,
                    },
                )
            })
            .collect::<HashMap<_, _>>();

        let mut tool_names = tooling_snapshot.tools().keys().cloned().collect::<Vec<_>>();
        tool_names.sort();
        tool_names.dedup();

        let tool_locations = tooling_snapshot
            .declaration_index()
            .symbols_by_category(ToolingSymbolCategory::Tool)
            .map(|named_symbol_span| NamedSpan {
                name: named_symbol_span.name.clone(),
                span: named_symbol_span.span,
            })
            .collect::<Vec<_>>();

        (tools, tool_names, tool_locations)
    }

    fn mcp_server_names_from_text(source_text: &str) -> Vec<String> {
        let mut server_names = source_text.lines().filter_map(Self::mcp_server_name_from_line).collect::<Vec<_>>();

        server_names.sort();
        server_names.dedup();

        server_names
    }

    fn mcp_server_name_from_line(source_line: &str) -> Option<String> {
        let trimmed_source_line = source_line.trim_start();
        let declaration_keyword = DeclarationKeyword::Mcp.as_str();
        let after_declaration_keyword = trimmed_source_line.strip_prefix(declaration_keyword)?;

        if !after_declaration_keyword.starts_with(char::is_whitespace) {
            return None;
        }

        let server_name = after_declaration_keyword
            .trim_start()
            .chars()
            .take_while(|character| character.is_ascii_alphanumeric() || *character == '_')
            .collect::<String>();

        if server_name.is_empty() {
            return None;
        }

        Some(server_name)
    }

    fn source_has_named_block_declaration(source_text: &str, declaration_keyword: &str) -> bool {
        for source_line in source_text.lines() {
            let trimmed_line = source_line.trim_start();
            let Some(line_after_keyword) = trimmed_line.strip_prefix(declaration_keyword) else {
                continue;
            };

            if !line_after_keyword.starts_with(char::is_whitespace) {
                continue;
            }

            if line_after_keyword.trim_start().starts_with('{') {
                return true;
            }
        }

        false
    }

    fn insert_schema_field_locations(&mut self, schema_name: &str, typed_fields: &[TypedField]) {
        Self::insert_field_locations(
            &mut self.schema_field_locations,
            Self::schema_field_location_prefix(schema_name),
            typed_fields,
        );
    }

    fn insert_singleton_field_locations(&mut self, singleton_kind: SingletonDeclarationKind, typed_fields: &[TypedField]) {
        match singleton_kind {
            SingletonDeclarationKind::Input => {
                Self::insert_field_locations(&mut self.input_field_locations, Vec::new(), typed_fields);
            }
            SingletonDeclarationKind::Secrets => {
                Self::insert_field_locations(&mut self.secrets_field_locations, Vec::new(), typed_fields);
            }
            SingletonDeclarationKind::Output => {}
        }
    }

    fn insert_agent_output_field_locations(&mut self, agent_name: &str, output_type_expression: &TypeExpression) {
        let TypeExpression::Object(typed_fields) = output_type_expression else {
            return;
        };

        Self::insert_field_locations(
            &mut self.agent_output_field_locations,
            Self::agent_field_location_prefix(agent_name),
            typed_fields,
        );
    }

    fn insert_workflow_dynamic_block(&mut self, dynamic_block: &superwire_core::dsl::DynamicBlock) {
        let mut dynamic_fields = self.dynamic_fields.clone();
        let mut dynamic_field_metadata = self.dynamic_field_metadata.clone();
        let mut dynamic_field_locations = self.dynamic_field_locations.clone();

        self.insert_dynamic_block_fields(
            dynamic_block,
            &mut dynamic_fields,
            &mut dynamic_field_metadata,
            &mut dynamic_field_locations,
        );

        self.dynamic_fields = dynamic_fields;
        self.dynamic_field_metadata = dynamic_field_metadata;
        self.dynamic_field_locations = dynamic_field_locations;
    }

    fn insert_dynamic_block_fields(
        &self,
        dynamic_block: &superwire_core::dsl::DynamicBlock,
        dynamic_fields: &mut BTreeMap<String, TypeExpression>,
        dynamic_field_metadata: &mut BTreeMap<String, FieldMetadata>,
        dynamic_field_locations: &mut HashMap<String, SourceSpan>,
    ) {
        let mut pending_dynamic_fields = dynamic_block.fields.iter().collect::<Vec<_>>();

        while !pending_dynamic_fields.is_empty() {
            let pending_count_before_pass = pending_dynamic_fields.len();

            pending_dynamic_fields.retain(|dynamic_field| {
                let Some(dynamic_field_type) = self.expression_type_with_dynamic_scope(&dynamic_field.value, dynamic_fields) else {
                    return true;
                };

                dynamic_fields.insert(dynamic_field.name.clone(), dynamic_field_type.clone());
                dynamic_field_metadata.insert(
                    dynamic_field.name.clone(),
                    FieldMetadata {
                        field_type: dynamic_field_type,
                        description: None,
                    },
                );
                dynamic_field_locations.insert(dynamic_field.name.clone(), dynamic_field.span);

                false
            });

            if pending_dynamic_fields.len() == pending_count_before_pass {
                break;
            }
        }
    }

    fn insert_field_locations(
        field_locations: &mut HashMap<String, SourceSpan>,
        field_prefix_segments: Vec<String>,
        typed_fields: &[TypedField],
    ) {
        for typed_field in typed_fields {
            let mut field_path_segments = field_prefix_segments.clone();
            field_path_segments.push(typed_field.name.clone());

            let field_location_key = Self::field_location_key(field_path_segments.as_slice());
            field_locations.insert(field_location_key, typed_field.span);

            if let TypeExpression::Object(nested_typed_fields) = &typed_field.field_type {
                Self::insert_field_locations(field_locations, field_path_segments, nested_typed_fields);
            }
        }
    }

    fn field_location_key(field_path_segments: &[String]) -> String {
        field_path_segments.join(".")
    }

    fn schema_field_location_prefix(schema_name: &str) -> Vec<String> {
        vec![schema_name.to_string()]
    }

    fn agent_field_location_prefix(agent_name: &str) -> Vec<String> {
        vec![agent_name.to_string()]
    }

    fn insert_provider(&mut self, provider_declaration: &ProviderDeclaration) {
        let provider_driver = ProviderDriver::parse(&provider_declaration.driver_name);

        self.providers
            .insert(provider_declaration.name.clone(), ProviderSummary { driver: provider_driver });

        self.provider_locations.push(NamedSpan {
            name: provider_declaration.name.clone(),
            span: provider_declaration.span,
        });
    }

    fn insert_model(&mut self, model_declaration: &ModelDeclaration) {
        self.models.insert(
            model_declaration.name.clone(),
            ModelSummary {
                provider_name: model_declaration.provider_name.clone(),
                model_identifier: model_declaration.id_literal().map(str::to_string),
            },
        );

        self.model_locations.push(NamedSpan {
            name: model_declaration.name.clone(),
            span: model_declaration.span,
        });
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

    pub fn type_suggestions(&self, line_prefix: &str, current_schema_name: Option<&str>) -> Vec<CompletionSuggestion> {
        let trimmed_line_prefix = line_prefix.trim_end();

        if let Some(reference_completion_path) = ReferenceCompletionPath::from_line_prefix(line_prefix) {
            if reference_completion_path.is_schema_root() {
                return self.schema_type_reference_suggestions(&reference_completion_path, current_schema_name);
            }
        }

        if trimmed_line_prefix.ends_with('.') && !trimmed_line_prefix.ends_with("schema.") {
            return Vec::new();
        }

        if trimmed_line_prefix.ends_with("schema.") {
            return self
                .schema_names
                .iter()
                .filter(|schema_name| current_schema_name.is_none_or(|current_name| *schema_name != current_name))
                .map(|schema_name| CompletionSuggestion {
                    label: schema_name.clone(),
                    kind: CompletionItemKind::STRUCT,
                    detail: "Named schema".to_string(),
                    documentation: "Type declared in a `schema` block.".to_string(),
                    insert_text: schema_name.clone(),
                })
                .collect();
        }

        let prefix = trailing_identifier(line_prefix).unwrap_or_default();
        let mut completion_suggestions = type_symbol_suggestions()
            .into_iter()
            .filter(|completion_suggestion| completion_suggestion.label.starts_with(prefix))
            .collect::<Vec<_>>();

        completion_suggestions.extend(self.structural_type_suggestions(prefix));

        completion_suggestions.extend(
            self.schema_names
                .iter()
                .filter(|schema_name| {
                    if current_schema_name == Some(schema_name.as_str()) {
                        return false;
                    }

                    let schema_reference = format!("schema.{schema_name}");
                    schema_reference.starts_with(prefix)
                })
                .map(|schema_name| CompletionSuggestion {
                    label: format!("schema.{schema_name}"),
                    kind: CompletionItemKind::STRUCT,
                    detail: "Named schema reference".to_string(),
                    documentation: "Reference a named schema type.".to_string(),
                    insert_text: format!("schema.{schema_name}"),
                }),
        );

        completion_suggestions
    }

    fn schema_type_reference_suggestions(
        &self,
        reference_completion_path: &ReferenceCompletionPath,
        current_schema_name: Option<&str>,
    ) -> Vec<CompletionSuggestion> {
        if reference_completion_path.complete_accesses.is_empty() {
            return self
                .schema_names
                .iter()
                .filter(|schema_name| current_schema_name.is_none_or(|current_name| *schema_name != current_name))
                .filter(|schema_name| schema_name.starts_with(&reference_completion_path.pending_prefix))
                .map(|schema_name| CompletionSuggestion {
                    label: schema_name.clone(),
                    kind: CompletionItemKind::STRUCT,
                    detail: "Named schema".to_string(),
                    documentation: "Type declared in a `schema` block.".to_string(),
                    insert_text: schema_name.clone(),
                })
                .collect();
        }

        let schema_name = &reference_completion_path.complete_accesses[0];

        if current_schema_name == Some(schema_name.as_str()) {
            return Vec::new();
        }

        let remaining_accesses = reference_completion_path.complete_accesses[1..].to_vec();
        let candidate_types = self
            .tooling_snapshot
            .resolve_reference_path_types(&ToolingReferencePath::schema(schema_name.clone(), remaining_accesses));
        let mut available_fields = BTreeMap::<String, TypeExpression>::new();

        for candidate_type in candidate_types {
            self.collect_type_fields(&candidate_type, &mut available_fields);
        }

        available_fields
            .into_iter()
            .filter(|(field_name, field_type)| {
                field_name.starts_with(&reference_completion_path.pending_prefix) && field_type.is_string_enum_expression()
            })
            .map(|(field_name, field_type)| CompletionSuggestion {
                label: field_name.clone(),
                kind: CompletionItemKind::PROPERTY,
                detail: format!("Enum field: {}", field_type.render_type()),
                documentation: "Schema field usable as an enum reference.".to_string(),
                insert_text: field_name,
            })
            .collect()
    }

    fn collect_type_fields(&self, candidate_type: &TypeExpression, available_fields: &mut BTreeMap<String, TypeExpression>) {
        match candidate_type {
            TypeExpression::Object(typed_fields) => {
                for typed_field in typed_fields {
                    available_fields
                        .entry(typed_field.name.clone())
                        .or_insert_with(|| typed_field.field_type.clone());
                }
            }
            TypeExpression::SchemaReference(schema_name) => {
                if let Some(schema_summary) = self.schemas.get(schema_name) {
                    for (field_name, field_type) in &schema_summary.fields {
                        available_fields.entry(field_name.clone()).or_insert_with(|| field_type.clone());
                    }
                }
            }
            TypeExpression::Variant { discriminator, cases } => {
                available_fields.entry(discriminator.clone()).or_insert_with(|| {
                    TypeExpression::Union(
                        cases
                            .iter()
                            .map(|variant_case| TypeExpression::StringEnum(variant_case.name.clone()))
                            .collect(),
                    )
                });
            }
            TypeExpression::Union(type_expressions) => {
                for type_expression in type_expressions {
                    self.collect_type_fields(type_expression, available_fields);
                }
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
            | TypeExpression::Tuple(_) => {}
        }
    }

    fn structural_type_suggestions(&self, type_prefix: &str) -> Vec<CompletionSuggestion> {
        let structural_type_suggestions = [
            CompletionSuggestion {
                label: "[string]".to_string(),
                kind: CompletionItemKind::STRUCT,
                detail: "Array type".to_string(),
                documentation: "Array type expression.".to_string(),
                insert_text: "[string]".to_string(),
            },
            CompletionSuggestion {
                label: "{}".to_string(),
                kind: CompletionItemKind::STRUCT,
                detail: "Object type".to_string(),
                documentation: "Object type expression.".to_string(),
                insert_text: "{}".to_string(),
            },
        ];

        structural_type_suggestions
            .into_iter()
            .filter(|completion_suggestion| completion_suggestion.label.starts_with(type_prefix))
            .collect()
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

    fn for_loop_binding_types(
        &self,
        agent_for_loop: &superwire_core::dsl::AgentForLoop,
        iterable_item_type: TypeExpression,
    ) -> BTreeMap<String, Vec<TypeExpression>> {
        let mut binding_types = BTreeMap::new();

        match &agent_for_loop.pattern {
            AgentForLoopPattern::Identifier(identifier) => {
                binding_types.insert(identifier.clone(), vec![iterable_item_type]);
            }
            AgentForLoopPattern::ObjectDestructuring(field_names) => {
                for field_name in field_names {
                    let resolved_field_types = self
                        .tooling_snapshot
                        .resolve_access_path_types(vec![iterable_item_type.clone()], std::slice::from_ref(field_name));

                    if resolved_field_types.is_empty() {
                        continue;
                    }

                    binding_types.insert(field_name.clone(), resolved_field_types);
                }
            }
        }

        binding_types
    }

    fn iterable_item_type(&self, iterable_expression: &Expression) -> Option<TypeExpression> {
        let iterable_type = self.expression_type(iterable_expression)?;

        match iterable_type {
            TypeExpression::Array {
                item_type,
                fixed_length: _,
            } => Some(*item_type),
            TypeExpression::Tuple(tuple_member_types) => Some(TypeExpression::Union(tuple_member_types)),
            TypeExpression::String
            | TypeExpression::Number
            | TypeExpression::Float
            | TypeExpression::Boolean
            | TypeExpression::Null
            | TypeExpression::AnyObject
            | TypeExpression::Object(_)
            | TypeExpression::SchemaReference(_)
            | TypeExpression::StringEnum(_)
            | TypeExpression::StringEnumReference(_)
            | TypeExpression::Variant {
                discriminator: _,
                cases: _,
            }
            | TypeExpression::Union(_) => None,
        }
    }

    fn expression_type(&self, expression: &Expression) -> Option<TypeExpression> {
        self.expression_type_with_dynamic_scope(expression, &self.dynamic_fields)
    }

    fn expression_type_with_dynamic_scope(
        &self,
        expression: &Expression,
        dynamic_fields: &BTreeMap<String, TypeExpression>,
    ) -> Option<TypeExpression> {
        match expression {
            Expression::StringLiteral(_) | Expression::StringTemplate(_) => Some(TypeExpression::String),
            Expression::NumberLiteral(number_literal) => {
                if number_literal.contains('.') {
                    return Some(TypeExpression::Float);
                }

                Some(TypeExpression::Number)
            }
            Expression::BooleanLiteral(_) => Some(TypeExpression::Boolean),
            Expression::NullLiteral => Some(TypeExpression::Null),
            Expression::Reference(reference) => self.reference_expression_type(reference, dynamic_fields),
            Expression::FunctionCall(_) => None,
            Expression::McpCall(_) => Some(TypeExpression::String),
            Expression::NullFallback(null_fallback) => self.expression_type_with_dynamic_scope(&null_fallback.value, dynamic_fields),
            Expression::VariantProjection(_) | Expression::Match(_) => None,
            Expression::ToolCall(tool_call) => {
                let tool_name = tool_call.callee.first_access_field()?;
                let tool_summary = self.tools.get(tool_name)?;

                tool_summary.output_type_expression.clone()
            }
            Expression::ArrayLiteral(array_items) => {
                let mut array_item_types = array_items
                    .iter()
                    .filter_map(|array_item| self.expression_type_with_dynamic_scope(array_item, dynamic_fields))
                    .collect::<Vec<_>>();

                if array_item_types.is_empty() {
                    return None;
                }

                if array_item_types.len() == 1 {
                    return Some(TypeExpression::Array {
                        item_type: Box::new(array_item_types.remove(0)),
                        fixed_length: None,
                    });
                }

                Some(TypeExpression::Array {
                    item_type: Box::new(TypeExpression::Union(array_item_types)),
                    fixed_length: None,
                })
            }
            Expression::ObjectLiteral(object_fields) => {
                let typed_fields = object_fields
                    .iter()
                    .filter_map(|object_field| {
                        let field_type = self.expression_type_with_dynamic_scope(&object_field.value, dynamic_fields)?;

                        Some(TypedField {
                            name: object_field.name.clone(),
                            field_type,
                            description: None,
                            span: object_field.span,
                        })
                    })
                    .collect::<Vec<_>>();

                Some(TypeExpression::Object(typed_fields))
            }
        }
    }

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

    fn reference_expression_type(
        &self,
        reference: &superwire_core::dsl::Reference,
        dynamic_fields: &BTreeMap<String, TypeExpression>,
    ) -> Option<TypeExpression> {
        let reference_keyword = reference.root_keyword()?;
        let reference_accesses = reference
            .accesses
            .iter()
            .map(|reference_access| reference_access.field.clone())
            .collect::<Vec<_>>();

        match reference_keyword {
            ReferenceKeyword::Dynamic => self.resolve_singleton_reference_type(dynamic_fields, &reference_accesses),
            ReferenceKeyword::Input => self.resolve_singleton_reference_type(&self.input_fields, &reference_accesses),
            ReferenceKeyword::Secrets => self.resolve_singleton_reference_type(&self.secrets_fields, &reference_accesses),
            ReferenceKeyword::Agent => {
                let agent_name = reference_accesses.first()?;
                let agent_output_type = self.agents.get(agent_name)?.output_type.clone()?;

                if reference_accesses.len() == 1 {
                    return Some(agent_output_type);
                }

                let candidate_types = self
                    .tooling_snapshot
                    .resolve_access_path_types(vec![agent_output_type], &reference_accesses[1..]);

                candidate_types.first().cloned()
            }
            ReferenceKeyword::Model | ReferenceKeyword::Tool | ReferenceKeyword::Resource | ReferenceKeyword::Prompt => None,
        }
    }
}

fn typed_fields_to_map(typed_fields: &[TypedField]) -> BTreeMap<String, TypeExpression> {
    typed_fields
        .iter()
        .map(|typed_field| (typed_field.name.clone(), typed_field.field_type.clone()))
        .collect()
}

fn typed_fields_to_metadata_map(typed_fields: &[TypedField]) -> BTreeMap<String, FieldMetadata> {
    typed_fields
        .iter()
        .map(|typed_field| {
            (
                typed_field.name.clone(),
                FieldMetadata {
                    field_type: typed_field.field_type.clone(),
                    description: typed_field.description.clone(),
                },
            )
        })
        .collect()
}

fn field_metadata_from_type_map(type_map: &BTreeMap<String, TypeExpression>) -> BTreeMap<String, FieldMetadata> {
    type_map
        .iter()
        .map(|(field_name, field_type)| {
            (
                field_name.clone(),
                FieldMetadata {
                    field_type: field_type.clone(),
                    description: None,
                },
            )
        })
        .collect()
}
