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

    fn check_required_agent_properties(workflow: &Workflow, errors: &mut Vec<ValidationError>) {
        for agent in &workflow.agents {
            let has_model = agent
                .properties
                .iter()
                .any(|p| matches!(p, AgentProperty::Model { .. }));
            let has_prompt = agent
                .properties
                .iter()
                .any(|p| matches!(p, AgentProperty::Prompt { .. }));
            let _has_output = agent
                .properties
                .iter()
                .any(|p| matches!(p, AgentProperty::Output { .. }));

            if !has_model {
                errors.push(ValidationError::MissingRequiredProperty {
                    file_path: "workflow".to_string(),
                    line: agent.span.line,
                    column: agent.span.column,
                    agent_name: agent.name.clone(),
                    property_name: "model".to_string(),
                    suggestion: Some(format!("Add 'model <- \"provider/model\"' to agent '{}'", agent.name)),
                });
            }

            if !has_prompt {
                errors.push(ValidationError::MissingRequiredProperty {
                    file_path: "workflow".to_string(),
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
                    } else if parts.len() == 2 && parts[0] == "agent" {
                        if !agent_names.contains(parts[1]) {
                            errors.push(ValidationError::UndefinedReference {
                                file_path: "workflow".to_string(),
                                line,
                                column,
                                reference: parts[1].to_string(),
                                suggestion: Some(format!("Define an agent named '{}'", parts[1])),
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
                if let AgentProperty::Model {
                    value: Value::String(model_ref) | Value::Interpolated(model_ref),
                    span,
                } = property
                {
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

    fn check_agent_field_references(workflow: &Workflow, errors: &mut Vec<ValidationError>) {
        let agent_schemas: HashMap<String, Option<&crate::ast::Schema>> = workflow
            .agents
            .iter()
            .map(|agent| {
                let schema = agent.properties.iter().find_map(|prop| {
                    if let AgentProperty::Output { value, .. } = prop {
                        match value {
                            crate::ast::SchemaReference::Inline(schema) => Some(schema),
                            _ => None,
                        }
                    } else {
                        None
                    }
                });
                (agent.name.clone(), schema)
            })
            .collect();

        for agent in &workflow.agents {
            for property in &agent.properties {
                match property {
                    AgentProperty::Prompt { value, span } => {
                        Self::check_field_references_in_value(value, &agent_schemas, span.line, span.column, errors);
                    }
                    AgentProperty::Context { value, span } => {
                        Self::check_field_references_in_value(value, &agent_schemas, span.line, span.column, errors);
                    }
                    AgentProperty::ForEach { collection, span, .. } => {
                        Self::check_field_references_in_value(
                            collection,
                            &agent_schemas,
                            span.line,
                            span.column,
                            errors,
                        );
                    }
                    _ => {}
                }
            }
        }

        if let Some(output_block) = &workflow.output {
            for field in &output_block.fields {
                Self::check_field_references_in_value(
                    &field.value,
                    &agent_schemas,
                    output_block.span.line,
                    output_block.span.column,
                    errors,
                );
            }
        }
    }

    fn check_field_references_in_value(
        value: &Value,
        agent_schemas: &HashMap<String, Option<&crate::ast::Schema>>,
        line: usize,
        column: usize,
        errors: &mut Vec<ValidationError>,
    ) {
        match value {
            Value::Reference(Reference::Agent { agent, field }) => {
                if let Some(Some(schema)) = agent_schemas.get(agent) {
                    let field_exists = schema.fields.iter().any(|f| f.name == *field);
                    if !field_exists {
                        errors.push(ValidationError::UndefinedReference {
                            file_path: "workflow".to_string(),
                            line,
                            column,
                            reference: format!("{}.{}", agent, field),
                            suggestion: Some(format!(
                                "Agent '{}' has an output schema, but field '{}' does not exist. Available fields: {}",
                                agent,
                                field,
                                schema
                                    .fields
                                    .iter()
                                    .map(|f| f.name.as_str())
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            )),
                        });
                    }
                }
            }
            Value::Interpolated(template) => {
                let interpolation_pattern = regex::Regex::new(r"\{\{\s*agent\.([^.}]+)\.([^}\s]+)\s*\}\}").unwrap();

                for capture in interpolation_pattern.captures_iter(template) {
                    let agent_name = capture[1].trim();
                    let field_name = capture[2].trim();

                    if let Some(schema_opt) = agent_schemas.get(agent_name) {
                        if let Some(schema) = schema_opt {
                            let field_exists = schema.fields.iter().any(|f| f.name == field_name);
                            if !field_exists {
                                errors.push(ValidationError::UndefinedReference {
                                    file_path: "workflow".to_string(),
                                    line,
                                    column,
                                    reference: format!("agent.{}.{}", agent_name, field_name),
                                    suggestion: Some(format!(
                                        "Agent '{}' has an output schema, but field '{}' does not exist. Available fields: {}",
                                        agent_name,
                                        field_name,
                                        schema.fields.iter().map(|f| f.name.as_str()).collect::<Vec<_>>().join(", ")
                                    )),
                                });
                            }
                        } else {
                            errors.push(ValidationError::UndefinedReference {
                                file_path: "workflow".to_string(),
                                line,
                                column,
                                reference: format!("agent.{}.{}", agent_name, field_name),
                                suggestion: Some(format!(
                                    "Agent '{}' does not have an output schema. You can only reference the entire agent output using '{{{{ agent.{} }}}}'",
                                    agent_name, agent_name
                                )),
                            });
                        }
                    }
                }
            }
            Value::Array(values) => {
                for val in values {
                    Self::check_field_references_in_value(val, agent_schemas, line, column, errors);
                }
            }
            Value::Object(map) => {
                for val in map.values() {
                    Self::check_field_references_in_value(val, agent_schemas, line, column, errors);
                }
            }
            Value::FunctionCall(func_call) => {
                for val in func_call.arguments.values() {
                    Self::check_field_references_in_value(val, agent_schemas, line, column, errors);
                }
            }
            _ => {}
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
                            let template_pattern =
                                regex::Regex::new(r"\{\{\s*([a-zA-Z_][a-zA-Z0-9_]*)\s*\}\}").unwrap();

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
                                        file_path: "workflow".to_string(),
                                        line: func_call.span.line,
                                        column: func_call.span.column,
                                        variable: template_var.clone(),
                                        suggestion: Some(format!(
                                            "Add binding for '{}' in the file function call",
                                            template_var
                                        )),
                                    });
                                }
                            }

                            for binding in &provided_bindings {
                                if !template_vars.contains(binding) {
                                    errors.push(ValidationError::UnusedTemplateBinding {
                                        file_path: "workflow".to_string(),
                                        line: func_call.span.line,
                                        column: func_call.span.column,
                                        binding: binding.clone(),
                                        suggestion: Some(format!(
                                            "Remove unused binding '{}' or add '{{{{ {} }}}}' to the template file",
                                            binding, binding
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
            Value::FunctionCall(function_call) => {
                if function_call.name == "compact" {
                    if !function_call.arguments.contains_key("model") {
                        errors.push(ValidationError::MissingRequiredArgument {
                            file_path: "workflow".to_string(),
                            line: function_call.span.line,
                            column: function_call.span.column,
                            function_name: "compact".to_string(),
                            argument_name: "model".to_string(),
                            suggestion: Some(
                                "Add model <- \"provider/model_name\" to the compact function".to_string(),
                            ),
                        });
                    }

                    if !function_call.arguments.contains_key("context") {
                        errors.push(ValidationError::MissingRequiredArgument {
                            file_path: "workflow".to_string(),
                            line: function_call.span.line,
                            column: function_call.span.column,
                            function_name: "compact".to_string(),
                            argument_name: "context".to_string(),
                            suggestion: Some("Add context <- agent.name.context to the compact function".to_string()),
                        });
                    }

                    if let Some(Value::String(model_ref) | Value::Interpolated(model_ref)) =
                        function_call.arguments.get("model")
                    {
                        if let Some((provider_name, model_name)) = model_ref.split_once('/') {
                            let provider_exists = providers.iter().any(|p| p.name == provider_name);
                            if !provider_exists {
                                errors.push(ValidationError::UndefinedReference {
                                    file_path: "workflow".to_string(),
                                    line: function_call.span.line,
                                    column: function_call.span.column,
                                    reference: provider_name.to_string(),
                                    suggestion: Some(format!("Provider '{}' is not defined", provider_name)),
                                });
                            } else {
                                let provider = providers.iter().find(|p| p.name == provider_name).unwrap();
                                if !provider.models.contains(&model_name.to_string()) {
                                    errors.push(ValidationError::ProviderModelMismatch {
                                        file_path: "workflow".to_string(),
                                        line: function_call.span.line,
                                        column: function_call.span.column,
                                        message: format!(
                                            "Model '{}' not found in provider '{}'",
                                            model_name, provider_name
                                        ),
                                        suggestion: Some(format!("Available models: {}", provider.models.join(", "))),
                                    });
                                }
                            }
                        }
                    }

                    for arg_value in function_call.arguments.values() {
                        Self::validate_compact_in_value(arg_value, providers, errors);
                    }
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
