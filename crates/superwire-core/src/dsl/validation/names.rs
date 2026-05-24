use super::super::ast::{ModelDeclaration, ProviderDeclaration, SchemaDeclaration};
use super::report::{ValidationIssue, ValidationReport};

impl ProviderDeclaration {
    pub(crate) fn validate_name(&self, validation_report: &mut ValidationReport) {
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
    pub(crate) fn validate_name(&self, validation_report: &mut ValidationReport) {
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
    pub(crate) fn validate_name(&self, validation_report: &mut ValidationReport) {
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
