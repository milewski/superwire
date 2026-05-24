use super::{McpPromptArgumentLock, McpServerLock, McpToolLock};
use std::collections::HashMap;

#[derive(Debug, Clone, Default)]
pub struct McpServerToolLookup {
    normalized_tool_names: HashMap<String, String>,
}

impl McpServerToolLookup {
    #[must_use]
    pub fn find_tool_with_name<'server>(
        &self,
        server_lock: &'server McpServerLock,
        requested_tool_name: &str,
    ) -> Option<(String, &'server McpToolLock)> {
        if let Some(mcp_tool_lock) = server_lock.tools.get(requested_tool_name) {
            return Some((requested_tool_name.to_string(), mcp_tool_lock));
        }

        let normalized_requested_name = McpServerLock::normalize_item_name(requested_tool_name);
        let resolved_tool_name = self.normalized_tool_names.get(&normalized_requested_name)?;
        let mcp_tool_lock = server_lock.tools.get(resolved_tool_name)?;

        Some((resolved_tool_name.clone(), mcp_tool_lock))
    }
}

impl McpServerLock {
    #[must_use]
    pub fn tool_lookup(&self) -> McpServerToolLookup {
        let mut normalized_tool_names = HashMap::new();

        for tool_name in self.tools.keys() {
            normalized_tool_names
                .entry(Self::normalize_item_name(tool_name))
                .or_insert_with(|| tool_name.clone());
        }

        McpServerToolLookup { normalized_tool_names }
    }

    #[must_use]
    pub fn find_tool_with_name(&self, requested_tool_name: &str) -> Option<(String, &McpToolLock)> {
        self.tool_lookup().find_tool_with_name(self, requested_tool_name)
    }

    #[must_use]
    pub fn find_resource_with_name(&self, requested_resource_name: &str) -> Option<String> {
        Self::find_listed_item_with_name(&self.resources, requested_resource_name)
    }

    #[must_use]
    pub fn find_prompt_with_name(&self, requested_prompt_name: &str) -> Option<String> {
        Self::find_listed_item_with_name(&self.prompts, requested_prompt_name)
    }

    #[must_use]
    pub fn prompt_arguments_for_name(&self, requested_prompt_name: &str) -> Option<&[McpPromptArgumentLock]> {
        let prompt_name = self.find_prompt_with_name(requested_prompt_name)?;

        self.prompt_arguments.get(&prompt_name).map(std::vec::Vec::as_slice)
    }

    #[must_use]
    fn find_listed_item_with_name(listed_item_names: &[String], requested_item_name: &str) -> Option<String> {
        if listed_item_names
            .iter()
            .any(|listed_item_name| listed_item_name == requested_item_name)
        {
            return Some(requested_item_name.to_string());
        }

        let normalized_requested_name = Self::normalize_item_name(requested_item_name);

        listed_item_names
            .iter()
            .find(|listed_item_name| Self::normalize_item_name(listed_item_name) == normalized_requested_name)
            .cloned()
    }

    #[must_use]
    pub fn normalize_item_name(item_name: &str) -> String {
        let mut normalized_name = String::new();
        let mut previous_was_underscore = false;

        for (character_index, character) in item_name.chars().enumerate() {
            if character.is_ascii_uppercase() {
                if character_index > 0 && !previous_was_underscore {
                    normalized_name.push('_');
                }

                normalized_name.push(character.to_ascii_lowercase());
                previous_was_underscore = false;

                continue;
            }

            if character.is_ascii_lowercase() || character.is_ascii_digit() {
                normalized_name.push(character);
                previous_was_underscore = false;

                continue;
            }

            if !previous_was_underscore {
                normalized_name.push('_');
                previous_was_underscore = true;
            }
        }

        normalized_name.trim_matches('_').to_string()
    }

    #[must_use]
    pub fn normalize_tool_name(tool_name: &str) -> String {
        Self::normalize_item_name(tool_name)
    }
}
