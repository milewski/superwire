use engine_ai_core::dsl::{
    parse_workflow, validate_workflow, AgentDeclaration, AgentProperty, Declaration, Expression, TypeExpression, ValidationIssue,
    ValidationReport, Workflow,
};

use crate::protocol::Position;

#[derive(Debug, Clone)]
pub struct ParsedDocument {
    source: String,
    workflow: Option<Workflow>,
    validation_report: Option<ValidationReport>,
}

impl ParsedDocument {
    #[must_use]
    pub fn parse(source: String) -> Self {
        let workflow = parse_workflow(&source).ok();
        let validation_report = workflow.as_ref().map(validate_workflow);

        Self {
            source,
            workflow,
            validation_report,
        }
    }

    #[must_use]
    pub fn source(&self) -> &str {
        &self.source
    }

    #[must_use]
    pub fn is_valid(&self) -> bool {
        self.workflow.is_some()
    }

    #[must_use]
    pub fn workflow(&self) -> Option<&Workflow> {
        self.workflow.as_ref()
    }

    #[must_use]
    pub fn diagnostics(&self) -> Vec<LspDiagnostic> {
        let mut diagnostics = Vec::new();

        if let Some(workflow) = &self.workflow {
            if let Some(validation_report) = &self.validation_report {
                for (issue, span) in validation_report.issues_with_spans() {
                    if let Some(span) = span {
                        diagnostics.push(LspDiagnostic {
                            range: source_span_to_range(span),
                            severity: diagnostic_severity(issue),
                            message: format_validation_issue(issue),
                        });
                    }
                }
            }

            for declaration in workflow.declarations() {
                if let Declaration::Agent(agent_declaration) = declaration {
                    for property in &agent_declaration.properties {
                        validate_agent_property(agent_declaration, property, &mut diagnostics);
                    }
                }
            }
        } else {
            diagnostics.push(LspDiagnostic {
                range: LspRange {
                    start: LspPosition { line: 0, character: 0 },
                    end: LspPosition { line: 0, character: 0 },
                },
                severity: DiagnosticSeverity::Error,
                message: "Failed to parse workflow".to_owned(),
            });
        }

        diagnostics
    }

    #[must_use]
    pub fn line_prefix(&self, position: Position) -> Option<String> {
        let line_text = self.source.lines().nth(position.line as usize)?;
        let line_characters: Vec<char> = line_text.chars().collect();
        let character_index = usize::min(position.character as usize, line_characters.len());

        Some(line_characters.into_iter().take(character_index).collect())
    }

    #[must_use]
    pub fn provider_names(&self) -> Vec<String> {
        self.declaration_names(|declaration| match declaration {
            Declaration::Provider(provider) => Some(provider.name.clone()),
            _ => None,
        })
    }

    #[must_use]
    pub fn schema_names(&self) -> Vec<String> {
        self.declaration_names(|declaration| match declaration {
            Declaration::Schema(schema) => Some(schema.name.clone()),
            _ => None,
        })
    }

    #[must_use]
    pub fn agent_names(&self) -> Vec<String> {
        self.declaration_names(|declaration| match declaration {
            Declaration::Agent(agent) => Some(agent.name.clone()),
            _ => None,
        })
    }

    #[must_use]
    pub fn input_fields(&self) -> Vec<String> {
        self.workflow()
            .and_then(Workflow::find_input)
            .map(|input_declaration| input_declaration.fields.iter().map(|field| field.name.clone()).collect())
            .unwrap_or_default()
    }

    #[must_use]
    pub fn secrets_fields(&self) -> Vec<String> {
        self.workflow()
            .and_then(Workflow::find_secrets)
            .map(|secrets_declaration| secrets_declaration.fields.iter().map(|field| field.name.clone()).collect())
            .unwrap_or_default()
    }

