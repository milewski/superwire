use super::{McpLock, McpToolLock};
use crate::dsl::{Declaration, McpPromptImportDeclaration, McpResourceImportDeclaration, ToolDeclaration, ToolSource, Workflow};

impl McpLock {
    pub fn apply_to_workflow(&self, workflow: &mut Workflow) {
        for declaration in &mut workflow.declarations {
            match declaration {
                Declaration::Tool(tool_declaration) => {
                    self.apply_to_tool_declaration(tool_declaration);
                }
                Declaration::McpToolBatch(tool_batch_import_declaration) => {
                    for tool_declaration in &mut tool_batch_import_declaration.tools {
                        self.apply_to_tool_declaration(tool_declaration);
                    }
                }
                Declaration::McpBatch(batch_import_declaration) => {
                    for tool_declaration in &mut batch_import_declaration.tools {
                        self.apply_to_tool_declaration(tool_declaration);
                    }

                    for resource_import_declaration in &mut batch_import_declaration.resources {
                        self.apply_to_resource_import_declaration(resource_import_declaration);
                    }

                    for prompt_import_declaration in &mut batch_import_declaration.prompts {
                        self.apply_to_prompt_import_declaration(prompt_import_declaration);
                    }
                }
                Declaration::McpResourceBatch(resource_batch_import_declaration) => {
                    for resource_import_declaration in &mut resource_batch_import_declaration.resources {
                        self.apply_to_resource_import_declaration(resource_import_declaration);
                    }
                }
                Declaration::McpPromptBatch(prompt_batch_import_declaration) => {
                    for prompt_import_declaration in &mut prompt_batch_import_declaration.prompts {
                        self.apply_to_prompt_import_declaration(prompt_import_declaration);
                    }
                }
                Declaration::McpResource(resource_import_declaration) => {
                    self.apply_to_resource_import_declaration(resource_import_declaration);
                }
                Declaration::McpPrompt(prompt_import_declaration) => {
                    self.apply_to_prompt_import_declaration(prompt_import_declaration);
                }
                Declaration::Provider(_)
                | Declaration::Model(_)
                | Declaration::McpServer(_)
                | Declaration::Secrets(_)
                | Declaration::Input(_)
                | Declaration::Schema(_)
                | Declaration::Dynamic(_)
                | Declaration::Agent(_)
                | Declaration::Output(_) => {}
            }
        }
    }

    fn apply_to_tool_declaration(&self, tool_declaration: &mut ToolDeclaration) {
        let Some((resolved_tool_name, mcp_tool)) = self.find_tool_for_tool_declaration(tool_declaration) else {
            return;
        };

        if let Some(ToolSource::Mcp(mcp_tool_source)) = &mut tool_declaration.source {
            mcp_tool_source.tool_name = resolved_tool_name;
        }

        tool_declaration.apply_mcp_schema(mcp_tool);
    }

    fn apply_to_resource_import_declaration(&self, resource_import_declaration: &mut McpResourceImportDeclaration) {
        if let Some(resolved_resource_name) = self.find_resource_name(
            &resource_import_declaration.source.server_name,
            &resource_import_declaration.source.item_name,
        ) {
            resource_import_declaration.source.item_name = resolved_resource_name;
        }
    }

    fn apply_to_prompt_import_declaration(&self, prompt_import_declaration: &mut McpPromptImportDeclaration) {
        if let Some(resolved_prompt_name) = self.find_prompt_name(
            &prompt_import_declaration.source.server_name,
            &prompt_import_declaration.source.item_name,
        ) {
            prompt_import_declaration.source.item_name = resolved_prompt_name;
        }
    }

    #[must_use]
    fn find_resource_name(&self, server_name: &str, requested_resource_name: &str) -> Option<String> {
        self.servers.get(server_name)?.find_resource_with_name(requested_resource_name)
    }

    #[must_use]
    fn find_prompt_name(&self, server_name: &str, requested_prompt_name: &str) -> Option<String> {
        self.servers.get(server_name)?.find_prompt_with_name(requested_prompt_name)
    }

    #[must_use]
    fn find_tool_for_tool_declaration(&self, tool_declaration: &ToolDeclaration) -> Option<(String, &McpToolLock)> {
        let Some(tool_source) = &tool_declaration.source else {
            return None;
        };

        let ToolSource::Mcp(mcp_tool_source) = tool_source;

        if mcp_tool_source.server_name.is_none() {
            if let Some(server_lock) = self.servers.get(&mcp_tool_source.tool_name) {
                if let Some((resolved_tool_name, mcp_tool)) = server_lock.find_tool_with_name(&tool_declaration.name) {
                    return Some((resolved_tool_name, mcp_tool));
                }
            }
        }

        self.find_tool_with_name(tool_source)
    }
}

impl ToolDeclaration {
    fn apply_mcp_schema(&mut self, mcp_tool: &McpToolLock) {
        if self.description.is_none() {
            self.description.clone_from(&mcp_tool.description);
        }

        if self.input_fields.is_empty() {
            let fixed_binding_names = self
                .fixed_binding_fields
                .iter()
                .map(|fixed_binding_field| fixed_binding_field.name.as_str())
                .collect::<Vec<_>>();

            self.input_fields = mcp_tool.input_fields_except(&fixed_binding_names);
        }

        if self.output_fields.is_empty() {
            self.output_fields = mcp_tool.output_fields();
        }
    }
}
