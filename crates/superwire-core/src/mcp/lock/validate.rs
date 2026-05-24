use super::{McpLock, McpPromptArgumentLock};
use crate::dsl::{Declaration, McpPromptImportDeclaration, Workflow};

impl McpLock {
    #[must_use]
    pub fn validate_prompt_import_bindings(&self, workflow: &Workflow) -> Vec<String> {
        let mut messages = Vec::new();

        for declaration in workflow.declarations() {
            match declaration {
                Declaration::McpPrompt(prompt_import_declaration) => {
                    messages.extend(self.prompt_import_binding_messages(prompt_import_declaration));
                }
                Declaration::McpPromptBatch(prompt_batch_import_declaration) => {
                    for prompt_item in &prompt_batch_import_declaration.items {
                        let prompt_import_declaration = prompt_item.to_prompt_import_declaration(
                            &prompt_batch_import_declaration.server_name,
                            &prompt_batch_import_declaration.parameters,
                        );

                        messages.extend(self.prompt_import_binding_messages(&prompt_import_declaration));
                    }
                }
                Declaration::McpBatch(batch_import_declaration) => {
                    for prompt_item in &batch_import_declaration.prompt_items {
                        let prompt_import_declaration = prompt_item.to_prompt_import_declaration(
                            &batch_import_declaration.server_name,
                            &batch_import_declaration.fixed_binding_fields,
                        );

                        messages.extend(self.prompt_import_binding_messages(&prompt_import_declaration));
                    }
                }
                Declaration::McpToolBatch(_)
                | Declaration::McpResourceBatch(_)
                | Declaration::McpResource(_)
                | Declaration::Tool(_)
                | Declaration::Provider(_)
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

        messages
    }

    #[must_use]
    fn prompt_import_binding_messages(&self, prompt_import_declaration: &McpPromptImportDeclaration) -> Vec<String> {
        let Some(server_lock) = self.servers.get(&prompt_import_declaration.source.server_name) else {
            return Vec::new();
        };
        let Some(prompt_arguments) = server_lock.prompt_arguments_for_name(&prompt_import_declaration.source.item_name) else {
            return Vec::new();
        };

        prompt_import_declaration.required_binding_messages(prompt_arguments)
    }
}

impl McpPromptImportDeclaration {
    #[must_use]
    fn required_binding_messages(&self, prompt_arguments: &[McpPromptArgumentLock]) -> Vec<String> {
        let mut messages = Vec::new();

        for prompt_argument in prompt_arguments.iter().filter(|prompt_argument| prompt_argument.required) {
            if self.has_parameter_binding(&prompt_argument.name) {
                continue;
            }

            messages.push(format!(
                "MCP prompt `{}` requires binding `{}` from server prompt `{}`",
                self.name, prompt_argument.name, self.source.item_name
            ));
        }

        messages
    }
}
