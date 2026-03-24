use engine_ai_core::dsl::{
    parse_workflow, AgentDeclaration, AgentProperty, CallArgument, Declaration, Expression, FunctionCall, NamedArgument, ObjectField,
    OutputDeclaration, ProviderDeclaration, Reference, StringTemplate, StringTemplatePart, TypeExpression, TypedField, Workflow,
};

pub fn format_source(source: &str) -> Result<String, engine_ai_core::dsl::DslParseError> {
    let workflow = parse_workflow(source)?;
    let workflow_formatter = WorkflowFormatter::new();

    Ok(workflow_formatter.format_workflow(&workflow))
}

#[derive(Debug, Default)]
struct WorkflowFormatter;

impl WorkflowFormatter {
    fn new() -> Self {
        Self
    }

    fn format_workflow(&self, workflow: &Workflow) -> String {
        let mut formatted_workflow = String::new();

        for (declaration_index, declaration) in workflow.declarations.iter().enumerate() {
            if declaration_index > 0 {
                formatted_workflow.push_str("\n\n");
            }

            formatted_workflow.push_str(&self.format_declaration(declaration));
        }

        formatted_workflow.push('\n');
        formatted_workflow
    }

    fn format_declaration(&self, declaration: &Declaration) -> String {
        match declaration {
            Declaration::Provider(provider_declaration) => self.format_provider_declaration(provider_declaration),
            Declaration::Secrets(secrets_declaration) => {
                format!("secrets {}", self.format_typed_fields_block(&secrets_declaration.fields, 0))
            }
            Declaration::Input(input_declaration) => format!("input {}", self.format_typed_fields_block(&input_declaration.fields, 0)),
            Declaration::Schema(schema_declaration) => format!(
                "schema {} {}",
                schema_declaration.name,
                self.format_typed_fields_block(&schema_declaration.fields, 0)
            ),
            Declaration::Agent(agent_declaration) => self.format_agent_declaration(agent_declaration),
            Declaration::Output(output_declaration) => self.format_output_declaration(output_declaration),
        }
    }

    fn format_provider_declaration(&self, provider_declaration: &ProviderDeclaration) -> String {
        format!(
            "provider {} {}",
            provider_declaration.name,
            self.format_object_fields_block(&provider_declaration.properties, 0)
        )
    }

    fn format_agent_declaration(&self, agent_declaration: &AgentDeclaration) -> String {
        let mut declaration_header = format!("agent {}", agent_declaration.name);

        if let Some(loop_declaration) = &agent_declaration.for_loop {
            declaration_header.push_str(" for ");
            declaration_header.push_str(&loop_declaration.iterator_name);
            declaration_header.push_str(" in ");
            declaration_header.push_str(&self.format_expression(&loop_declaration.iterable, 0));
        }

        declaration_header.push(' ');
        declaration_header.push_str(&self.format_agent_properties_block(&agent_declaration.properties, 0));
        declaration_header
    }

    fn format_output_declaration(&self, output_declaration: &OutputDeclaration) -> String {
        format!("output {}", self.format_object_fields_block(&output_declaration.fields, 0))
    }

    fn format_agent_properties_block(&self, agent_properties: &[AgentProperty], indentation_level: usize) -> String {
        let mut block_lines = Vec::with_capacity(agent_properties.len());

        for agent_property in agent_properties {
            block_lines.push(self.format_agent_property(agent_property, indentation_level + 1));
        }

        self.format_block_lines(block_lines, indentation_level)
    }

    fn format_agent_property(&self, agent_property: &AgentProperty, indentation_level: usize) -> String {
        match agent_property {
            AgentProperty::Model(model_expression) => {
                format!("model: {}", self.format_expression(model_expression, indentation_level))
            }
            AgentProperty::Prompt(prompt_expression) => {
                format!("prompt: {}", self.format_expression(prompt_expression, indentation_level))
            }
            AgentProperty::Output(output_type) => {
                format!("output: {}", self.format_type_expression(output_type, indentation_level))
            }
            AgentProperty::Context(context_expression) => {
                format!("context: {}", self.format_expression(context_expression, indentation_level))
            }
            AgentProperty::Inference(inference_expression) => {
                format!("inference: {}", self.format_expression(inference_expression, indentation_level))
            }
            AgentProperty::Tools(tools_expression) => {
                format!("tools: {}", self.format_expression(tools_expression, indentation_level))
            }
            AgentProperty::Custom { name, value } => {
                format!("{}: {}", name, self.format_expression(value, indentation_level))
            }
        }
    }

