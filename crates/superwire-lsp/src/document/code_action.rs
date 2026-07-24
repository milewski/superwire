use superwire_dsl::{ToolPropertyName, TypedField};

use lsp_types::{Position, Range};

use super::{CodeActionEdit, CodeActionSuggestion, DocumentState, RenderTypeExpression};

impl DocumentState {
    #[must_use]
    pub fn code_actions(&self, position: Position) -> Vec<CodeActionSuggestion> {
        let semantic_index = self.semantic_index_for_completion(position);
        let property_name = self
            .tool_schema_property_name_at_position(position)
            .unwrap_or(ToolPropertyName::Output);

        let Some(mcp_tool_schema_source) = self.mcp_tool_schema_source_at_position(position, &semantic_index) else {
            return self.schema_completion_code_actions(position, property_name);
        };
        let schema_block = self.current_tool_schema_block(position);
        let schema_fields = match mcp_tool_schema_source {
            super::completion::McpToolSchemaSource::LocalTool(tool_name) => {
                semantic_index.mcp_tool_schema_fields(&tool_name, property_name)
            }
            super::completion::McpToolSchemaSource::McpTool { server_name, tool_name } => {
                semantic_index.mcp_tool_schema_fields_for_source(server_name.as_deref(), &tool_name, property_name)
            }
            super::completion::McpToolSchemaSource::McpToolBatch { server_name, tool_names } => {
                semantic_index.mcp_tool_batch_common_schema_fields(&server_name, &tool_names, property_name)
            }
        };

        if schema_fields.is_empty() {
            return self.schema_completion_code_actions(position, property_name);
        }

        let edit_range = schema_block.as_ref().map_or(
            Range {
                start: position,
                end: position,
            },
            |schema_block| schema_block.content_range,
        );
        let indent = schema_block.as_ref().map_or_else(
            || self.line_indent_at_position(position),
            |schema_block| schema_block.indent.clone(),
        );

        vec![CodeActionSuggestion {
            title: format!("Fill {} schema from MCP lock", property_name.as_str()),
            edit: CodeActionEdit {
                range: edit_range,
                new_text: Self::render_schema_block_contents(&schema_fields, indent.as_str()),
            },
        }]
    }

    fn schema_completion_code_actions(&self, position: Position, property_name: ToolPropertyName) -> Vec<CodeActionSuggestion> {
        let schema_lines = self
            .completion_suggestions(position)
            .into_iter()
            .filter(|completion_suggestion| completion_suggestion.insert_text.contains(':'))
            .map(|completion_suggestion| completion_suggestion.insert_text)
            .collect::<Vec<_>>();

        if schema_lines.is_empty() {
            return Vec::new();
        }

        let indent = self.line_indent_at_position(position);
        let field_indent = format!("{indent}    ");
        let new_text = schema_lines
            .into_iter()
            .map(|schema_line| format!("{field_indent}{schema_line}"))
            .collect::<Vec<_>>()
            .join("\n");

        vec![CodeActionSuggestion {
            title: format!("Fill {} schema from MCP lock", property_name.as_str()),
            edit: CodeActionEdit {
                range: Range {
                    start: position,
                    end: position,
                },
                new_text,
            },
        }]
    }

    fn current_tool_schema_block(&self, position: Position) -> Option<ToolSchemaBlock> {
        let cursor_offset = self.byte_offset(position)?;
        let source_prefix = &self.text[..cursor_offset];

        [ToolPropertyName::Input, ToolPropertyName::Bindings, ToolPropertyName::Output]
            .into_iter()
            .filter_map(|property_name| self.tool_schema_block_for_property(source_prefix, property_name))
            .max_by_key(|schema_block| schema_block.open_brace_offset)
    }

    fn tool_schema_block_for_property(&self, source_prefix: &str, property_name: ToolPropertyName) -> Option<ToolSchemaBlock> {
        let property_name_text = property_name.as_str();
        let property_name_offset = source_prefix.rfind(property_name_text)?;

        if !Self::is_keyword_boundary(source_prefix, property_name_offset, property_name_text.len()) {
            return None;
        }

        let after_property_name = &source_prefix[property_name_offset + property_name_text.len()..];
        let open_brace_relative_offset = after_property_name.find('{')?;
        let open_brace_offset = property_name_offset + property_name_text.len() + open_brace_relative_offset;

        if Self::block_balance(&source_prefix[open_brace_offset..]) <= 0 {
            return None;
        }

        let content_start_offset = open_brace_offset + 1;
        let content_start_position = self.position_for_byte_offset(content_start_offset)?;
        let content_end_position = self.position_for_byte_offset(source_prefix.len())?;
        let indent = self.line_indent_at_position(content_start_position);

        Some(ToolSchemaBlock {
            open_brace_offset,
            content_range: Range {
                start: content_start_position,
                end: content_end_position,
            },
            indent,
        })
    }

    fn render_schema_block_contents(schema_fields: &[TypedField], indent: &str) -> String {
        let field_indent = format!("{indent}    ");
        let rendered_fields = schema_fields
            .iter()
            .map(|typed_field| {
                format!(
                    "{field_indent}{}: {}",
                    typed_field.name,
                    typed_field.field_type.render_type_expanded(field_indent.as_str())
                )
            })
            .collect::<Vec<_>>()
            .join("\n");

        format!("\n{rendered_fields}\n{indent}")
    }

    fn line_indent_at_position(&self, position: Position) -> String {
        self.text
            .lines()
            .nth(position.line as usize)
            .unwrap_or_default()
            .chars()
            .take_while(|character| character.is_whitespace())
            .collect()
    }
}

struct ToolSchemaBlock {
    open_brace_offset: usize,
    content_range: Range,
    indent: String,
}