    #[must_use]
    pub fn find_agent_output_fields(&self, agent_name: &str) -> Vec<String> {
        let Some(workflow) = self.workflow() else {
            return Vec::new();
        };

        let Some(agent_declaration) = workflow.find_agent(agent_name) else {
            return Vec::new();
        };

        let Some(output_type) = agent_declaration.properties.iter().find_map(|property| {
            if let AgentProperty::Output(type_expression) = property {
                Some(type_expression)
            } else {
                None
            }
        }) else {
            return Vec::new();
        };

        self.resolve_type_fields(output_type)
    }

    #[must_use]
    pub fn resolve_type_fields(&self, type_expression: &TypeExpression) -> Vec<String> {
        match type_expression {
            TypeExpression::Object(fields) => fields.iter().map(|field| field.name.clone()).collect(),
            TypeExpression::SchemaReference(schema_name) => {
                let Some(workflow) = self.workflow() else {
                    return Vec::new();
                };

                workflow
                    .find_schema(schema_name)
                    .map(|schema| schema.fields.iter().map(|field| field.name.clone()).collect())
                    .unwrap_or_default()
            }
            TypeExpression::Union(types) => {
                let mut fields = Vec::new();

                for union_type in types {
                    fields.extend(self.resolve_type_fields(union_type));
                }

                fields
            }
            _ => Vec::new(),
        }
    }

    #[must_use]
    pub fn resolve_identifier_type(&self, identifier: &str) -> Option<TypeExpression> {
        if let Some(schema) = self.workflow().and_then(|w| w.find_schema(identifier)) {
            return Some(TypeExpression::SchemaReference(schema.name.clone()));
        }

        if let Some(agent) = self.workflow().and_then(|w| w.find_agent(identifier)) {
            return agent.properties.iter().find_map(|property| {
                if let AgentProperty::Output(type_expression) = property {
                    Some(type_expression.clone())
                } else {
                    None
                }
            });
        }

        if identifier == "input" {
            return Some(TypeExpression::Object(
                self.workflow()
                    .and_then(Workflow::find_input)
                    .map(|input| input.fields.clone())
                    .unwrap_or_default(),
            ));
        }

        if identifier == "secrets" {
            return Some(TypeExpression::Object(
                self.workflow()
                    .and_then(Workflow::find_secrets)
                    .map(|secrets| secrets.fields.clone())
                    .unwrap_or_default(),
            ));
        }

        None
    }

    #[must_use]
    pub fn resolve_reference_chain_fields(&self, reference_chain: &[String]) -> Vec<String> {
        if reference_chain.is_empty() {
            return Vec::new();
        }

        let root_identifier = &reference_chain[0];
        let fields_after_root = &reference_chain[1..];

        let mut current_type = if root_identifier == "agent" {
            if fields_after_root.is_empty() {
                return self.agent_names();
            }

            let agent_name = &fields_after_root[0];
            let remaining_fields = &fields_after_root[1..];

            let Some(agent) = self.workflow().and_then(|w| w.find_agent(agent_name)) else {
                return Vec::new();
            };

            let Some(output_type) = agent.properties.iter().find_map(|property| {
                if let AgentProperty::Output(type_expression) = property {
                    Some(type_expression.clone())
                } else {
                    None
                }
            }) else {
                return Vec::new();
            };

            let mut type_cursor = output_type;

            for field_name in remaining_fields {
                let next_type = self.find_field_type(&type_cursor, field_name);

                match next_type {
                    Some(resolved_type) => type_cursor = resolved_type,
                    None => return Vec::new(),
                }
            }

            return self.resolve_type_fields(&type_cursor);
        } else if root_identifier == "input" {
            if fields_after_root.is_empty() {
                return self.input_fields();
            }

            let Some(input_declaration) = self.workflow().and_then(Workflow::find_input) else {
                return Vec::new();
            };

            TypeExpression::Object(input_declaration.fields.clone())
        } else if root_identifier == "secrets" {
            if fields_after_root.is_empty() {
                return self.secrets_fields();
            }

            let Some(secrets_declaration) = self.workflow().and_then(Workflow::find_secrets) else {
                return Vec::new();
            };

            TypeExpression::Object(secrets_declaration.fields.clone())
        } else if let Some(resolved_type) = self.resolve_identifier_type(root_identifier) {
            resolved_type
        } else {
            return Vec::new();
        };

        for field_name in fields_after_root {
            let next_type = self.find_field_type(&current_type, field_name);

            match next_type {
                Some(resolved_type) => current_type = resolved_type,
                None => return Vec::new(),
            }
        }

        self.resolve_type_fields(&current_type)
    }