    fn format_typed_fields_block(&self, typed_fields: &[TypedField], indentation_level: usize) -> String {
        let mut block_lines = Vec::with_capacity(typed_fields.len());

        for typed_field in typed_fields {
            block_lines.push(self.format_typed_field(typed_field, indentation_level + 1));
        }

        self.format_block_lines(block_lines, indentation_level)
    }

    fn format_typed_field(&self, typed_field: &TypedField, indentation_level: usize) -> String {
        let mut formatted_typed_field = format!(
            "{}: {}",
            typed_field.name,
            self.format_type_expression(&typed_field.field_type, indentation_level)
        );

        if let Some(field_description) = &typed_field.description {
            formatted_typed_field.push(' ');
            formatted_typed_field.push_str(&self.format_plain_string_literal(field_description));
        }

        formatted_typed_field
    }

    fn format_type_expression(&self, type_expression: &TypeExpression, indentation_level: usize) -> String {
        match type_expression {
            TypeExpression::String => "string".to_owned(),
            TypeExpression::Number => "number".to_owned(),
            TypeExpression::Float => "float".to_owned(),
            TypeExpression::Boolean => "boolean".to_owned(),
            TypeExpression::Null => "null".to_owned(),
            TypeExpression::SchemaReference(schema_name) => format!("schema.{schema_name}"),
            TypeExpression::StringEnum(enum_value) => self.format_plain_string_literal(enum_value),
            TypeExpression::Array { item_type, fixed_length } => {
                let formatted_item_type = self.format_type_expression(item_type, indentation_level);

                if let Some(array_length) = fixed_length {
                    format!("[{formatted_item_type}; {array_length}]")
                } else {
                    format!("[{formatted_item_type}]")
                }
            }
            TypeExpression::Tuple(tuple_types) => {
                let mut formatted_tuple_types = Vec::with_capacity(tuple_types.len());

                for tuple_type in tuple_types {
                    formatted_tuple_types.push(self.format_type_expression(tuple_type, indentation_level));
                }

                format!("({})", formatted_tuple_types.join(", "))
            }
            TypeExpression::Object(object_fields) => self.format_typed_fields_block(object_fields, indentation_level),
            TypeExpression::Union(union_types) => {
                let mut formatted_union_types = Vec::with_capacity(union_types.len());

                for union_type in union_types {
                    formatted_union_types.push(self.format_type_expression(union_type, indentation_level));
                }

                formatted_union_types.join(" | ")
            }
        }
    }

    fn format_object_fields_block(&self, object_fields: &[ObjectField], indentation_level: usize) -> String {
        let mut block_lines = Vec::with_capacity(object_fields.len());

        for object_field in object_fields {
            block_lines.push(self.format_object_field(object_field, indentation_level + 1));
        }

        self.format_block_lines(block_lines, indentation_level)
    }

    fn format_object_field(&self, object_field: &ObjectField, indentation_level: usize) -> String {
        format!(
            "{}: {}",
            object_field.name,
            self.format_expression(&object_field.value, indentation_level)
        )
    }

    fn format_expression(&self, expression: &Expression, indentation_level: usize) -> String {
        match expression {
            Expression::StringLiteral(string_value) => self.format_expression_string_literal(string_value),
            Expression::StringTemplate(string_template) => self.format_expression_string_template(string_template, indentation_level),
            Expression::NumberLiteral(number_literal) => number_literal.replace('_', ""),
            Expression::BooleanLiteral(boolean_literal) => {
                if *boolean_literal {
                    "true".to_owned()
                } else {
                    "false".to_owned()
                }
            }
            Expression::NullLiteral => "null".to_owned(),
            Expression::Reference(reference) => self.format_reference(reference),
            Expression::FunctionCall(function_call) => self.format_function_call(function_call, indentation_level),
            Expression::ArrayLiteral(array_values) => self.format_array_expression(array_values, indentation_level),
            Expression::ObjectLiteral(object_fields) => self.format_object_fields_block(object_fields, indentation_level),
        }
    }

