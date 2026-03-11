use super::rules::{IndentationRule, ArrayFormattingRule};
use crate::ast::{
    Agent, AgentProperty, FunctionCall, InputBlock, NamedSchema, OutputBlock, Provider, Reference, SchemaField,
    SchemaReference, SchemaType, Value, Workflow,
};
use std::collections::HashMap;

pub struct Writer {
    indent_level: usize,
    indentation_rule: IndentationRule,
    array_rule: ArrayFormattingRule,
}

impl Writer {
    pub fn new() -> Self {
        Self {
            indent_level: 0,
            indentation_rule: IndentationRule::new(),
            array_rule: ArrayFormattingRule::new(),
        }
    }

    fn get_indent(&self, level: usize) -> String {
        self.indentation_rule.get_indent(level)
    }

    pub fn write_workflow(&self, workflow: &Workflow) -> String {
        let mut output = String::new();
        let mut sections = Vec::new();

        // Write providers first
        if !workflow.providers.is_empty() {
            let providers_section = self.write_providers(&workflow.providers);
            sections.push(providers_section);
        }

        // Write schemas
        if !workflow.schemas.is_empty() {
            let schemas_section = self.write_schemas(&workflow.schemas);
            sections.push(schemas_section);
        }

        // Write agents
        if !workflow.agents.is_empty() {
            let agents_section = self.write_agents(&workflow.agents);
            sections.push(agents_section);
        }

        // Write input block
        if let Some(input) = &workflow.input {
            let input_section = self.write_input_block(input);
            sections.push(input_section);
        }

        // Write output block
        if let Some(output_block) = &workflow.output {
            let output_section = self.write_output_block(output_block);
            sections.push(output_section);
        }

        // Join sections with double newlines
        for (index, section) in sections.iter().enumerate() {
            if index > 0 {
                output.push_str("\n\n");
            }
            output.push_str(section);
        }

        // Ensure file ends with single newline
        if !output.ends_with('\n') {
            output.push('\n');
        }

        output
    }

    fn write_providers(&self, providers: &[Provider]) -> String {
        let mut output = String::new();

        for (index, provider) in providers.iter().enumerate() {
            if index > 0 && true {
                output.push('\n');
            }
            output.push_str(&self.write_provider(provider));
        }

        output
    }

    fn write_provider(&self, provider: &Provider) -> String {
        let mut output = String::new();
        let indent = self.get_indent(self.indent_level);

        output.push_str(&format!("{}provider {} {{\n", indent, provider.name));

        // Write driver
        output.push_str(&format!(
            "{}driver{}<- \"{}\"\n",
            self.get_indent(self.indent_level + 1),
            if true { " " } else { "" },
            provider.driver
        ));

        // Write api_endpoint if present
        if let Some(api_endpoint) = &provider.api_endpoint {
            output.push_str(&format!(
                "{}api_endpoint{}<- \"{}\"\n",
                self.get_indent(self.indent_level + 1),
                if true { " " } else { "" },
                api_endpoint
            ));
        }

        // Write models
        if !provider.models.is_empty() {
            let models_value = Value::Array(provider.models.iter().map(|m| Value::String(m.clone())).collect());
            output.push_str(&format!(
                "{}models{}<- {}\n",
                self.get_indent(self.indent_level + 1),
                if true { " " } else { "" },
                self.write_value_with_indent(&models_value, self.indent_level + 1)
            ));
        }

        output.push_str(&format!("{indent}}}"));
        output
    }

    fn write_schemas(&self, schemas: &[NamedSchema]) -> String {
        let mut output = String::new();

        for (index, schema) in schemas.iter().enumerate() {
            if index > 0 && true {
                output.push('\n');
            }
            output.push_str(&self.write_named_schema(schema));
        }

        output
    }

    fn write_named_schema(&self, schema: &NamedSchema) -> String {
        let mut output = String::new();
        let indent = self.get_indent(self.indent_level);

        output.push_str(&format!("{}schema {} {{\n", indent, schema.name));
        output.push_str(&self.write_schema_fields(&schema.schema.fields, self.indent_level + 1));
        output.push_str(&format!("{indent}}}"));

        output
    }

    fn write_schema_fields(&self, fields: &[SchemaField], indent_level: usize) -> String {
        let mut output = String::new();

        for field in fields {
            output.push_str(&self.write_schema_field(field, indent_level));
        }

        output
    }

    fn write_schema_field(&self, field: &SchemaField, indent_level: usize) -> String {
        let mut output = String::new();
        let indent = self.get_indent(indent_level);

        output.push_str(&format!(
            "{}{}: {}",
            indent,
            field.name,
            self.write_schema_type_with_indent(&field.field_type, indent_level)
        ));

        if let Some(description) = &field.description {
            output.push_str(&format!(" // {description}"));
        }

        output.push('\n');
        output
    }