    #[must_use]
    pub fn find_field_type(&self, type_expression: &TypeExpression, field_name: &str) -> Option<TypeExpression> {
        match type_expression {
            TypeExpression::Object(fields) => fields
                .iter()
                .find(|field| field.name == field_name)
                .map(|field| field.field_type.clone()),
            TypeExpression::SchemaReference(schema_name) => self.workflow().and_then(|w| w.find_schema(schema_name)).and_then(|schema| {
                schema
                    .fields
                    .iter()
                    .find(|field| field.name == field_name)
                    .map(|field| field.field_type.clone())
            }),
            TypeExpression::Union(types) => {
                for union_type in types {
                    if let Some(field_type) = self.find_field_type(union_type, field_name) {
                        return Some(field_type);
                    }
                }

                None
            }
            _ => None,
        }
    }

    fn declaration_names<F>(&self, extract_name: F) -> Vec<String>
    where
        F: Fn(&Declaration) -> Option<String>,
    {
        self.workflow()
            .map(|workflow| workflow.declarations().iter().filter_map(extract_name).collect())
            .unwrap_or_default()
    }
}

#[must_use]
pub fn parse_reference_chain(line_prefix: &str) -> Vec<String> {
    let trimmed = line_prefix.trim_end();
    let mut chain = Vec::new();
    let mut position = 0;
    let characters: Vec<char> = trimmed.chars().collect();

    while position < characters.len() {
        while position < characters.len() && (characters[position] == '.' || characters[position] == '?') {
            position += 1;
        }

        if position >= characters.len() {
            break;
        }

        let start = position;

        while position < characters.len() && (characters[position].is_ascii_alphanumeric() || characters[position] == '_') {
            position += 1;
        }

        if position == start {
            break;
        }

        chain.push(characters[start..position].iter().collect::<String>());
    }

    chain
}

#[must_use]
pub fn unescape_string(raw_string: &str) -> String {
    if raw_string.len() < 2 {
        return String::new();
    }

    let mut parsed = String::new();
    let mut characters = raw_string[1..raw_string.len() - 1].chars();

    while let Some(character) = characters.next() {
        if character != '\\' {
            parsed.push(character);
            continue;
        }

        let Some(escaped) = characters.next() else {
            parsed.push('\\');
            continue;
        };

        let unescaped = match escaped {
            'n' => '\n',
            'r' => '\r',
            't' => '\t',
            '\\' => '\\',
            '"' => '"',
            _ => escaped,
        };

        parsed.push(unescaped);
    }

    parsed
}

#[must_use]
#[allow(clippy::cast_possible_truncation)]
pub fn pest_position_to_lsp(engine_ai_core::dsl::SourcePosition { line, column }: engine_ai_core::dsl::SourcePosition) -> LspPosition {
    LspPosition {
        line: (line.saturating_sub(1)) as u32,
        character: (column.saturating_sub(1)) as u32,
    }
}

#[must_use]
pub fn source_span_to_range(span: engine_ai_core::dsl::SourceSpan) -> LspRange {
    LspRange {
        start: pest_position_to_lsp(span.start),
        end: pest_position_to_lsp(span.end),
    }
}

#[derive(Debug, Clone, Copy)]
pub struct LspPosition {
    pub line: u32,
    pub character: u32,
}

