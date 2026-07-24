use lsp_types::{CompletionItemKind, Position};
use superwire_dsl::{DeclarationKeyword, ImportKeyword, ToolPropertyName};
use superwire_mcp::McpServerLock;

use super::super::semantic_index::SemanticIndex;
use super::super::text_utils::trailing_identifier;
use super::super::{CompletionSuggestion, DocumentState, RenderTypeExpression};

pub(in crate::document) enum McpToolSchemaSource {
    LocalTool(String),
    McpTool { server_name: Option<String>, tool_name: String },
    McpToolBatch { server_name: String, tool_names: Vec<String> },
}

struct McpBatchItemCompletionContext {
    server_name: String,
    item_kind: DeclarationKeyword,
    item_prefix: String,
    existing_item_names: Vec<String>,
}

pub(super) struct PromptImportBindingCompletionContext {
    pub(super) server_name: String,
    pub(super) prompt_name: String,
    pub(super) binding_prefix: String,
    pub(super) existing_binding_names: Vec<String>,
}

impl DocumentState {
    pub(super) fn mcp_tool_schema_field_suggestions(
        &self,
        position: Position,
        line_prefix: &str,
        semantic_index: &SemanticIndex,
    ) -> Option<Vec<CompletionSuggestion>> {
        self.tool_schema_property_name_at_position(position)?;
        let mcp_tool_schema_source = self.mcp_tool_schema_source_at_position(position, semantic_index)?;

        Some(self.mcp_tool_schema_field_suggestions_for_source(semantic_index, position, line_prefix.trim_start(), mcp_tool_schema_source))
    }

