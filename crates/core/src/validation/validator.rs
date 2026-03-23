use crate::ast::{Agent, AgentProperty, Reference, SchemaType, Value, Workflow};
use crate::validation::error::ValidationError;
use regex::Regex;
use std::collections::{HashMap, HashSet};

static INTERPOLATION_PATTERN: std::sync::LazyLock<Regex> =
    std::sync::LazyLock::new(|| Regex::new(r"\{\{([^}]+)\}\}").expect("Invalid regex pattern"));

const WORKFLOW_FILE_PATH: &str = "workflow";

#[derive(Debug)]
enum AgentOutputType<'a> {
    InlineSchema(&'a crate::ast::Schema),
    InlineType(&'a SchemaType),
    Named(&'a str),
    None,
}

pub struct WorkflowValidator;

impl WorkflowValidator {
    pub fn validate(workflow: &Workflow) -> Result<(), Vec<ValidationError>> {
        let mut errors = Vec::new();

        Self::check_duplicate_agent_names(workflow, &mut errors);
        Self::check_duplicate_schema_names(workflow, &mut errors);
        Self::check_duplicate_provider_names(workflow, &mut errors);
        Self::check_required_agent_properties(workflow, &mut errors);
        Self::check_undefined_references(workflow, &mut errors);
        Self::check_provider_model_references(workflow, &mut errors);
        Self::check_agent_field_references(workflow, &mut errors);
        Self::check_template_variables(workflow, &mut errors);
        Self::check_compact_function_calls(workflow, &mut errors);

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    fn check_duplicate_agent_names(workflow: &Workflow, errors: &mut Vec<ValidationError>) {
        Self::check_duplicates(&workflow.agents, |agent| &agent.name, |agent| &agent.span, "agent", errors);
    }

    fn check_duplicate_schema_names(workflow: &Workflow, errors: &mut Vec<ValidationError>) {
        Self::check_duplicates(&workflow.schemas, |schema| &schema.name, |schema| &schema.span, "schema", errors);
    }

    fn check_duplicate_provider_names(workflow: &Workflow, errors: &mut Vec<ValidationError>) {
        Self::check_duplicates(
            &workflow.providers,
            |provider| &provider.name,
            |provider| &provider.span,
            "provider",
            errors,
        );
    }

    fn check_duplicates<T>(
        collection: &[T],
        name_getter: impl Fn(&T) -> &String,
        span_getter: impl Fn(&T) -> &crate::ast::Span,
        item_type: &str,
        errors: &mut Vec<ValidationError>,
    ) {
        let mut seen = HashMap::new();

        for item in collection {
            let name = name_getter(item);
            let span = span_getter(item);

            if let Some(first_location) = seen.get(name) {
                errors.push(ValidationError::DuplicateName {
                    file_path: WORKFLOW_FILE_PATH.to_string(),
                    line: span.line,
                    column: span.column,
                    name: name.clone(),
                    first_defined_at: format!("line {first_location}"),
                    suggestion: Some(format!("Rename one of the '{name}' {item_type}s")),
                });
            } else {
                seen.insert(name.clone(), span.line);
            }
        }
    }

    fn check_required_agent_properties(workflow: &Workflow, errors: &mut Vec<ValidationError>) {
        for agent in &workflow.agents {
            let has_model = agent.properties.iter().any(|p| matches!(p, AgentProperty::Model { .. }));
            let has_prompt = agent.properties.iter().any(|p| matches!(p, AgentProperty::Prompt { .. }));
            let _has_output = agent.properties.iter().any(|p| matches!(p, AgentProperty::Output { .. }));

            if !has_model {
                errors.push(ValidationError::MissingRequiredProperty {
                    file_path: WORKFLOW_FILE_PATH.to_string(),
                    line: agent.span.line,
                    column: agent.span.column,
                    agent_name: agent.name.clone(),
                    property_name: "model".to_string(),
                    suggestion: Some(format!("Add 'model <- \"provider/model\"' to agent '{}'", agent.name)),
                });
            }

            if !has_prompt {
                errors.push(ValidationError::MissingRequiredProperty {
                    file_path: WORKFLOW_FILE_PATH.to_string(),
                    line: agent.span.line,
                    column: agent.span.column,
                    agent_name: agent.name.clone(),
                    property_name: "prompt".to_string(),
                    suggestion: Some(format!("Add 'prompt <- \"...\"' to agent '{}'", agent.name)),
                });
            }
        }
    }

    fn check_undefined_references(workflow: &Workflow, errors: &mut Vec<ValidationError>) {
        let agent_names: HashSet<&str> = workflow.agents.iter().map(|a| a.name.as_str()).collect();
        let schema_names: HashSet<&str> = workflow.schemas.iter().map(|s| s.name.as_str()).collect();

        for agent in &workflow.agents {
            Self::check_agent_references(agent, &agent_names, &schema_names, errors);
        }
    }

    fn check_agent_references(agent: &Agent, agent_names: &HashSet<&str>, schema_names: &HashSet<&str>, errors: &mut Vec<ValidationError>) {
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
        agent_names: &HashSet<&str>,
        schema_names: &HashSet<&str>,
        line: usize,
        column: usize,
        errors: &mut Vec<ValidationError>,
    ) {
        match value {
            Value::Reference(reference) => {
                Self::check_reference(reference, agent_names, schema_names, line, column, errors);
            }
            Value::Interpolated(template) => {
                for capture in INTERPOLATION_PATTERN.captures_iter(template) {
                    let reference_text = capture[1].trim();
                    let parts: Vec<&str> = reference_text.split('.').collect();

                    if parts.len() == 1 && parts[0] != "input" {
                        if !agent_names.contains(parts[0]) {
                            errors.push(ValidationError::UndefinedReference {
                                file_path: WORKFLOW_FILE_PATH.to_string(),
                                line,
                                column,
                                reference: parts[0].to_string(),
                                suggestion: Some(format!("Define an agent named '{}'", parts[0])),
                            });
                        }
                    } else if parts.len() == 2 && parts[0] == "agent" {
                        if !agent_names.contains(parts[1]) {
                            errors.push(ValidationError::UndefinedReference {
                                file_path: WORKFLOW_FILE_PATH.to_string(),
                                line,
                                column,
                                reference: parts[1].to_string(),
                                suggestion: Some(format!("Define an agent named '{}'", parts[1])),
                            });
                        }
                    } else if parts.len() == 2 && parts[0] != "input" && !agent_names.contains(parts[0]) {
                        errors.push(ValidationError::UndefinedReference {
                            file_path: WORKFLOW_FILE_PATH.to_string(),
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
        agent_names: &HashSet<&str>,
        schema_names: &HashSet<&str>,
        line: usize,
        column: usize,
        errors: &mut Vec<ValidationError>,
    ) {
        match reference {
            Reference::Agent { agent, .. } => {
                if !agent_names.contains(agent.as_str()) {
                    errors.push(ValidationError::UndefinedReference {
                        file_path: WORKFLOW_FILE_PATH.to_string(),
                        line,
                        column,
                        reference: agent.clone(),
                        suggestion: Some(format!("Define an agent named '{agent}'")),
                    });
                }
            }
            Reference::AgentOutput { agent } => {
                if !agent_names.contains(agent.as_str()) {
                    errors.push(ValidationError::UndefinedReference {
                        file_path: WORKFLOW_FILE_PATH.to_string(),
                        line,
                        column,
                        reference: agent.clone(),
                        suggestion: Some(format!("Define an agent named '{agent}'")),
                    });
                }
            }
            Reference::AgentContext { agent } => {
                if !agent_names.contains(agent.as_str()) {
                    errors.push(ValidationError::UndefinedReference {
                        file_path: WORKFLOW_FILE_PATH.to_string(),
                        line,
                        column,
                        reference: agent.clone(),
                        suggestion: Some(format!("Define an agent named '{agent}'")),
                    });
                }
            }
            Reference::Schema { name } => {
                if !schema_names.contains(name.as_str()) {
                    errors.push(ValidationError::UndefinedReference {
                        file_path: WORKFLOW_FILE_PATH.to_string(),
                        line,
                        column,
                        reference: name.clone(),
                        suggestion: Some(format!("Define a schema named '{name}'")),
                    });
                }
            }
            Reference::Input { .. } => {}
            Reference::Tool { .. } => {}
        }
    }

    fn check_provider_model_references(workflow: &Workflow, errors: &mut Vec<ValidationError>) {
        let provider_models: HashMap<&str, &Vec<String>> = workflow.providers.iter().map(|p| (p.name.as_str(), &p.models)).collect();

        for agent in &workflow.agents {
            for property in &agent.properties {
                if let AgentProperty::Model {
                    value: Value::String(model_ref) | Value::Interpolated(model_ref),
                    span,
                } = property
                {
                    if let Some((provider_name, model_name)) = model_ref.split_once('/') {
                        if let Some(models) = provider_models.get(provider_name) {
                            if !models.contains(&model_name.to_string()) {
                                errors.push(ValidationError::ProviderModelMismatch {
                                    file_path: WORKFLOW_FILE_PATH.to_string(),
                                    line: span.line,
                                    column: span.column,
                                    message: format!("Model '{model_name}' not found in provider '{provider_name}'"),
                                    suggestion: Some(format!("Available models: {}", models.join(", "))),
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    fn check_agent_field_references(workflow: &Workflow, errors: &mut Vec<ValidationError>) {
        let agent_output_types: HashMap<&str, AgentOutputType> = workflow
            .agents
            .iter()
            .map(|agent| {
                let output_type = agent.properties.iter().find_map(|prop| {
                    if let AgentProperty::Output { value, .. } = prop {
                        match value {
                            crate::ast::SchemaReference::Inline(schema) => Some(AgentOutputType::InlineSchema(schema)),
                            crate::ast::SchemaReference::InlineType { schema_type, .. } => Some(AgentOutputType::InlineType(schema_type)),
                            crate::ast::SchemaReference::Named(name) => Some(AgentOutputType::Named(name)),
                        }
                    } else {
                        None
                    }
                });
                (agent.name.as_str(), output_type.unwrap_or(AgentOutputType::None))
            })
            .collect();

        for agent in &workflow.agents {
            for property in &agent.properties {
                match property {
                    AgentProperty::Prompt { value, span } => {
                        Self::check_field_references_in_value(value, &agent_output_types, span.line, span.column, errors);
                    }
                    AgentProperty::Context { value, span } => {
                        Self::check_field_references_in_value(value, &agent_output_types, span.line, span.column, errors);
                    }
                    AgentProperty::ForEach { collection, span, .. } => {
                        Self::check_field_references_in_value(collection, &agent_output_types, span.line, span.column, errors);
                    }
                    _ => {}
                }
            }
        }

        if let Some(output_block) = &workflow.output {
            for field in &output_block.fields {
                Self::check_field_references_in_value(
                    &field.value,
                    &agent_output_types,
                    output_block.span.line,
                    output_block.span.column,
                    errors,
                );
            }
        }
    }

    #[allow(clippy::too_many_lines)]
    fn check_field_references_in_value(
        value: &Value,
        agent_output_types: &HashMap<&str, AgentOutputType<'_>>,
        line: usize,
        column: usize,
        errors: &mut Vec<ValidationError>,
    ) {
        match value {
            Value::Reference(Reference::Agent { agent, field }) => {
                if let Some(output_type) = agent_output_types.get(agent.as_str()) {
                    match output_type {
                        AgentOutputType::InlineSchema(schema) => {
                            let field_exists = schema.fields.iter().any(|f| f.name == *field);
                            if !field_exists {
                                errors.push(ValidationError::UndefinedReference {
                                    file_path: WORKFLOW_FILE_PATH.to_string(),
                                    line,
                                    column,
                                    reference: format!("{agent}.{field}"),
                                    suggestion: Some(format!(
                                        "Agent '{}' has an output schema, but field '{}' does not exist. Available fields: {}",
                                        agent,
                                        field,
                                        schema.fields.iter().map(|f| f.name.as_str()).collect::<Vec<_>>().join(", ")
                                    )),
                                });
                            }
                        }
                        AgentOutputType::InlineType(schema_type) => {
                            // InlineType outputs (like [string], number, etc.) cannot have field access
                            let type_description = Self::describe_schema_type(schema_type);
                            errors.push(ValidationError::UndefinedReference {
                                file_path: WORKFLOW_FILE_PATH.to_string(),
                                line,
                                column,
                                reference: format!("{agent}.{field}"),
                                suggestion: Some(format!(
                                    "Agent '{agent}' has output type '{type_description}' which does not support field access. You can only reference the entire output using 'agent.{agent}'"
                                )),
                            });
                        }
                        AgentOutputType::Named(_name) => {
                            // For named schemas, we cannot validate at parse time without resolving the schema
                            // This would require looking up the schema definition, which we skip for now
                        }
                        AgentOutputType::None => {
                            errors.push(ValidationError::UndefinedReference {
                                file_path: WORKFLOW_FILE_PATH.to_string(),
                                line,
                                column,
                                reference: format!("{agent}.{field}"),
                                suggestion: Some(format!(
                                    "Agent '{agent}' does not have an output schema. You can only reference the entire agent output using 'agent.{agent}'"
                                )),
                            });
                        }
                    }
                }
            }
            Value::Interpolated(template) => {
                let Ok(interpolation_pattern) = regex::Regex::new(r"\{\{\s*agent\.([^.}]+)\.([^}\s]+)\s*\}\}") else {
                    return; // Skip this template if regex compilation fails
                };

                for capture in interpolation_pattern.captures_iter(template) {
                    let agent_name = capture[1].trim();
                    let field_name = capture[2].trim();

                    if let Some(output_type) = agent_output_types.get(agent_name) {
                        match output_type {
                            AgentOutputType::InlineSchema(schema) => {
                                let field_exists = schema.fields.iter().any(|f| f.name == field_name);
                                if !field_exists {
                                    errors.push(ValidationError::UndefinedReference {
                                        file_path: WORKFLOW_FILE_PATH.to_string(),
                                        line,
                                        column,
                                        reference: format!("agent.{agent_name}.{field_name}"),
                                        suggestion: Some(format!(
                                            "Agent '{}' has an output schema, but field '{}' does not exist. Available fields: {}",
                                            agent_name,
                                            field_name,
                                            schema.fields.iter().map(|f| f.name.as_str()).collect::<Vec<_>>().join(", ")
                                        )),
                                    });
                                }
                            }
                            AgentOutputType::InlineType(schema_type) => {
                                let type_description = Self::describe_schema_type(schema_type);
                                errors.push(ValidationError::UndefinedReference {
                                    file_path: WORKFLOW_FILE_PATH.to_string(),
                                    line,
                                    column,
                                    reference: format!("agent.{agent_name}.{field_name}"),
                                    suggestion: Some(format!(
                                        "Agent '{agent_name}' has output type '{type_description}' which does not support field access. You can only reference the entire output using '{{{{ agent.{agent_name} }}}}'"
                                    )),
                                });
                            }
                            AgentOutputType::Named(_name) => {
                                // Skip validation for named schemas
                            }
                            AgentOutputType::None => {
                                errors.push(ValidationError::UndefinedReference {
                                    file_path: WORKFLOW_FILE_PATH.to_string(),
                                    line,
                                    column,
                                    reference: format!("agent.{agent_name}.{field_name}"),
                                    suggestion: Some(format!(
                                        "Agent '{agent_name}' does not have an output schema. You can only reference the entire agent output using '{{{{ agent.{agent_name} }}}}'"
                                    )),
                                });
                            }
                        }
                    }
                }
            }
            Value::Array(values) => {
                for val in values {
                    Self::check_field_references_in_value(val, agent_output_types, line, column, errors);
                }
            }
            Value::Object(map) => {
                for val in map.values() {
                    Self::check_field_references_in_value(val, agent_output_types, line, column, errors);
                }
            }
            Value::FunctionCall(func_call) => {
                for val in func_call.arguments.values() {
                    Self::check_field_references_in_value(val, agent_output_types, line, column, errors);
                }
            }
            _ => {}
        }
    }

    fn describe_schema_type(schema_type: &SchemaType) -> String {
        match schema_type {
            SchemaType::String => "string".to_string(),
            SchemaType::Number => "number".to_string(),
            SchemaType::Float => "float".to_string(),
            SchemaType::Boolean => "boolean".to_string(),
            SchemaType::Null => "null".to_string(),
            SchemaType::Array(inner, quantity) => {
                let inner_desc = Self::describe_schema_type(inner);
                if let Some(quantity) = quantity {
                    format!("[{inner_desc}; {quantity}]")
                } else {
                    format!("[{inner_desc}]")
                }
            }
            SchemaType::Enum(values) => {
                format!("enum({})", values.join(" | "))
            }
            SchemaType::Object(fields) => {
                if fields.is_empty() {
                    "object".to_string()
                } else {
                    let field_names: Vec<_> = fields.iter().map(|f| f.name.as_str()).collect();
                    format!("object {{ {} }}", field_names.join(", "))
                }
            }
        }
    }

    fn check_template_variables(workflow: &Workflow, errors: &mut Vec<ValidationError>) {
        for agent in &workflow.agents {
            for property in &agent.properties {
                if let AgentProperty::Prompt { value, .. } = property {
                    Self::check_template_in_value(value, errors);
                }
            }
        }

        if let Some(output_block) = &workflow.output {
            for field in &output_block.fields {
                Self::check_template_in_value(&field.value, errors);
            }
        }
    }

    fn check_template_in_value(value: &Value, errors: &mut Vec<ValidationError>) {
        match value {
            Value::FunctionCall(func_call) => {
                if func_call.name == "file" {
                    if let Some(Value::String(path)) = func_call.arguments.get("path") {
                        if let Ok(content) = std::fs::read_to_string(path) {
                            let Ok(template_pattern) = regex::Regex::new(r"\{\{\s*([a-zA-Z_][a-zA-Z0-9_]*)\s*\}\}") else {
                                return; // Skip template validation if regex compilation fails
                            };

                            let mut template_vars = HashSet::new();
                            for capture in template_pattern.captures_iter(&content) {
                                template_vars.insert(capture[1].to_string());
                            }

                            let mut provided_bindings = HashSet::new();
                            for key in func_call.arguments.keys() {
                                if key != "path" {
                                    provided_bindings.insert(key.clone());
                                }
                            }

                            for template_var in &template_vars {
                                if !provided_bindings.contains(template_var) {
                                    errors.push(ValidationError::MissingTemplateVariable {
                                        file_path: WORKFLOW_FILE_PATH.to_string(),
                                        line: func_call.span.line,
                                        column: func_call.span.column,
                                        variable: template_var.clone(),
                                        suggestion: Some(format!("Add binding for '{template_var}' in the file function call")),
                                    });
                                }
                            }

                            for binding in &provided_bindings {
                                if !template_vars.contains(binding) {
                                    errors.push(ValidationError::UnusedTemplateBinding {
                                        file_path: WORKFLOW_FILE_PATH.to_string(),
                                        line: func_call.span.line,
                                        column: func_call.span.column,
                                        binding: binding.clone(),
                                        suggestion: Some(format!(
                                            "Remove unused binding '{binding}' or add '{{{{ {binding} }}}}' to the template file"
                                        )),
                                    });
                                }
                            }
                        }
                    }
                }

                for val in func_call.arguments.values() {
                    Self::check_template_in_value(val, errors);
                }
            }
            Value::Array(values) => {
                for val in values {
                    Self::check_template_in_value(val, errors);
                }
            }
            Value::Object(map) => {
                for val in map.values() {
                    Self::check_template_in_value(val, errors);
                }
            }
            _ => {}
        }
    }

    fn check_compact_function_calls(workflow: &Workflow, errors: &mut Vec<ValidationError>) {
        if let Some(output_block) = &workflow.output {
            for field in &output_block.fields {
                Self::validate_compact_in_value(&field.value, &workflow.providers, errors);
            }
        }

        for agent in &workflow.agents {
            for property in &agent.properties {
                if let AgentProperty::Context { value, .. } = property {
                    Self::validate_compact_in_value(value, &workflow.providers, errors);
                }
            }
        }
    }

    fn validate_compact_in_value(value: &Value, providers: &[crate::ast::Provider], errors: &mut Vec<ValidationError>) {
        match value {
            Value::FunctionCall(function_call) if function_call.name == "compact" => {
                if !function_call.arguments.contains_key("model") {
                    errors.push(ValidationError::MissingRequiredArgument {
                        file_path: WORKFLOW_FILE_PATH.to_string(),
                        line: function_call.span.line,
                        column: function_call.span.column,
                        function_name: "compact".to_string(),
                        argument_name: "model".to_string(),
                        suggestion: Some("Add model <- \"provider/model_name\" to the compact function".to_string()),
                    });
                }

                if !function_call.arguments.contains_key("context") {
                    errors.push(ValidationError::MissingRequiredArgument {
                        file_path: WORKFLOW_FILE_PATH.to_string(),
                        line: function_call.span.line,
                        column: function_call.span.column,
                        function_name: "compact".to_string(),
                        argument_name: "context".to_string(),
                        suggestion: Some("Add context <- agent.name.context to the compact function".to_string()),
                    });
                }

                if let Some(Value::String(model_ref) | Value::Interpolated(model_ref)) = function_call.arguments.get("model") {
                    if let Some((provider_name, model_name)) = model_ref.split_once('/') {
                        let provider_exists = providers.iter().any(|p| p.name == provider_name);
                        if provider_exists {
                            if let Some(provider) = providers.iter().find(|p| p.name == provider_name) {
                                if !provider.models.contains(&model_name.to_string()) {
                                    errors.push(ValidationError::ProviderModelMismatch {
                                        file_path: WORKFLOW_FILE_PATH.to_string(),
                                        line: function_call.span.line,
                                        column: function_call.span.column,
                                        message: format!("Model '{model_name}' not found in provider '{provider_name}'"),
                                        suggestion: Some(format!("Available models: {}", provider.models.join(", "))),
                                    });
                                }
                            }
                        } else {
                            errors.push(ValidationError::UndefinedReference {
                                file_path: WORKFLOW_FILE_PATH.to_string(),
                                line: function_call.span.line,
                                column: function_call.span.column,
                                reference: provider_name.to_string(),
                                suggestion: Some(format!("Provider '{provider_name}' is not defined")),
                            });
                        }
                    }
                }

                for arg_value in function_call.arguments.values() {
                    Self::validate_compact_in_value(arg_value, providers, errors);
                }
            }
            Value::Array(items) => {
                for item in items {
                    Self::validate_compact_in_value(item, providers, errors);
                }
            }
            Value::Object(map) => {
                for val in map.values() {
                    Self::validate_compact_in_value(val, providers, errors);
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{Agent, AgentProperty, OutputBlock, OutputField, SchemaReference, SchemaType, Span, Value, Workflow};

    fn create_test_span() -> Span {
        Span {
            start: 0,
            end: 0,
            line: 1,
            column: 1,
        }
    }

    #[test]
    fn test_inline_type_field_access_error() {
        let workflow = Workflow {
            providers: vec![],
            schemas: vec![],
            agents: vec![Agent {
                name: "file_explorer".to_string(),
                is_terminal: false,
                properties: vec![AgentProperty::Output {
                    value: SchemaReference::InlineType {
                        schema_type: SchemaType::Array(Box::new(SchemaType::String), None),
                        description: None,
                    },
                    span: create_test_span(),
                }],
                span: create_test_span(),
            }],
            input: None,
            output: Some(OutputBlock {
                fields: vec![OutputField {
                    name: "files".to_string(),
                    value: Value::Reference(Reference::Agent {
                        agent: "file_explorer".to_string(),
                        field: "example".to_string(),
                    }),
                    span: create_test_span(),
                }],
                span: create_test_span(),
            }),
            span: create_test_span(),
        };

        let result = WorkflowValidator::validate(&workflow);
        assert!(result.is_err());

        let errors = result.unwrap_err();
        eprintln!("Errors: {errors:#?}");

        // Find the specific error we're looking for
        let field_access_error = errors
            .iter()
            .find(|e| matches!(e, ValidationError::UndefinedReference { reference, .. } if reference == "file_explorer.example"));
        assert!(
            field_access_error.is_some(),
            "Expected UndefinedReference error for file_explorer.example"
        );

        match field_access_error.unwrap() {
            ValidationError::UndefinedReference { reference, suggestion, .. } => {
                assert_eq!(reference, "file_explorer.example");
                assert!(suggestion.as_ref().unwrap().contains("[string]"));
                assert!(suggestion.as_ref().unwrap().contains("does not support field access"));
            }
            _ => panic!("Expected UndefinedReference error"),
        }
    }

    #[test]
    fn test_inline_type_interpolation_field_access_error() {
        let workflow = Workflow {
            providers: vec![],
            schemas: vec![],
            agents: vec![Agent {
                name: "calculator".to_string(),
                is_terminal: false,
                properties: vec![AgentProperty::Output {
                    value: SchemaReference::InlineType {
                        schema_type: SchemaType::Number,
                        description: None,
                    },
                    span: create_test_span(),
                }],
                span: create_test_span(),
            }],
            input: None,
            output: Some(OutputBlock {
                fields: vec![OutputField {
                    name: "result".to_string(),
                    value: Value::Interpolated("The result is {{ agent.calculator.value }}".to_string()),
                    span: create_test_span(),
                }],
                span: create_test_span(),
            }),
            span: create_test_span(),
        };

        let result = WorkflowValidator::validate(&workflow);
        assert!(result.is_err());

        let errors = result.unwrap_err();

        // Find the specific error we're looking for
        let field_access_error = errors
            .iter()
            .find(|e| matches!(e, ValidationError::UndefinedReference { reference, .. } if reference == "agent.calculator.value"));
        assert!(
            field_access_error.is_some(),
            "Expected UndefinedReference error for agent.calculator.value"
        );

        match field_access_error.unwrap() {
            ValidationError::UndefinedReference { reference, suggestion, .. } => {
                assert_eq!(reference, "agent.calculator.value");
                assert!(suggestion.as_ref().unwrap().contains("number"));
                assert!(suggestion.as_ref().unwrap().contains("does not support field access"));
            }
            _ => panic!("Expected UndefinedReference error"),
        }
    }

    #[test]
    fn test_no_output_schema_field_access_error() {
        let workflow = Workflow {
            providers: vec![],
            schemas: vec![],
            agents: vec![Agent {
                name: "simple_agent".to_string(),
                is_terminal: false,
                properties: vec![],
                span: create_test_span(),
            }],
            input: None,
            output: Some(OutputBlock {
                fields: vec![OutputField {
                    name: "data".to_string(),
                    value: Value::Reference(Reference::Agent {
                        agent: "simple_agent".to_string(),
                        field: "result".to_string(),
                    }),
                    span: create_test_span(),
                }],
                span: create_test_span(),
            }),
            span: create_test_span(),
        };

        let result = WorkflowValidator::validate(&workflow);
        assert!(result.is_err());

        let errors = result.unwrap_err();

        // Find the specific error we're looking for
        let field_access_error = errors
            .iter()
            .find(|e| matches!(e, ValidationError::UndefinedReference { reference, .. } if reference == "simple_agent.result"));
        assert!(
            field_access_error.is_some(),
            "Expected UndefinedReference error for simple_agent.result"
        );

        match field_access_error.unwrap() {
            ValidationError::UndefinedReference { reference, suggestion, .. } => {
                assert_eq!(reference, "simple_agent.result");
                assert!(suggestion.as_ref().unwrap().contains("does not have an output schema"));
            }
            _ => panic!("Expected UndefinedReference error"),
        }
    }

    #[test]
    fn test_valid_inline_schema_field_access() {
        let workflow = Workflow {
            providers: vec![],
            schemas: vec![],
            agents: vec![Agent {
                name: "data_agent".to_string(),
                is_terminal: false,
                properties: vec![
                    AgentProperty::Model {
                        value: Value::String("test/model".to_string()),
                        span: create_test_span(),
                    },
                    AgentProperty::Prompt {
                        value: Value::String("test prompt".to_string()),
                        span: create_test_span(),
                    },
                    AgentProperty::Output {
                        value: SchemaReference::Inline(crate::ast::Schema {
                            fields: vec![crate::ast::SchemaField {
                                name: "result".to_string(),
                                field_type: SchemaType::String,
                                description: None,
                                span: create_test_span(),
                            }],
                            span: create_test_span(),
                        }),
                        span: create_test_span(),
                    },
                ],
                span: create_test_span(),
            }],
            input: None,
            output: Some(OutputBlock {
                fields: vec![OutputField {
                    name: "data".to_string(),
                    value: Value::Reference(Reference::Agent {
                        agent: "data_agent".to_string(),
                        field: "result".to_string(),
                    }),
                    span: create_test_span(),
                }],
                span: create_test_span(),
            }),
            span: create_test_span(),
        };

        let result = WorkflowValidator::validate(&workflow);
        // The workflow should be valid since we're accessing a field that exists
        if let Err(errors) = result {
            eprintln!("Unexpected errors: {errors:#?}");
            panic!("Expected validation to pass");
        }
    }

    #[test]
    fn test_invalid_inline_schema_field_access() {
        let workflow = Workflow {
            providers: vec![],
            schemas: vec![],
            agents: vec![Agent {
                name: "data_agent".to_string(),
                is_terminal: false,
                properties: vec![
                    AgentProperty::Model {
                        value: Value::String("test/model".to_string()),
                        span: create_test_span(),
                    },
                    AgentProperty::Prompt {
                        value: Value::String("test prompt".to_string()),
                        span: create_test_span(),
                    },
                    AgentProperty::Output {
                        value: SchemaReference::Inline(crate::ast::Schema {
                            fields: vec![crate::ast::SchemaField {
                                name: "result".to_string(),
                                field_type: SchemaType::String,
                                description: None,
                                span: create_test_span(),
                            }],
                            span: create_test_span(),
                        }),
                        span: create_test_span(),
                    },
                ],
                span: create_test_span(),
            }],
            input: None,
            output: Some(OutputBlock {
                fields: vec![OutputField {
                    name: "data".to_string(),
                    value: Value::Reference(Reference::Agent {
                        agent: "data_agent".to_string(),
                        field: "nonexistent".to_string(),
                    }),
                    span: create_test_span(),
                }],
                span: create_test_span(),
            }),
            span: create_test_span(),
        };

        let result = WorkflowValidator::validate(&workflow);
        assert!(result.is_err());

        let errors = result.unwrap_err();

        // Find the specific error we're looking for
        let field_access_error = errors
            .iter()
            .find(|e| matches!(e, ValidationError::UndefinedReference { reference, .. } if reference == "data_agent.nonexistent"));
        assert!(
            field_access_error.is_some(),
            "Expected UndefinedReference error for data_agent.nonexistent"
        );

        match field_access_error.unwrap() {
            ValidationError::UndefinedReference { reference, suggestion, .. } => {
                assert_eq!(reference, "data_agent.nonexistent");
                assert!(suggestion.as_ref().unwrap().contains("Available fields: result"));
            }
            _ => panic!("Expected UndefinedReference error"),
        }
    }
}
