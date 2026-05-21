use super::{
    AgentDeclaration, Declaration, DynamicBlock, InputDeclaration, McpPromptImportDeclaration, McpResourceImportDeclaration,
    McpServerDeclaration, ModelDeclaration, OutputDeclaration, ProviderDeclaration, SchemaDeclaration, SecretsDeclaration, ToolDeclaration,
    TypeExpression,
};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Workflow {
    pub declarations: Vec<Declaration>,
    pub source_text: Option<String>,
}

impl Workflow {
    #[must_use]
    pub fn declarations(&self) -> &[Declaration] {
        &self.declarations
    }

    #[must_use]
    pub fn source_text(&self) -> Option<&str> {
        self.source_text.as_deref()
    }

    #[must_use]
    pub fn with_source_text(mut self, source_text: impl Into<String>) -> Self {
        self.source_text = Some(source_text.into());

        self
    }

    #[must_use]
    pub fn find_provider(&self, provider_name: &str) -> Option<&ProviderDeclaration> {
        self.declarations.iter().find_map(|declaration| match declaration {
            Declaration::Provider(provider_declaration) if provider_declaration.name == provider_name => Some(provider_declaration),
            _ => None,
        })
    }

    #[must_use]
    pub fn find_model(&self, model_name: &str) -> Option<&ModelDeclaration> {
        self.declarations.iter().find_map(|declaration| match declaration {
            Declaration::Model(model_declaration) if model_declaration.name == model_name => Some(model_declaration),
            _ => None,
        })
    }

    #[must_use]
    pub fn find_mcp_server(&self, server_name: &str) -> Option<&McpServerDeclaration> {
        self.declarations.iter().find_map(|declaration| match declaration {
            Declaration::McpServer(mcp_server_declaration) if mcp_server_declaration.name == server_name => Some(mcp_server_declaration),
            _ => None,
        })
    }

    #[must_use]
    pub fn find_secrets(&self) -> Option<&SecretsDeclaration> {
        self.declarations.iter().find_map(|declaration| match declaration {
            Declaration::Secrets(secrets_declaration) => Some(secrets_declaration),
            _ => None,
        })
    }

    #[must_use]
    pub fn find_input(&self) -> Option<&InputDeclaration> {
        self.declarations.iter().find_map(|declaration| match declaration {
            Declaration::Input(input_declaration) => Some(input_declaration),
            _ => None,
        })
    }

    #[must_use]
    pub fn find_schema(&self, schema_name: &str) -> Option<&SchemaDeclaration> {
        self.declarations.iter().find_map(|declaration| match declaration {
            Declaration::Schema(schema_declaration) if schema_declaration.name == schema_name => Some(schema_declaration),
            _ => None,
        })
    }

    #[must_use]
    pub fn find_tool(&self, tool_name: &str) -> Option<&ToolDeclaration> {
        self.tool_declarations().find(|tool_declaration| tool_declaration.name == tool_name)
    }

    pub fn tool_declarations(&self) -> impl Iterator<Item = &ToolDeclaration> {
        self.declarations.iter().flat_map(Declaration::tool_declarations)
    }

    #[must_use]
    pub fn find_resource_import(&self, resource_name: &str) -> Option<&McpResourceImportDeclaration> {
        self.declarations.iter().find_map(|declaration| match declaration {
            Declaration::McpResource(resource_import_declaration) if resource_import_declaration.name == resource_name => {
                Some(resource_import_declaration)
            }
            Declaration::McpBatch(batch_import_declaration) => batch_import_declaration
                .resources
                .iter()
                .find(|resource_import_declaration| resource_import_declaration.name == resource_name),
            Declaration::McpResourceBatch(resource_batch_import_declaration) => resource_batch_import_declaration
                .resources
                .iter()
                .find(|resource_import_declaration| resource_import_declaration.name == resource_name),
            _ => None,
        })
    }

    #[must_use]
    pub fn find_prompt_import(&self, prompt_name: &str) -> Option<&McpPromptImportDeclaration> {
        self.declarations.iter().find_map(|declaration| match declaration {
            Declaration::McpPrompt(prompt_import_declaration) if prompt_import_declaration.name == prompt_name => {
                Some(prompt_import_declaration)
            }
            Declaration::McpBatch(batch_import_declaration) => batch_import_declaration
                .prompts
                .iter()
                .find(|prompt_import_declaration| prompt_import_declaration.name == prompt_name),
            Declaration::McpPromptBatch(prompt_batch_import_declaration) => prompt_batch_import_declaration
                .prompts
                .iter()
                .find(|prompt_import_declaration| prompt_import_declaration.name == prompt_name),
            _ => None,
        })
    }

    pub fn resource_imports(&self) -> impl Iterator<Item = &McpResourceImportDeclaration> {
        self.declarations.iter().flat_map(|declaration| match declaration {
            Declaration::McpResource(resource_import_declaration) => std::slice::from_ref(resource_import_declaration).iter(),
            Declaration::McpBatch(batch_import_declaration) => batch_import_declaration.resources.iter(),
            Declaration::McpResourceBatch(resource_batch_import_declaration) => resource_batch_import_declaration.resources.iter(),
            _ => [].iter(),
        })
    }

    pub fn prompt_imports(&self) -> impl Iterator<Item = &McpPromptImportDeclaration> {
        self.declarations.iter().flat_map(|declaration| match declaration {
            Declaration::McpPrompt(prompt_import_declaration) => std::slice::from_ref(prompt_import_declaration).iter(),
            Declaration::McpBatch(batch_import_declaration) => batch_import_declaration.prompts.iter(),
            Declaration::McpPromptBatch(prompt_batch_import_declaration) => prompt_batch_import_declaration.prompts.iter(),
            _ => [].iter(),
        })
    }

    #[must_use]
    pub fn find_agent(&self, agent_name: &str) -> Option<&AgentDeclaration> {
        self.declarations.iter().find_map(|declaration| match declaration {
            Declaration::Agent(agent_declaration) if agent_declaration.name == agent_name => Some(agent_declaration),
            _ => None,
        })
    }

    #[must_use]
    pub fn find_output(&self) -> Option<&OutputDeclaration> {
        self.declarations.iter().find_map(|declaration| match declaration {
            Declaration::Output(output_declaration) => Some(output_declaration),
            _ => None,
        })
    }

    pub fn dynamic_blocks(&self) -> impl Iterator<Item = &DynamicBlock> {
        self.declarations.iter().filter_map(|declaration| match declaration {
            Declaration::Dynamic(dynamic_block) => Some(dynamic_block),
            Declaration::Provider(_)
            | Declaration::Model(_)
            | Declaration::McpServer(_)
            | Declaration::Secrets(_)
            | Declaration::Input(_)
            | Declaration::Schema(_)
            | Declaration::Tool(_)
            | Declaration::McpBatch(_)
            | Declaration::McpToolBatch(_)
            | Declaration::McpResourceBatch(_)
            | Declaration::McpPromptBatch(_)
            | Declaration::McpResource(_)
            | Declaration::McpPrompt(_)
            | Declaration::Agent(_)
            | Declaration::Output(_) => None,
        })
    }

    #[must_use]
    pub fn named_schema_types(&self) -> HashMap<String, TypeExpression> {
        self.declarations
            .iter()
            .filter_map(|declaration| match declaration {
                Declaration::Schema(schema_declaration) => Some((schema_declaration.name.clone(), schema_declaration.type_expression())),
                _ => None,
            })
            .collect()
    }
}