    fn format_function_call(&self, function_call: &FunctionCall, indentation_level: usize) -> String {
        let formatted_callee = self.format_reference(&function_call.callee);

        if function_call.arguments.is_empty() {
            return format!("{formatted_callee}()");
        }

        let mut formatted_arguments = Vec::with_capacity(function_call.arguments.len());

        for call_argument in &function_call.arguments {
            formatted_arguments.push(self.format_call_argument(call_argument, indentation_level + 1));
        }

        let inline_arguments = formatted_arguments.join(", ");
        let should_use_multiline_layout = inline_arguments.len() > 60
            || formatted_arguments.len() > 2
            || formatted_arguments
                .iter()
                .any(|formatted_argument| formatted_argument.contains('\n'));

        if !should_use_multiline_layout {
            return format!("{formatted_callee}({inline_arguments})");
        }

        let mut formatted_function_call = format!("{formatted_callee}(\n");

        for formatted_argument in formatted_arguments {
            formatted_function_call.push_str(&self.indentation(indentation_level + 1));
            formatted_function_call.push_str(&formatted_argument);
            formatted_function_call.push_str(",\n");
        }

        formatted_function_call.push_str(&self.indentation(indentation_level));
        formatted_function_call.push(')');
        formatted_function_call
    }

    fn format_call_argument(&self, call_argument: &CallArgument, indentation_level: usize) -> String {
        match call_argument {
            CallArgument::Positional(positional_expression) => self.format_expression(positional_expression, indentation_level),
            CallArgument::Named(NamedArgument {
                name: argument_name,
                value: argument_value,
            }) => {
                format!("{}: {}", argument_name, self.format_expression(argument_value, indentation_level))
            }
        }
    }

    fn format_array_expression(&self, array_values: &[Expression], indentation_level: usize) -> String {
        if array_values.is_empty() {
            return "[]".to_owned();
        }

        let mut formatted_array_values = Vec::with_capacity(array_values.len());

        for array_value in array_values {
            formatted_array_values.push(self.format_expression(array_value, indentation_level + 1));
        }

        let should_use_multiline_layout = formatted_array_values.len() > 1
            || formatted_array_values
                .iter()
                .any(|formatted_array_value| formatted_array_value.contains('\n'));

        if !should_use_multiline_layout {
            return format!("[{}]", formatted_array_values.join(", "));
        }

        let mut formatted_array = String::from("[\n");

        for formatted_array_value in formatted_array_values {
            formatted_array.push_str(&self.indentation(indentation_level + 1));
            formatted_array.push_str(&formatted_array_value);
            formatted_array.push_str(",\n");
        }

        formatted_array.push_str(&self.indentation(indentation_level));
        formatted_array.push(']');
        formatted_array
    }

    fn format_expression_string_literal(&self, string_value: &str) -> String {
        format!("\"{}\"", self.escape_string_content(string_value, true))
    }

    fn format_expression_string_template(&self, string_template: &StringTemplate, indentation_level: usize) -> String {
        let mut formatted_template_contents = String::new();

        for string_template_part in &string_template.parts {
            match string_template_part {
                StringTemplatePart::Text(string_text) => {
                    formatted_template_contents.push_str(&self.escape_string_content(string_text, true));
                }
                StringTemplatePart::Interpolation(interpolation_expression) => {
                    formatted_template_contents.push_str("{{ ");
                    formatted_template_contents.push_str(&self.format_expression(interpolation_expression, indentation_level));
                    formatted_template_contents.push_str(" }}");
                }
            }
        }

        format!("\"{formatted_template_contents}\"")
    }

    fn format_plain_string_literal(&self, string_value: &str) -> String {
        format!("\"{}\"", self.escape_string_content(string_value, false))
    }

    fn format_reference(&self, reference: &Reference) -> String {
        let mut formatted_reference = match &reference.root {
            engine_ai_core::dsl::ReferenceRoot::Keyword(reference_keyword) => reference_keyword.as_str().to_owned(),
            engine_ai_core::dsl::ReferenceRoot::Identifier(identifier_name) => identifier_name.clone(),
        };

        for reference_access in &reference.accesses {
            if reference_access.optional {
                formatted_reference.push_str("?.");
            } else {
                formatted_reference.push('.');
            }

            formatted_reference.push_str(&reference_access.field);
        }

        formatted_reference
    }

