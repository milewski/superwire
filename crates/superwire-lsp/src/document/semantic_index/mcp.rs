use super::super::{CompletionSuggestion, RenderTypeExpression};
use super::SemanticIndex;
use lsp_types::CompletionItemKind;
use std::collections::{BTreeMap, HashSet};
use superwire_core::mcp::{McpServerLock, McpToolLock};
use superwire_dsl::{ToolPropertyName, TypedField};

impl SemanticIndex {
    pub fn mcp_tool_batch_item_suggestions(
        &self,
        server_name: &str,
        tool_prefix: &str,
        existing_tool_names: &[String],
    ) -> Vec<CompletionSuggestion> {
        let Some(server_lock) = self.mcp_lock.as_ref().and_then(|mcp_lock| mcp_lock.servers.get(server_name)) else {
            return Vec::new();
        };
        let existing_tool_name_set = existing_tool_names.iter().map(String::as_str).collect::<HashSet<_>>();

        let mut normalized_tool_names = server_lock
            .tools
            .keys()
            .map(|tool_name| McpServerLock::normalize_item_name(tool_name))
            .filter(|normalized_tool_name| normalized_tool_name.starts_with(tool_prefix))
            .filter(|normalized_tool_name| !existing_tool_name_set.contains(normalized_tool_name.as_str()))
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
        let existing_resource_name_set = existing_resource_names.iter().map(String::as_str).collect::<HashSet<_>>();

        let mut normalized_resource_names = server_lock
            .resources
            .iter()
            .map(|resource_name| McpServerLock::normalize_item_name(resource_name))
            .filter(|normalized_resource_name| normalized_resource_name.starts_with(resource_prefix))
            .filter(|normalized_resource_name| !existing_resource_name_set.contains(normalized_resource_name.as_str()))
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
        let existing_prompt_name_set = existing_prompt_names.iter().map(String::as_str).collect::<HashSet<_>>();

        let mut normalized_prompt_names = server_lock
            .prompts
            .iter()
            .map(|prompt_name| McpServerLock::normalize_item_name(prompt_name))
            .filter(|normalized_prompt_name| normalized_prompt_name.starts_with(prompt_prefix))
            .filter(|normalized_prompt_name| !existing_prompt_name_set.contains(normalized_prompt_name.as_str()))
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
        let existing_binding_name_set = existing_binding_names.iter().map(String::as_str).collect::<HashSet<_>>();

        prompt_arguments
            .iter()
            .filter(|prompt_argument| prompt_argument.name.starts_with(binding_prefix))
            .filter(|prompt_argument| !existing_binding_name_set.contains(prompt_argument.name.as_str()))
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
        let existing_field_name_set = existing_field_names.iter().map(String::as_str).collect::<HashSet<_>>();

        self.mcp_tool_schema_fields(tool_name, property_name)
            .iter()
            .filter(|typed_field| typed_field.name.starts_with(field_prefix))
            .filter(|typed_field| !existing_field_name_set.contains(typed_field.name.as_str()))
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
                    self.find_mcp_tool_lock_in_server(server_name, server_lock, tool_name)
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
            let mut tool_fields_by_name = BTreeMap::new();

            for tool_field in tool_fields {
                tool_fields_by_name
                    .entry(tool_field.name)
                    .or_insert_with(Vec::new)
                    .push(tool_field.field_type);
            }

            common_fields.retain(|common_field| {
                tool_fields_by_name.get(&common_field.name).is_some_and(|tool_field_types| {
                    tool_field_types
                        .iter()
                        .any(|tool_field_type| tool_field_type == &common_field.field_type)
                })
            });
        }

        common_fields
    }

    fn schema_fields_from_mcp_tool_lock(mcp_tool_lock: &McpToolLock, property_name: ToolPropertyName) -> Vec<TypedField> {
        match property_name {
            ToolPropertyName::Input | ToolPropertyName::Bindings => mcp_tool_lock.input_fields_except(&[]),
            ToolPropertyName::Output => mcp_tool_lock.output_fields(),
            ToolPropertyName::Description | ToolPropertyName::MaxCalls => Vec::new(),
        }
    }

    fn mcp_tool_lock(&self, tool_name: &str) -> Option<&McpToolLock> {
        let tool_summary = self.tools.get(tool_name)?;
        let mcp_tool_name = tool_summary.mcp_tool_name.as_deref()?;

        self.mcp_tool_lock_for_source(tool_summary.mcp_server_name.as_deref(), mcp_tool_name)
    }

    fn mcp_tool_lock_for_source(&self, server_name: Option<&str>, mcp_tool_name: &str) -> Option<&McpToolLock> {
        let mcp_lock = self.mcp_lock.as_ref()?;

        if let Some(server_name) = server_name {
            let server_lock = mcp_lock.servers.get(server_name)?;

            return self
                .find_mcp_tool_lock_in_server(server_name, server_lock, mcp_tool_name)
                .map(|(_resolved_tool_name, mcp_tool_lock)| mcp_tool_lock);
        }

        mcp_lock.servers.iter().find_map(|(server_name, server_lock)| {
            self.find_mcp_tool_lock_in_server(server_name, server_lock, mcp_tool_name)
                .map(|(_resolved_tool_name, mcp_tool_lock)| mcp_tool_lock)
        })
    }

    fn find_mcp_tool_lock_in_server<'lock>(
        &self,
        server_name: &str,
        server_lock: &'lock McpServerLock,
        mcp_tool_name: &str,
    ) -> Option<(String, &'lock McpToolLock)> {
        if let Some(tool_lookup) = self.mcp_server_tool_lookups.get(server_name) {
            return tool_lookup.find_tool_with_name(server_lock, mcp_tool_name);
        }

        server_lock.tool_lookup().find_tool_with_name(server_lock, mcp_tool_name)
    }
}