#[derive(Debug, Clone, Copy)]
pub struct LspRange {
    pub start: LspPosition,
    pub end: LspPosition,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticSeverity {
    Error,
    Warning,
    Information,
    Hint,
}

#[derive(Debug, Clone)]
pub struct LspDiagnostic {
    pub range: LspRange,
    pub severity: DiagnosticSeverity,
    pub message: String,
}

fn diagnostic_severity(issue: &ValidationIssue) -> DiagnosticSeverity {
    match issue {
        ValidationIssue::DuplicateProvider { .. }
        | ValidationIssue::DuplicateSchema { .. }
        | ValidationIssue::DuplicateAgent { .. }
        | ValidationIssue::DuplicateSingletonDeclaration { .. }
        | ValidationIssue::InvalidModelExpression { .. }
        | ValidationIssue::UnknownProviderInModel { .. }
        | ValidationIssue::UnknownModelForProvider { .. }
        | ValidationIssue::UnknownAgentReference { .. }
        | ValidationIssue::InvalidKeywordReferenceRoot { .. }
        | ValidationIssue::MissingInputDeclaration { .. }
        | ValidationIssue::MissingSecretsDeclaration { .. }
        | ValidationIssue::UnknownInputFieldReference { .. }
        | ValidationIssue::UnknownSecretsFieldReference { .. }
        | ValidationIssue::SecretReferenceInLlmContext { .. }
        | ValidationIssue::MissingAgentOutputTypeForFieldReference { .. }
        | ValidationIssue::InvalidReferencePath { .. }
        | ValidationIssue::UnknownSchemaReference { .. }
        | ValidationIssue::AgentDependencyCycle { .. } => DiagnosticSeverity::Error,
    }
}

fn format_validation_issue(issue: &ValidationIssue) -> String {
    match issue {
        ValidationIssue::DuplicateProvider { provider_name } => format!("Duplicate provider declaration: {provider_name}"),
        ValidationIssue::DuplicateSchema { schema_name } => format!("Duplicate schema declaration: {schema_name}"),
        ValidationIssue::DuplicateAgent { agent_name } => format!("Duplicate agent declaration: {agent_name}"),
        ValidationIssue::DuplicateSingletonDeclaration { declaration_kind } => {
            format!("Duplicate singleton declaration: {declaration_kind:?}")
        }
        ValidationIssue::InvalidModelExpression { agent_name } => format!("Invalid model expression in agent: {agent_name}"),
        ValidationIssue::UnknownProviderInModel { agent_name, provider_name } => {
            format!("Unknown provider '{provider_name}' in model binding for agent: {agent_name}")
        }
        ValidationIssue::UnknownModelForProvider {
            agent_name,
            provider_name,
            model_name,
        } => {
            format!("Unknown model '{model_name}' for provider '{provider_name}' in agent: {agent_name}")
        }
        ValidationIssue::UnknownAgentReference { referenced_agent, context } => {
            format!("Unknown agent reference '{referenced_agent}' in {context:?}")
        }
        ValidationIssue::InvalidKeywordReferenceRoot { keyword, context } => {
            format!("Invalid keyword reference root '{:?}' in {:?}", keyword.as_str(), context)
        }
        ValidationIssue::MissingInputDeclaration { context } => {
            format!("Missing input declaration referenced in {context:?}")
        }
        ValidationIssue::MissingSecretsDeclaration { context } => {
            format!("Missing secrets declaration referenced in {context:?}")
        }
        ValidationIssue::UnknownInputFieldReference { field_name, context } => {
            format!("Unknown input field '{field_name}' in {context:?}")
        }
        ValidationIssue::UnknownSecretsFieldReference { field_name, context } => {
            format!("Unknown secrets field '{field_name}' in {context:?}")
        }
        ValidationIssue::SecretReferenceInLlmContext { reference_path, context } => {
            format!("Secret reference '{reference_path}' should not be exposed in {context:?}")
        }
        ValidationIssue::MissingAgentOutputTypeForFieldReference { agent_name, context } => {
            format!("Agent '{agent_name}' has no output type for field reference in {context:?}")
        }
        ValidationIssue::InvalidReferencePath {
            reference_path,
            invalid_field,
            context,
        } => {
            format!("Invalid field '{invalid_field}' in reference path '{reference_path}' in {context:?}")
        }
        ValidationIssue::UnknownSchemaReference {
            referenced_schema,
            context,
        } => {
            format!("Unknown schema reference '{referenced_schema}' in {context:?}")
        }
        ValidationIssue::AgentDependencyCycle { agent_names } => {
            format!("Agent dependency cycle detected: {}", agent_names.join(", "))
        }
    }
}

fn validate_agent_property(agent: &AgentDeclaration, property: &AgentProperty, diagnostics: &mut Vec<LspDiagnostic>) {
    if let AgentProperty::Model(Expression::StringLiteral(_)) = property {
        diagnostics.push(LspDiagnostic {
            range: source_span_to_range(agent.span),
            severity: DiagnosticSeverity::Error,
            message: format!(
                "Agent '{}' model should be a function call like `provider_name(\"model_name\")`, not a string literal",
                agent.name
            ),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_valid_document() {
        let document = ParsedDocument::parse(
            r#"
            provider ollama {
                driver: "ollama"
                models: ["qwen3.5:32b"]
            }

            agent greeting {
                model: ollama("qwen3.5:32b")
                prompt: "Hello"
                output: string
            }

            output {
                greeting: agent.greeting
            }
            "#
            .to_owned(),
        );

        assert!(document.is_valid());
        assert_eq!(document.provider_names(), vec!["ollama"]);
        assert_eq!(document.agent_names(), vec!["greeting"]);
    }

    #[test]
    fn extracts_schema_names() {
        let document = ParsedDocument::parse(
            r#"
            schema User {
                name: string
                email: string
            }

            schema Profile {
                bio: string
            }
            "#
            .to_owned(),
        );

        let names = document.schema_names();
        assert!(names.contains(&"User".to_owned()));
        assert!(names.contains(&"Profile".to_owned()));
    }

    #[test]
    fn extracts_input_fields() {
        let document = ParsedDocument::parse(
            r#"
            input {
                title: string
                count: number
            }
            "#
            .to_owned(),
        );

        let fields = document.input_fields();
        assert!(fields.contains(&"title".to_owned()));
        assert!(fields.contains(&"count".to_owned()));
    }

    #[test]
    fn resolves_agent_output_fields() {
        let document = ParsedDocument::parse(
            r#"
            schema Result {
                answer: string
                confidence: number
            }

            agent responder {
                output: schema.Result
            }
            "#
            .to_owned(),
        );

        let fields = document.find_agent_output_fields("responder");
        assert!(fields.contains(&"answer".to_owned()));
        assert!(fields.contains(&"confidence".to_owned()));
    }

    #[test]
    fn parses_reference_chain() {
        assert_eq!(parse_reference_chain("agent.foo."), vec!["agent", "foo"]);
        assert_eq!(parse_reference_chain("input."), vec!["input"]);
        assert_eq!(parse_reference_chain("agent.foo.bar."), vec!["agent", "foo", "bar"]);
        assert_eq!(parse_reference_chain("secrets."), vec!["secrets"]);
        assert_eq!(parse_reference_chain("schema.User."), vec!["schema", "User"]);
    }

    #[test]
    fn generates_diagnostics_for_unknown_reference() {
        let document = ParsedDocument::parse(
            r#"
            input {
                title: string
            }

            agent researcher {
                prompt: input.nonexistent
            }
            "#
            .to_owned(),
        );

        let diagnostics = document.diagnostics();
        assert!(!diagnostics.is_empty());
        assert!(diagnostics.iter().any(|d| d.message.contains("nonexistent")));
    }
}
