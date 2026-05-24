use lsp_types::{CompletionItemKind, Position};
use superwire_dsl::{ReferenceKeyword, ToolCallKeyword, ToolPropertyName, TypeExpression, TypedField};

use super::super::position::byte_offset_for_position;
use super::super::scope::tool_property_scope_suggestions;
use super::super::semantic_index::SemanticIndex;
use super::super::text_utils::trailing_identifier;
use super::super::{CompletionSuggestion, DocumentState, RenderTypeExpression};
use super::McpToolSchemaSource;

pub(super) struct ToolCallBindingCompletionContext {
    pub(super) tool_name: String,
    pub(super) binding_prefix: String,
    pub(super) existing_binding_names: Vec<String>,
}

impl DocumentState {
    pub(in crate::document) fn tool_schema_property_name_at_position(&self, position: Position) -> Option<ToolPropertyName> {
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

    pub(super) fn tool_call_binding_completion_context(
        &self,
        position: Position,
        line_prefix: &str,
    ) -> Option<ToolCallBindingCompletionContext> {
        if line_prefix.contains(':') {
            return None;
        }

        let cursor_offset = byte_offset_for_position(&self.text, position)?;
        let source_prefix = &self.text[..cursor_offset];
        let bindings_keyword = ToolPropertyName::Bindings.as_str();
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

    pub(super) fn tool_property_suggestions(
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
                            kind: CompletionItemKind::PROPERTY,
                            detail: "Tool declaration property".to_string(),
                            documentation: "Property available inside a `tool` declaration.".to_string(),
                            insert_text: property_name.as_str().to_string(),
                        });
                }

                CompletionSuggestion {
                    label: property_name.as_str().to_string(),
                    kind: CompletionItemKind::PROPERTY,
                    detail: "MCP schema property".to_string(),
                    documentation: format!("Insert `{}` with fields discovered from the MCP lock file.", property_name.as_str()),
                    insert_text: Self::render_schema_property_snippet(property_name, &schema_fields, line_prefix),
                }
            })
            .collect()
    }

    fn render_schema_property_snippet(property_name: ToolPropertyName, schema_fields: &[TypedField], line_prefix: &str) -> String {
        let property_indent = line_prefix
            .chars()
            .take_while(|character| character.is_whitespace())
            .collect::<String>();
        let field_indent = format!("{property_indent}    ");
        let rendered_fields = schema_fields
            .iter()
            .map(|typed_field| {
                let rendered_description = typed_field
                    .description
                    .as_ref()
                    .map(|description_text| {
                        description_text
                            .lines()
                            .map(|description_line| format!("{field_indent}/// {description_line}"))
                            .collect::<Vec<_>>()
                            .join("\n")
                    })
                    .unwrap_or_default();

                let rendered_field = if property_name == ToolPropertyName::Bindings {
                    format!("{field_indent}{}: $1", typed_field.name)
                } else {
                    let rendered_field_type = typed_field.field_type.render_type_expanded(field_indent.as_str());
                    let normalized_rendered_field_type =
                        Self::normalize_rendered_field_type_snippet(&typed_field.field_type, &rendered_field_type, field_indent.as_str());

                    format!("{field_indent}{}: {}", typed_field.name, normalized_rendered_field_type)
                };

                if rendered_description.is_empty() {
                    return rendered_field;
                }

                format!("{rendered_description}\n{rendered_field}")
            })
            .collect::<Vec<_>>()
            .join("\n");

        format!("{} {{\n{rendered_fields}\n{property_indent}}}", property_name.as_str())
    }

    fn normalize_rendered_field_type_snippet(field_type: &TypeExpression, rendered_field_type: &str, field_indent: &str) -> String {
        if !matches!(field_type, TypeExpression::Object(_)) {
            return rendered_field_type.to_string();
        }

        let rendered_prefix = format!("{field_indent}{{");

        if let Some(stripped_rendered_field_type) = rendered_field_type.strip_prefix(rendered_prefix.as_str()) {
            return format!("{{{stripped_rendered_field_type}");
        }

        rendered_field_type.to_string()
    }
}
