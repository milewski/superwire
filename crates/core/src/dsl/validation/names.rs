use super::super::ast::{
    AgentDeclaration, McpPromptImportDeclaration, McpResourceImportDeclaration, ModelDeclaration, ProviderDeclaration, SchemaDeclaration,
    ToolDeclaration,
};
use super::index::ValidationIndex;
use super::report::{ValidationIssue, ValidationReport};

impl ValidationIndex {
    pub(super) fn register_provider_name(
        &mut self,
        provider_declaration: &ProviderDeclaration,
        validation_report: &mut ValidationReport,
    ) -> bool {
        provider_declaration.validate_name(validation_report);

        let inserted_provider = self.provider_names.insert(provider_declaration.name.clone());

        if !inserted_provider {
            validation_report.push_issue_with_span(
                ValidationIssue::DuplicateProvider {
                    provider_name: provider_declaration.name.clone(),
                },
                Some(provider_declaration.span),
            );
        }

        inserted_provider
    }

    pub(super) fn register_model_name(&mut self, model_declaration: &ModelDeclaration, validation_report: &mut ValidationReport) -> bool {
        model_declaration.validate_name(validation_report);

        let inserted_model = self.model_names.insert(model_declaration.name.clone());

        if !inserted_model {
            validation_report.push_issue_with_span(
                ValidationIssue::DuplicateModel {
                    model_name: model_declaration.name.clone(),
                },
                Some(model_declaration.span),
            );
        }

        inserted_model
    }

    pub(super) fn register_schema_name(
        &mut self,
        schema_declaration: &SchemaDeclaration,
        validation_report: &mut ValidationReport,
    ) -> bool {
        schema_declaration.validate_name(validation_report);

        let inserted_schema = self.schema_names.insert(schema_declaration.name.clone());

        if !inserted_schema {
            validation_report.push_issue_with_span(
                ValidationIssue::DuplicateSchema {
                    schema_name: schema_declaration.name.clone(),
                },
                Some(schema_declaration.span),
            );
        }

        inserted_schema
    }

    pub(super) fn register_tool_name(&mut self, tool_declaration: &ToolDeclaration, validation_report: &mut ValidationReport) -> bool {
        let inserted_tool = self.tool_names.insert(tool_declaration.name.clone());

        if !inserted_tool {
            validation_report.push_issue_with_span(
                ValidationIssue::DuplicateTool {
                    tool_name: tool_declaration.name.clone(),
                },
                Some(tool_declaration.span),
            );
        }

        inserted_tool
    }

    pub(super) fn register_resource_name(
        &mut self,
        resource_import_declaration: &McpResourceImportDeclaration,
        validation_report: &mut ValidationReport,
    ) -> bool {
        let inserted_resource = self.resource_names.insert(resource_import_declaration.name.clone());

        if !inserted_resource {
            validation_report.push_issue_with_span(
                ValidationIssue::DuplicateResource {
                    resource_name: resource_import_declaration.name.clone(),
                },
                Some(resource_import_declaration.span),
            );
        }

        inserted_resource
    }

    pub(super) fn register_prompt_name(
        &mut self,
        prompt_import_declaration: &McpPromptImportDeclaration,
        validation_report: &mut ValidationReport,
    ) -> bool {
        let inserted_prompt = self.prompt_names.insert(prompt_import_declaration.name.clone());

        if !inserted_prompt {
            validation_report.push_issue_with_span(
                ValidationIssue::DuplicatePrompt {
                    prompt_name: prompt_import_declaration.name.clone(),
                },
                Some(prompt_import_declaration.span),
            );
        }

        inserted_prompt
    }

    pub(super) fn register_agent_name(&mut self, agent_declaration: &AgentDeclaration, validation_report: &mut ValidationReport) -> bool {
        let inserted_agent = self.agent_names.insert(agent_declaration.name.clone());

        if !inserted_agent {
            validation_report.push_issue_with_span(
                ValidationIssue::DuplicateAgent {
                    agent_name: agent_declaration.name.clone(),
                },
                Some(agent_declaration.span),
            );
        }

        inserted_agent
    }
}

impl ProviderDeclaration {
    fn validate_name(&self, validation_report: &mut ValidationReport) {
        if is_lowercase_snake_case(&self.name) {
            return;
        }

        validation_report.push_issue_with_span(
            ValidationIssue::InvalidProviderName {
                provider_name: self.name.clone(),
            },
            Some(self.span),
        );
    }
}

impl ModelDeclaration {
    fn validate_name(&self, validation_report: &mut ValidationReport) {
        if is_lowercase_snake_case(&self.name) {
            return;
        }

        validation_report.push_issue_with_span(
            ValidationIssue::InvalidModelName {
                model_name: self.name.clone(),
            },
            Some(self.span),
        );
    }
}

impl SchemaDeclaration {
    fn validate_name(&self, validation_report: &mut ValidationReport) {
        if is_lowercase_snake_case(&self.name) {
            return;
        }

        validation_report.push_issue_with_span(
            ValidationIssue::InvalidSchemaName {
                schema_name: self.name.clone(),
            },
            Some(self.span),
        );
    }
}

fn is_lowercase_snake_case(identifier: &str) -> bool {
    let mut characters = identifier.chars();

    let Some(first_character) = characters.next() else {
        return false;
    };

    if !first_character.is_ascii_lowercase() {
        return false;
    }

    characters.all(|character| character.is_ascii_lowercase() || character.is_ascii_digit() || character == '_')
}