    fn write_schema_type_with_indent(&self, schema_type: &SchemaType, indent_level: usize) -> String {
        match schema_type {
            SchemaType::String => "string".to_string(),
            SchemaType::Number => "number".to_string(),
            SchemaType::Boolean => "boolean".to_string(),
            SchemaType::Null => "null".to_string(),
            SchemaType::Array(inner) => format!("[{}]", self.write_schema_type_with_indent(inner, indent_level)),
            SchemaType::Enum(variants) => {
                format!(
                    "enum({})",
                    variants
                        .iter()
                        .map(|v| format!("\"{v}\""))
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            }
            SchemaType::Object(fields) => {
                if fields.len() > 1 {
                    let mut output = String::from("{\n");
                    for field in fields {
                        output.push_str(&self.write_schema_field(field, indent_level + 1));
                    }
                    output.push_str(&format!("{}}}", self.get_indent(indent_level)));
                    output
                } else {
                    "{}".to_string()
                }
            }
        }
    }

    fn write_input_block(&self, input: &InputBlock) -> String {
        let mut output = String::new();
        let indent = self.get_indent(self.indent_level);

        output.push_str(&format!("{indent}input {{\n"));

        for field in &input.fields {
            output.push_str(&format!(
                "{}{}: {}\n",
                self.get_indent(self.indent_level + 1),
                field.name,
                self.write_schema_type_with_indent(&field.field_type, self.indent_level + 1)
            ));
        }

        output.push_str(&format!("{indent}}}"));
        output
    }

    fn write_agents(&self, agents: &[Agent]) -> String {
        let mut output = String::new();

        for (index, agent) in agents.iter().enumerate() {
            if index > 0 && true {
                output.push('\n');
            }
            output.push_str(&self.write_agent(agent));
            // Ensure each agent ends with a newline
            if !output.ends_with('\n') {
                output.push('\n');
            }
        }

        // Remove the final newline since sections will be joined with double newlines
        if output.ends_with('\n') {
            output.pop();
        }

        output
    }

    fn write_agent(&self, agent: &Agent) -> String {
        let mut output = String::new();
        let indent = self.get_indent(self.indent_level);

        // Write agent declaration - always use regular agent syntax
        output.push_str(&format!("{}agent {} {{\n", indent, agent.name));

        // Write properties
        for property in &agent.properties {
            output.push_str(&self.write_agent_property(property));
        }

        output.push_str(&format!("{indent}}}"));
        output
    }

    fn write_agent_property(&self, property: &AgentProperty) -> String {
        let indent = self.get_indent(self.indent_level + 1);
        let assignment_op = if true { " <- " } else { "<-" };

        match property {
            AgentProperty::Model { value, .. } => {
                format!(
                    "{}model{}{}\n",
                    indent,
                    assignment_op,
                    self.write_value_with_indent(value, self.indent_level + 1)
                )
            }
            AgentProperty::Tools { value, .. } => {
                format!(
                    "{}tools{}{}\n",
                    indent,
                    assignment_op,
                    self.write_value_with_indent(value, self.indent_level + 1)
                )
            }
            AgentProperty::Context { value, .. } => {
                format!(
                    "{}context{}{}\n",
                    indent,
                    assignment_op,
                    self.write_value_with_indent(value, self.indent_level + 1)
                )
            }
            AgentProperty::Output { value, .. } => {
                format!(
                    "{}output{}{}\n",
                    indent,
                    assignment_op,
                    self.write_schema_reference_with_indent(value, self.indent_level + 1)
                )
            }
            AgentProperty::Prompt { value, .. } => {
                format!(
                    "{}prompt{}{}\n",
                    indent,
                    assignment_op,
                    self.write_value_with_indent(value, self.indent_level + 1)
                )
            }
            AgentProperty::ForEach {
                collection, identifier, ..
            } => {
                format!(
                    "{}for_each{}{} as {}\n",
                    indent,
                    assignment_op,
                    self.write_value_with_indent(collection, self.indent_level + 1),
                    identifier
                )
            }
        }
    }

    fn write_schema_reference_with_indent(&self, schema_ref: &SchemaReference, indent_level: usize) -> String {
        match schema_ref {
            SchemaReference::Named(name) => name.clone(),
            SchemaReference::Inline(schema) => {
                if schema.fields.len() > 1 {
                    let mut output = String::from("{\n");
                    for field in &schema.fields {
                        output.push_str(&self.write_schema_field(field, indent_level + 1));
                    }
                    output.push_str(&format!("{}}}", self.get_indent(indent_level)));
                    output
                } else {
                    "{}".to_string()
                }
            }
            SchemaReference::InlineType {
                schema_type,
                description,
            } => {
                let mut output = self.write_schema_type_with_indent(schema_type, indent_level);
                if let Some(desc) = description {
                    output.push_str(&format!(" // {desc}"));
                }
                output
            }
        }
    }

    fn write_value_with_indent(&self, value: &Value, indent_level: usize) -> String {
        match value {
            Value::String(s) => {
                if s.contains('\n') && true {
                    self.write_multiline_string_with_indent(s, indent_level)
                } else {
                    format!("\"{}\"", s.replace('"', "\\\""))
                }
            }
            Value::Number(n) => {
                // Handle integer vs float formatting
                if n.fract() == 0.0 {
                    format!("{}", *n as i64)
                } else {
                    n.to_string()
                }
            }
            Value::Boolean(b) => b.to_string(),
            Value::Null => "null".to_string(),
            Value::Array(items) => self.write_array_with_indent(items, indent_level),
            Value::Object(obj) => self.write_object_with_indent(obj, indent_level),
            Value::Reference(reference) => self.write_reference(reference),
            Value::FunctionCall(func) => self.write_function_call_with_indent(func, indent_level),
            Value::Interpolated(s) => format!("\"{s}\""),
        }
    }

    fn write_multiline_string_with_indent(&self, content: &str, indent_level: usize) -> String {
        let base_indent = self.get_indent(indent_level);
        let mut output = String::from("\"\"\"");

        // Add the content exactly as it is, preserving original formatting
        output.push_str(content.trim_end());

        // Close the multiline string with proper base indentation
        output.push_str(&format!("\n{base_indent}\"\"\""));
        output
    }

    fn write_array_with_indent(&self, items: &[Value], indent_level: usize) -> String {
        // Calculate total content length to decide if we should break
        let items_str: Vec<String> = items
            .iter()
            .map(|item| self.write_value_with_indent(item, indent_level))
            .collect();

        if self.array_rule.should_break_array(items) {
            let mut output = String::from("[\n");
            for item_str in &items_str {
                output.push_str(&format!(
                    "{}{},\n",
                    self.get_indent(indent_level + 1),
                    item_str
                ));
            }
            output.push_str(&format!("{}]", self.get_indent(indent_level)));
            output
        } else {
            format!("[{}]", items_str.join(", "))
        }
    }

    fn write_object_with_indent(&self, obj: &HashMap<String, Value>, indent_level: usize) -> String {
        if obj.len() > 1 {
            let mut output = String::from("{\n");
            // Sort keys for consistent output
            let mut sorted_keys: Vec<_> = obj.keys().collect();
            sorted_keys.sort();
            for key in sorted_keys {
                let value = &obj[key];
                output.push_str(&format!(
                    "{}{}: {},\n",
                    self.get_indent(indent_level + 1),
                    key,
                    self.write_value_with_indent(value, indent_level + 1)
                ));
            }
            output.push_str(&format!("{}}}", self.get_indent(indent_level)));
            output
        } else {
            let mut sorted_keys: Vec<_> = obj.keys().collect();
            sorted_keys.sort();
            let pairs: Vec<String> = sorted_keys
                .iter()
                .map(|k| {
                    format!(
                        "{}: {}",
                        k,
                        self.write_value_with_indent(&obj[k.as_str()], indent_level)
                    )
                })
                .collect();
            format!("{{{}}}", pairs.join(", "))
        }
    }

    fn write_reference(&self, reference: &Reference) -> String {
        match reference {
            Reference::Agent { agent, field } => format!("agent.{agent}.{field}"),
            Reference::AgentOutput { agent } => format!("agent.{agent}.output"),
            Reference::AgentContext { agent } => format!("agent.{agent}.context"),
            Reference::Input { field } => format!("input.{field}"),
            Reference::Schema { name } => name.clone(),
            Reference::Tool { name } => format!("tool.{name}"),
        }
    }

    fn write_function_call_with_indent(&self, func: &FunctionCall, indent_level: usize) -> String {
        let mut output = format!("{}(", func.name);

        if func.arguments.is_empty() {
            output.push(')');
        } else if func.arguments.len() > 1 {
            output.push('\n');
            // Sort keys for consistent output
            let mut sorted_keys: Vec<_> = func.arguments.keys().collect();
            sorted_keys.sort();
            for key in sorted_keys {
                let value = &func.arguments[key];
                output.push_str(&format!(
                    "{}{}: {},\n",
                    self.get_indent(indent_level + 1),
                    key,
                    self.write_value_with_indent(value, indent_level + 1)
                ));
            }
            output.push_str(&format!("{})", self.get_indent(indent_level)));
        } else {
            let mut sorted_keys: Vec<_> = func.arguments.keys().collect();
            sorted_keys.sort();
            let args: Vec<String> = sorted_keys
                .iter()
                .map(|k| {
                    format!(
                        "{}: {}",
                        k,
                        self.write_value_with_indent(&func.arguments[k.as_str()], indent_level)
                    )
                })
                .collect();
            output.push_str(&args.join(", "));
            output.push(')');
        }

        output
    }

    fn write_output_block(&self, output_block: &OutputBlock) -> String {
        let mut output = String::new();
        let indent = self.get_indent(self.indent_level);

        output.push_str(&format!("{indent}output {{\n"));

        for field in &output_block.fields {
            output.push_str(&format!(
                "{}{}{}{}\n",
                self.get_indent(self.indent_level + 1),
                field.name,
                if true { " <- " } else { "<-" },
                self.write_value_with_indent(&field.value, self.indent_level + 1)
            ));
        }

        output.push_str(&format!("{indent}}}"));
        output
    }
}

impl Default for Writer {
    fn default() -> Self {
        Self::new()
    }
}
