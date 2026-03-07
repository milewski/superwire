use crate::ast::*;
use anyhow::{Result, anyhow};
use std::collections::{HashMap, HashSet};

pub fn validate_document(doc: &Document) -> Result<()> {
    validate_unique_names(doc)?;
    validate_references(doc)?;
    validate_providers(doc)?;
    validate_terminal_agents(doc)?;
    Ok(())
}

fn validate_unique_names(doc: &Document) -> Result<()> {
    // Check for duplicate agent names
    let agent_names: Vec<&String> = doc.agents.keys().collect();
    let unique_agents: HashSet<&String> = agent_names.iter().copied().collect();
    if agent_names.len() != unique_agents.len() {
        return Err(anyhow!("Duplicate agent names found"));
    }

    // Check for duplicate schema names
    let schema_names: Vec<&String> = doc.schemas.keys().collect();
    let unique_schemas: HashSet<&String> = schema_names.iter().copied().collect();
    if schema_names.len() != unique_schemas.len() {
        return Err(anyhow!("Duplicate schema names found"));
    }

    // Check for duplicate provider names
    let provider_names: Vec<&String> = doc.providers.keys().collect();
    let unique_providers: HashSet<&String> = provider_names.iter().copied().collect();
    if provider_names.len() != unique_providers.len() {
        return Err(anyhow!("Duplicate provider names found"));
    }

    Ok(())
}

fn validate_references(doc: &Document) -> Result<()> {
    for (agent_name, agent) in &doc.agents {
        // Validate schema references
        if let Some(SchemaRef::Named(schema_name)) = &agent.output {
            if !doc.schemas.contains_key(schema_name) {
                return Err(anyhow!("Agent '{}' references undefined schema '{}'", agent_name, schema_name));
            }
        }

        // Validate context references
        if let Some(context_ref) = &agent.context {
            let referenced_agent = match context_ref {
                ContextRef::Full(name) | ContextRef::Summary(name) => name,
            };
            if !doc.agents.contains_key(referenced_agent) {
                return Err(anyhow!("Agent '{}' references undefined agent '{}' in context", agent_name, referenced_agent));
            }
        }

        // Validate model/provider references
        if let Some(model_ref) = &agent.model {
            validate_model_reference(model_ref, doc)?;
        }
    }

    Ok(())
}

fn validate_model_reference(model_ref: &str, doc: &Document) -> Result<()> {
    let parts: Vec<&str> = model_ref.split('/').collect();
    if parts.len() != 2 {
        return Err(anyhow!("Invalid model reference format: '{}'. Expected 'provider/model'", model_ref));
    }

    let provider_name = parts[0];
    let model_name = parts[1];

    let provider = doc.providers.get(provider_name)
        .ok_or_else(|| anyhow!("Model reference '{}' uses undefined provider '{}'", model_ref, provider_name))?;

    if !provider.models.contains(&model_name.to_string()) {
        return Err(anyhow!("Provider '{}' does not declare model '{}'", provider_name, model_name));
    }

    Ok(())
}

fn validate_providers(doc: &Document) -> Result<()> {
    for (name, provider) in &doc.providers {
        if provider.driver.is_empty() {
            return Err(anyhow!("Provider '{}' has empty driver", name));
        }
        if provider.api_endpoint.is_empty() {
            return Err(anyhow!("Provider '{}' has empty api_endpoint", name));
        }
        if provider.models.is_empty() {
            return Err(anyhow!("Provider '{}' has no models declared", name));
        }
    }
    Ok(())
}

fn validate_terminal_agents(doc: &Document) -> Result<()> {
    let terminal_count = doc.agents.values().filter(|a| a.is_terminal).count();
    if terminal_count == 0 {
        return Err(anyhow!("No terminal agents declared. At least one agent must be marked with '<-'"));
    }
    Ok(())
}

pub fn validate_template_variables(template: &str, provided_vars: &HashSet<String>) -> Result<()> {
    let mut required_vars = HashSet::new();

    // Simple regex-like parsing for {{ variable_name }}
    let mut chars = template.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '{' {
            if let Some(&'{') = chars.peek() {
                chars.next(); // consume second {
                let mut var_name = String::new();
                while let Some(c) = chars.next() {
                    if c == '}' {
                        if let Some(&'}') = chars.peek() {
                            chars.next(); // consume second }
                            required_vars.insert(var_name.trim().to_string());
                            break;
                        }
                    }
                    var_name.push(c);
                }
            }
        }
    }

    // Check for missing variables
    for var in &required_vars {
        if !provided_vars.contains(var) {
            return Err(anyhow!("Template variable '{}' is not provided", var));
        }
    }

    // Check for unused variables
    for var in provided_vars {
        if !required_vars.contains(var) {
            return Err(anyhow!("Provided variable '{}' is not used in template", var));
        }
    }

    Ok(())
}