    fn format_block_lines(&self, block_lines: Vec<String>, indentation_level: usize) -> String {
        if block_lines.is_empty() {
            return "{}".to_owned();
        }

        let mut formatted_block = String::from("{\n");

        for block_line in block_lines {
            formatted_block.push_str(&self.indentation(indentation_level + 1));
            formatted_block.push_str(&block_line);
            formatted_block.push('\n');
        }

        formatted_block.push_str(&self.indentation(indentation_level));
        formatted_block.push('}');
        formatted_block
    }

    fn escape_string_content(&self, string_content: &str, escape_braces: bool) -> String {
        let mut escaped_string = String::new();

        for string_character in string_content.chars() {
            match string_character {
                '\\' => escaped_string.push_str("\\\\"),
                '\"' => escaped_string.push_str("\\\""),
                '\n' => escaped_string.push_str("\\n"),
                '\r' => escaped_string.push_str("\\r"),
                '\t' => escaped_string.push_str("\\t"),
                '{' if escape_braces => escaped_string.push_str("\\{"),
                '}' if escape_braces => escaped_string.push_str("\\}"),
                _ => escaped_string.push(string_character),
            }
        }

        escaped_string
    }

    fn indentation(&self, indentation_level: usize) -> String {
        "    ".repeat(indentation_level)
    }
}

#[cfg(test)]
mod tests {
    use super::format_source;
    use std::fs;
    use std::path::{Path, PathBuf};

    #[test]
    fn produces_canonical_format_for_messy_workflow() {
        let source = r#"
            provider    ollama    {driver:"ollama"     models:["qwen3.5:32b",]}
            agent greeting{model:ollama("qwen3.5:32b") prompt:"Hello \{there\}" output:string}
            output { greeting: agent.greeting }
        "#;

        let formatted_source = format_source(source).expect("formatter should parse valid workflow");

        let expected_source = r#"provider ollama {
    driver: "ollama"
    models: ["qwen3.5:32b"]
}

agent greeting {
    model: ollama("qwen3.5:32b")
    prompt: "Hello \{there\}"
    output: string
}

output {
    greeting: agent.greeting
}
"#;

        assert_eq!(formatted_source, expected_source);
    }

    #[test]
    fn formatter_is_idempotent_for_all_workflow_examples() {
        for workflow_path in discover_workflow_examples() {
            let workflow_source = fs::read_to_string(&workflow_path)
                .unwrap_or_else(|read_error| panic!("failed to read {}: {read_error}", workflow_path.display()));

            let first_formatted_output = format_source(&workflow_source)
                .unwrap_or_else(|format_error| panic!("failed to format {}: {format_error}", workflow_path.display()));

            let second_formatted_output = format_source(&first_formatted_output)
                .unwrap_or_else(|format_error| panic!("failed to re-format {}: {format_error}", workflow_path.display()));

            assert_eq!(
                first_formatted_output,
                second_formatted_output,
                "formatter output should be stable for {}",
                workflow_path.display()
            );
        }
    }

    fn discover_workflow_examples() -> Vec<PathBuf> {
        let workflows_directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("../core/workflows");
        let mut workflow_paths = Vec::new();

        collect_workflow_paths(&workflows_directory, &mut workflow_paths);
        workflow_paths.sort();
        workflow_paths
    }

    fn collect_workflow_paths(current_directory: &Path, workflow_paths: &mut Vec<PathBuf>) {
        let directory_entries = fs::read_dir(current_directory)
            .unwrap_or_else(|read_error| panic!("failed to read directory {}: {read_error}", current_directory.display()));

        for directory_entry_result in directory_entries {
            let directory_entry = directory_entry_result
                .unwrap_or_else(|read_error| panic!("failed to read entry in {}: {read_error}", current_directory.display()));

            let entry_path = directory_entry.path();

            if entry_path.is_dir() {
                collect_workflow_paths(&entry_path, workflow_paths);

                continue;
            }

            if entry_path.extension().and_then(|extension| extension.to_str()) != Some("ai") {
                continue;
            }

            workflow_paths.push(entry_path);
        }
    }
}