    pub(super) fn mcp_tool_schema_field_suggestions_for_source(
        &self,
        semantic_index: &SemanticIndex,
        position: Position,
        trimmed_prefix: &str,
        mcp_tool_schema_source: McpToolSchemaSource,
    ) -> Vec<CompletionSuggestion> {
        let existing_fields = self.existing_typed_field_names(position);
        let field_prefix = super::super::text_utils::trailing_identifier(trimmed_prefix).unwrap_or_default();
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
        typed_field: &superwire_dsl::TypedField,
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
            kind: CompletionItemKind::PROPERTY,
            detail: typed_field.description.clone().unwrap_or_else(|| rendered_type.clone()),
            documentation: typed_field
                .description
                .clone()
                .unwrap_or_else(|| format!("MCP tool {} field of type `{rendered_type}`.", tool_property_name.as_str())),
            insert_text,
        }
    }

    pub(in crate::document) fn mcp_tool_schema_source_at_position(
        &self,
        position: Position,
        semantic_index: &SemanticIndex,
    ) -> Option<McpToolSchemaSource> {
        if let Some(tool_name) = semantic_index.tool_name_at_position(self.position_context(position)?) {
            return Some(McpToolSchemaSource::LocalTool(tool_name.to_string()));
        }

        let mcp_tool_batch_schema_source = self.mcp_tool_batch_schema_source_at_position(position);
        let cursor_offset = self.byte_offset(position)?;
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

        match import_segments.next() {
            Some(import_kind) if import_kind == DeclarationKeyword::Tool.as_str() && import_segments.next().is_none() => {}
            None => {}
            _ => return None,
        }

        Some(McpToolSchemaSource::McpTool {
            server_name: Some(server_name),
            tool_name,
        })
    }

    fn mcp_tool_batch_schema_source_at_position(&self, position: Position) -> Option<McpToolSchemaSource> {
        let cursor_offset = self.byte_offset(position)?;
        let workflow = self.semantic_snapshot.workflow_document().workflow()?;

        workflow.declarations().iter().find_map(|declaration| {
            let (server_name, span, tool_names) = match declaration {
                superwire_dsl::Declaration::McpToolBatch(mcp_tool_batch) => (
                    mcp_tool_batch.server_name.clone(),
                    mcp_tool_batch.span,
                    mcp_tool_batch.items.iter().map(|item| item.source_name.clone()).collect(),
                ),
                superwire_dsl::Declaration::McpBatch(mcp_batch) => (
                    mcp_batch.server_name.clone(),
                    mcp_batch.span,
                    mcp_batch.tool_items.iter().map(|item| item.source_name.clone()).collect(),
                ),
                _ => return None,
            };
            let span_range = span.to_byte_range(&self.text)?;

            if !span_range.contains(&cursor_offset) {
                return None;
            }

            Some(McpToolSchemaSource::McpToolBatch { server_name, tool_names })
        })
    }

    fn mcp_batch_item_completion_context(&self, position: Position) -> Option<McpBatchItemCompletionContext> {
        let cursor_offset = self.byte_offset(position)?;
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
        let batch_block_prefix = &batch_prefix[open_brace_index + 1..];
        let (item_kind, item_prefix) = Self::mcp_batch_item_completion_target(batch_block_prefix)?;
        let existing_item_names = Self::existing_batch_item_names_from_block_prefix(batch_block_prefix, item_kind);

        match import_segments.next() {
            Some(segment) if segment == item_kind.as_str() && import_segments.next().is_none() => {}
            None => {}
            _ => return None,
        }

        Some(McpBatchItemCompletionContext {
            server_name,
            item_kind,
            item_prefix,
            existing_item_names,
        })
    }

    pub(super) fn mcp_tool_batch_item_suggestions(
        &self,
        semantic_index: &SemanticIndex,
        position: Position,
        _line_prefix: &str,
    ) -> Option<Vec<CompletionSuggestion>> {
        let batch_item_completion_context = self.mcp_batch_item_completion_context(position)?;

        match batch_item_completion_context.item_kind {
            DeclarationKeyword::Tool => Some(semantic_index.mcp_tool_batch_item_suggestions(
                &batch_item_completion_context.server_name,
                &batch_item_completion_context.item_prefix,
                &batch_item_completion_context.existing_item_names,
            )),
            DeclarationKeyword::Resource => Some(semantic_index.mcp_resource_batch_item_suggestions(
                &batch_item_completion_context.server_name,
                &batch_item_completion_context.item_prefix,
                &batch_item_completion_context.existing_item_names,
            )),
            DeclarationKeyword::Prompt => Some(semantic_index.mcp_prompt_batch_item_suggestions(
                &batch_item_completion_context.server_name,
                &batch_item_completion_context.item_prefix,
                &batch_item_completion_context.existing_item_names,
            )),
            _ => Some(Vec::new()),
        }
    }

    fn existing_batch_item_names_from_block_prefix(batch_block_prefix: &str, item_kind: DeclarationKeyword) -> Vec<String> {
        let item_keyword = item_kind.as_str();

        batch_block_prefix
            .lines()
            .filter_map(|source_line| {
                let trimmed_source_line = source_line.trim_start();
                let after_item_keyword = trimmed_source_line.strip_prefix(item_keyword)?.trim_start();
                let item_name = after_item_keyword
                    .chars()
                    .take_while(|character| character.is_ascii_alphanumeric() || *character == '_' || *character == '-')
                    .collect::<String>();

                if item_name.is_empty() {
                    return None;
                }

                Some(McpServerLock::normalize_item_name(&item_name))
            })
            .collect()
    }

    fn mcp_batch_item_completion_target(batch_block_prefix: &str) -> Option<(DeclarationKeyword, String)> {
        let mut brace_balance = 0_i32;
        let mut token_start_index = None;
        let mut last_item_keyword = None;

        for (character_index, character) in batch_block_prefix.char_indices() {
            if character.is_ascii_alphanumeric() || character == '_' {
                if brace_balance == 0 && token_start_index.is_none() {
                    token_start_index = Some(character_index);
                }

                continue;
            }

            if let Some(start_index) = token_start_index.take() {
                let identifier = &batch_block_prefix[start_index..character_index];

                if brace_balance == 0 {
                    if let Some(declaration_keyword) = DeclarationKeyword::from_identifier(identifier) {
                        if matches!(
                            declaration_keyword,
                            DeclarationKeyword::Tool | DeclarationKeyword::Prompt | DeclarationKeyword::Resource
                        ) {
                            last_item_keyword = Some((declaration_keyword, character_index));
                        }
                    }
                }
            }

            match character {
                '{' => brace_balance += 1,
                '}' => brace_balance -= 1,
                _ => {}
            }
        }

        if let Some(start_index) = token_start_index {
            let identifier = &batch_block_prefix[start_index..];

            if brace_balance == 0 {
                if let Some(declaration_keyword) = DeclarationKeyword::from_identifier(identifier) {
                    if matches!(
                        declaration_keyword,
                        DeclarationKeyword::Tool | DeclarationKeyword::Prompt | DeclarationKeyword::Resource
                    ) {
                        last_item_keyword = Some((declaration_keyword, batch_block_prefix.len()));
                    }
                }
            }
        }

        let (item_kind, item_keyword_end_index) = last_item_keyword?;
        let after_item_keyword = &batch_block_prefix[item_keyword_end_index..];

        if !after_item_keyword.starts_with(char::is_whitespace) {
            return None;
        }

        let item_prefix = after_item_keyword.trim_start();

        if item_prefix.split_whitespace().nth(1).is_some() {
            return None;
        }

        Some((item_kind, item_prefix.trim().to_string()))
    }

    pub(super) fn mcp_batch_import_allowed_keywords_at_position(&self, position: Position) -> Option<Vec<DeclarationKeyword>> {
        let cursor_offset = self.byte_offset(position)?;
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

        let _server_name = import_segments.next()?;

        match import_segments.next() {
            Some(import_kind) if import_kind == DeclarationKeyword::Tool.as_str() && import_segments.next().is_none() => {
                Some(vec![DeclarationKeyword::Tool])
            }
            Some(import_kind) if import_kind == DeclarationKeyword::Resource.as_str() && import_segments.next().is_none() => {
                Some(vec![DeclarationKeyword::Resource])
            }
            Some(import_kind) if import_kind == DeclarationKeyword::Prompt.as_str() && import_segments.next().is_none() => {
                Some(vec![DeclarationKeyword::Prompt])
            }
            None => Some(vec![
                DeclarationKeyword::Tool,
                DeclarationKeyword::Prompt,
                DeclarationKeyword::Resource,
            ]),
            _ => None,
        }
    }

    pub(super) fn prompt_import_binding_completion_context(
        &self,
        position: Position,
        line_prefix: &str,
    ) -> Option<PromptImportBindingCompletionContext> {
        if line_prefix.contains(':') {
            return None;
        }

        let cursor_offset = self.byte_offset(position)?;
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

        let workflow = self.semantic_snapshot.workflow_document().workflow()?;
        let prompt_import_declaration = workflow
            .prompt_imports()
            .filter(|prompt_import_declaration| {
                self.position_context(position)
                    .is_some_and(|position_context| position_context.contains(prompt_import_declaration.span))
            })
            .min_by_key(|prompt_import_declaration| {
                prompt_import_declaration
                    .span
                    .to_byte_range(&self.text)
                    .map_or(usize::MAX, |span_range| span_range.end.saturating_sub(span_range.start))
            })?;

        let binding_prefix = trailing_identifier(line_prefix).unwrap_or_default().to_string();
        let existing_binding_names = Self::existing_object_field_names(&source_prefix[bindings_open_brace_index + 1..]);

        Some(PromptImportBindingCompletionContext {
            server_name: prompt_import_declaration.source.server_name.clone(),
            prompt_name: prompt_import_declaration.source.item_name.clone(),
            binding_prefix,
            existing_binding_names,
        })
    }
}
