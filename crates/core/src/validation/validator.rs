use crate::ast::{Agent, AgentProperty, Reference, Value, Workflow};
use crate::validation::error::ValidationError;
use std::collections::{HashMap, HashSet};

pub struct WorkflowValidator;

impl WorkflowValidator {
    pub fn validate(workflow: &Workflow) -> Result<(), Vec<ValidationError>> {
        let mut errors = Vec::new();

        Self::check_duplicate_agent_names(workflow, &mut errors);
        Self::check_duplicate_schema_names(workflow, &mut errors);
        Self::check_duplicate_provider_names(workflow, &mut errors);
        Self::check_undefined_references(workflow, &mut errors);
        Self::check_provider_model_references(workflow, &mut errors);

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    fn check_duplicate_agent_names(workflow: &Workflow, errors: &mut Vec<ValidationError>) {
        let mut seen = HashMap::new();

        for agent in &workflow.agents {
            if let Some(first_location) = seen.get(&agent.name) {
                errors.push(ValidationError::DuplicateName {
                    file_path: "workflow".to_string(),
                    line: agent.span.line,
                    column: agent.span.column,
                    name: agent.name.clone(),
                    first_defined_at: format!("line {}", first_location),
                    suggestion: Some(format!("Rename one of the '{}' agents", agent.name)),
                });
            } else {
                seen.insert(agent.name.clone(), agent.span.line);
            }
        }
    }

    fn check_duplicate_schema_names(workflow: &Workflow, errors: &mut Vec<ValidationError>) {
        let mut seen = HashMap::new();

        for schema in &workflow.schemas {
            if let Some(first_location) = seen.get(&schema.name) {
                errors.push(ValidationError::DuplicateName {
                    file_path: "workflow".to_string(),
                    line: schema.span.line,
                    column: schema.span.column,
                    name: schema.name.clone(),
                    first_defined_at: format!("line {}", first_location),
                    suggestion: Some(format!("Rename one of the '{}' schemas", schema.name)),
                });
            } else {
                seen.insert(schema.name.clone(), schema.span.line);
            }
        }
    }

    fn check_duplicate_provider_names(workflow: &Workflow, errors: &mut Vec<ValidationError>) {
        let mut seen = HashMap::new();

        for provider in &workflow.providers {
            if let Some(first_location) = seen.get(&provider.name) {
                errors.push(ValidationError::DuplicateName {
                    file_path: "workflow".to_string(),
                    line: provider.span.line,
                    column: provider.span.column,
                    name: provider.name.clone(),
                    first_defined_at: format!("line {}", first_location),
                    suggestion: Some(format!("Rename one of the '{}' providers", provider.name)),
                });
            } else {
                seen.insert(provider.name.clone(), provider.span.line);
            }
        }
    }

    fn check_undefined_references(workflow: &Workflow, errors: &mut Vec<ValidationError>) {
        let agent_names: HashSet<String> = workflow.agents.iter().map(|a| a.name.clone()).collect();
        let schema_names: HashSet<String> = workflow.schemas.iter().map(|s| s.name.clone()).collect();

        for agent in &workflow.agents {
            Self::check_agent_references(agent, &agent_names, &schema_names, errors);
        }
    }

    fn check_agent_references(
        agent: &Agent,
        agent_names: &HashSet<String>,
        schema_names: &HashSet<String>,
        errors: &mut Vec<ValidationError>,
    ) {
        for property in &agent.properties {
            match property {
                AgentProperty::Model { value, span } => {
                    Self::check_value_references(value, agent_names, schema_names, span.line, span.column, errors);
                }
                AgentProperty::Tools { value, span } => {
                    Self::check_value_references(value, agent_names, schema_names, span.line, span.column, errors);
                }
                AgentProperty::Context { value, span } => {
                    Self::check_value_references(value, agent_names, schema_names, span.line, span.column, errors);
                }
                AgentProperty::Prompt { value, span } => {
                    Self::check_value_references(value, agent_names, schema_names, span.line, span.column, errors);
                }
                AgentProperty::ForEach { collection, span, .. } => {
                    Self::check_value_references(collection, agent_names, schema_names, span.line, span.column, errors);
                }
                AgentProperty::Output { .. } => {}
            }
        }
    }

    fn check_value_references(
        value: &Value,
        agent_names: &HashSet<String>,
        schema_names: &HashSet<String>,
        line: usize,
        column: usize,
        errors: &mut Vec<ValidationError>,
    ) {
        match value {
            Value::Reference(reference) => {
                Self::check_reference(reference, agent_names, schema_names, line, column, errors);
            }
            Value::Interpolated(template) => {
                let interpolation_pattern = regex::Regex::new(r"\{\{([^}]+)\}\}").unwrap();

                for capture in interpolation_pattern.captures_iter(template) {
                    let reference_text = capture[1].trim();
                    let parts: Vec<&str> = reference_text.split('.').collect();

                    if parts.len() == 1 && parts[0] != "input" {
                        if !agent_names.contains(parts[0]) {
                            errors.push(ValidationError::UndefinedReference {
                                file_path: "workflow".to_string(),
                                line,
                                column,
                                reference: parts[0].to_string(),
                                suggestion: Some(format!("Define an agent named '{}'", parts[0])),
                            });
                        }
                    } else if parts.len() == 2 && parts[0] != "input" && !agent_names.contains(parts[0]) {
                        errors.push(ValidationError::UndefinedReference {
                            file_path: "workflow".to_string(),
                            line,
                            column,
                            reference: parts[0].to_string(),
                            suggestion: Some(format!("Define an agent named '{}'", parts[0])),
                        });
                    }
                }
            }
            Value::Array(values) => {
                for val in values {
                    Self::check_value_references(val, agent_names, schema_names, line, column, errors);
                }
            }
            Value::Object(map) => {
                for val in map.values() {
                    Self::check_value_references(val, agent_names, schema_names, line, column, errors);
                }
            }
            Value::FunctionCall(func_call) => {
                for val in func_call.arguments.values() {
                    Self::check_value_references(val, agent_names, schema_names, line, column, errors);
                }
            }
            _ => {}
        }
    }

    fn check_reference(
        reference: &Reference,
        agent_names: &HashSet<String>,
        schema_names: &HashSet<String>,
        line: usize,
        column: usize,
        errors: &mut Vec<ValidationError>,
    ) {
        match reference {
            Reference::Agent { agent, .. } => {
                if !agent_names.contains(agent) {
                    errors.push(ValidationError::UndefinedReference {
                        file_path: "workflow".to_string(),
                        line,
                        column,
                        reference: agent.clone(),
                        suggestion: Some(format!("Define an agent named '{}'", agent)),
                    });
                }
            }
            Reference::AgentOutput { agent } => {
                if !agent_names.contains(agent) {
                    errors.push(ValidationError::UndefinedReference {
                        file_path: "workflow".to_string(),
                        line,
                        column,
                        reference: agent.clone(),
                        suggestion: Some(format!("Define an agent named '{}'", agent)),
                    });
                }
            }
            Reference::AgentContext { agent } => {
                if !agent_names.contains(agent) {
                    errors.push(ValidationError::UndefinedReference {
                        file_path: "workflow".to_string(),
                        line,
                        column,
                        reference: agent.clone(),
                        suggestion: Some(format!("Define an agent named '{}'", agent)),
                    });
                }
            }
            Reference::Schema { name } => {
                if !schema_names.contains(name) {
                    errors.push(ValidationError::UndefinedReference {
                        file_path: "workflow".to_string(),
                        line,
                        column,
                        reference: name.clone(),
                        suggestion: Some(format!("Define a schema named '{}'", name)),
                    });
                }
            }
            Reference::Input { .. } => {}
        }
    }

    fn check_provider_model_references(workflow: &Workflow, errors: &mut Vec<ValidationError>) {
        let provider_models: HashMap<String, Vec<String>> = workflow
            .providers
            .iter()
            .map(|p| (p.name.clone(), p.models.clone()))
            .collect();

        for agent in &workflow.agents {
            for property in &agent.properties {
                if let AgentProperty::Model { value, span } = property {
                    if let Value::String(model_ref) | Value::Interpolated(model_ref) = value {
                        if let Some((provider_name, model_name)) = model_ref.split_once('/') {
                            if let Some(models) = provider_models.get(provider_name) {
                                if !models.contains(&model_name.to_string()) {
                                    errors.push(ValidationError::ProviderModelMismatch {
                                        file_path: "workflow".to_string(),
                                        line: span.line,
                                        column: span.column,
                                        message: format!(
                                            "Model '{}' not found in provider '{}'",
                                            model_name, provider_name
                                        ),
                                        suggestion: Some(format!("Available models: {}", models.join(", "))),
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
