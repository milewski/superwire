use crate::ast::*;
use crate::parser::error::ParserError;
use crate::parser::{Rule, WorkflowParser};
use pest::Parser;
use std::collections::HashMap;

pub struct AstBuilder {
    file_path: String,
}

impl AstBuilder {
    pub fn new(file_path: String) -> Self {
        Self { file_path }
    }

    pub fn parse(&self, input_str: &str) -> Result<Workflow, ParserError> {
        let pairs = WorkflowParser::parse(Rule::workflow, input_str)
            .map_err(|error| self.enhance_pest_error(error, input_str))?;

        let mut providers = Vec::new();
        let mut schemas = Vec::new();
        let mut agents = Vec::new();
        let mut input = None;
        let mut output = None;

        for pair in pairs {
            if pair.as_rule() == Rule::workflow {
                for inner_pair in pair.into_inner() {
                    match inner_pair.as_rule() {
                        Rule::provider => {
                            providers.push(self.parse_provider(inner_pair)?);
                        }
                        Rule::schema => {
                            schemas.push(self.parse_named_schema(inner_pair)?);
                        }
                        Rule::agent => {
                            agents.push(self.parse_agent(inner_pair)?);
                        }
                        Rule::input_block => {
                            input = Some(self.parse_input_block(inner_pair)?);
                        }
                        Rule::output_block => {
                            output = Some(self.parse_output_block(inner_pair)?);
                        }
                        Rule::EOI => {}
                        _ => {}
                    }
                }
            }
        }

        Ok(Workflow {
            providers,
            schemas,
            agents,
            input,
            output,
            span: Span::new(0, input_str.len(), 1, 1),
        })
    }

    fn parse_provider(&self, pair: pest::iterators::Pair<Rule>) -> Result<Provider, ParserError> {
        let span = self.pair_to_span(&pair);
        let mut name = String::new();
        let mut driver = String::new();
        let mut api_endpoint = None;
        let mut models = Vec::new();

        for inner_pair in pair.into_inner() {
            match inner_pair.as_rule() {
                Rule::identifier => {
                    if name.is_empty() {
                        name = inner_pair.as_str().to_string();
                    }
                }
                Rule::provider_property => {
                    let full_text = inner_pair.as_str();
                    let property_name = if full_text.starts_with("driver") {
                        "driver"
                    } else if full_text.starts_with("api_endpoint") {
                        "api_endpoint"
                    } else if full_text.starts_with("models") {
                        "models"
                    } else {
                        ""
                    };

                    let mut property_value = None;

                    for prop_pair in inner_pair.into_inner() {
                        if prop_pair.as_rule() == Rule::string_value {
                            property_value = Some(Value::String(self.parse_string_value(prop_pair)?));
                        } else if prop_pair.as_rule() == Rule::array_value {
                            property_value = Some(self.parse_array_value(prop_pair)?);
                        }
                    }

                    if let Some(value) = property_value {
                        match property_name {
                            "driver" => {
                                if let Value::String(string) = value {
                                    driver = string;
                                }
                            }
                            "api_endpoint" => {
                                if let Value::String(string) = value {
                                    api_endpoint = Some(string);
                                }
                            }
                            "models" => {
                                if let Value::Array(array) = value {
                                    models = array
                                        .iter()
                                        .filter_map(|v| match v {
                                            Value::String(string) => Some(string.clone()),
                                            Value::Interpolated(string) => Some(string.clone()),
                                            _ => None,
                                        })
                                        .collect();
                                }
                            }
                            _ => {}
                        }
                    }
                }
                _ => {}
            }
        }

        Ok(Provider {
            name,
            driver,
            api_endpoint,
            models,
            span,
        })
    }

    fn parse_named_schema(&self, pair: pest::iterators::Pair<Rule>) -> Result<NamedSchema, ParserError> {
        let span = self.pair_to_span(&pair);
        let mut name = String::new();
        let mut fields = Vec::new();

        for inner_pair in pair.into_inner() {
            match inner_pair.as_rule() {
                Rule::identifier => {
                    if name.is_empty() {
                        name = inner_pair.as_str().to_string();
                    }
                }
                Rule::schema_field => {
                    fields.push(self.parse_schema_field(inner_pair)?);
                }
                _ => {}
            }
        }

        Ok(NamedSchema {
            name,
            schema: Schema {
                fields,
                span: span.clone(),
            },
            span,
        })
    }

    fn parse_schema_field(&self, pair: pest::iterators::Pair<Rule>) -> Result<SchemaField, ParserError> {
        let span = self.pair_to_span(&pair);
        let mut name = String::new();
        let mut field_type = SchemaType::String;
        let mut description = None;

        for inner_pair in pair.into_inner() {
            match inner_pair.as_rule() {
                Rule::identifier => {
                    if name.is_empty() {
                        name = inner_pair.as_str().to_string();
                    }
                }
                Rule::schema_type => {
                    field_type = self.parse_schema_type(inner_pair)?;
                }
                Rule::string_value => {
                    description = Some(self.parse_string_value(inner_pair)?);
                }
                _ => {}
            }
        }

        Ok(SchemaField {
            name,
            field_type,
            description,
            span,
        })
    }

    fn parse_schema_type(&self, pair: pest::iterators::Pair<Rule>) -> Result<SchemaType, ParserError> {
        for inner_pair in pair.into_inner() {
            match inner_pair.as_rule() {
                Rule::schema_type_primitive => {
                    return Ok(match inner_pair.as_str() {
                        "string" => SchemaType::String,
                        "number" => SchemaType::Number,
                        "boolean" => SchemaType::Boolean,
                        "null" => SchemaType::Null,
                        _ => SchemaType::String,
                    });
                }
                Rule::schema_type_array => {
                    for array_inner in inner_pair.into_inner() {
                        if array_inner.as_rule() == Rule::schema_type {
                            let inner_type = self.parse_schema_type(array_inner)?;
                            return Ok(SchemaType::Array(Box::new(inner_type)));
                        }
                    }
                }
                Rule::schema_type_enum => {
                    let mut variants = Vec::new();
                    for enum_inner in inner_pair.into_inner() {
                        match enum_inner.as_rule() {
                            Rule::schema_type_primitive => {
                                variants.push(enum_inner.as_str().to_string());
                            }
                            Rule::string_value => {
                                variants.push(self.parse_string_value(enum_inner)?);
                            }
                            _ => {}
                        }
                    }
                    return Ok(SchemaType::Enum(variants));
                }
                Rule::schema_type_object => {
                    let mut fields = Vec::new();
                    for obj_inner in inner_pair.into_inner() {
                        if obj_inner.as_rule() == Rule::schema_field {
                            fields.push(self.parse_schema_field(obj_inner)?);
                        }
                    }
                    return Ok(SchemaType::Object(fields));
                }
                _ => {}
            }
        }

        Ok(SchemaType::String)
    }

    fn parse_agent(&self, pair: pest::iterators::Pair<Rule>) -> Result<Agent, ParserError> {
        let span = self.pair_to_span(&pair);
        let mut name = String::new();
        let mut is_terminal = false;
        let mut properties = Vec::new();

        for inner_pair in pair.into_inner() {
            match inner_pair.as_rule() {
                Rule::terminal_marker => {
                    is_terminal = true;
                }
                Rule::identifier => {
                    if name.is_empty() {
                        name = inner_pair.as_str().to_string();
                    }
                }
                Rule::agent_property => {
                    properties.push(self.parse_agent_property(inner_pair)?);
                }
                _ => {}
            }
        }

        Ok(Agent {
            name,
            is_terminal,
            properties,
            span,
        })
    }

    fn parse_agent_property(&self, pair: pest::iterators::Pair<Rule>) -> Result<AgentProperty, ParserError> {
        let span = self.pair_to_span(&pair);
        let mut property_name = String::new();
        let mut value: Option<Value> = None;
        let mut schema_ref: Option<SchemaReference> = None;
        let mut for_each_identifier = String::new();

        for inner_pair in pair.into_inner() {
            match inner_pair.as_rule() {
                Rule::property_model => property_name = "model".to_string(),
                Rule::property_tools => property_name = "tools".to_string(),
                Rule::property_context => property_name = "context".to_string(),
                Rule::property_output => property_name = "output".to_string(),
                Rule::property_prompt => property_name = "prompt".to_string(),
                Rule::property_for_each => property_name = "for_each".to_string(),
                Rule::value => {
                    value = Some(self.parse_value(inner_pair)?);
                }
                Rule::schema_reference => {
                    schema_ref = Some(self.parse_schema_reference(inner_pair)?);
                }
                Rule::identifier => {
                    if property_name == "for_each" && for_each_identifier.is_empty() {
                        for_each_identifier = inner_pair.as_str().to_string();
                    }
                }
                _ => {}
            }
        }

        match property_name.as_str() {
            "model" => Ok(AgentProperty::Model {
                value: value.unwrap_or(Value::String(String::new())),
                span,
            }),
            "tools" => Ok(AgentProperty::Tools {
                value: value.unwrap_or(Value::Array(Vec::new())),
                span,
            }),
            "context" => Ok(AgentProperty::Context {
                value: value.unwrap_or(Value::String(String::new())),
                span,
            }),
            "output" => Ok(AgentProperty::Output {
                value: schema_ref.unwrap_or(SchemaReference::Inline(Schema {
                    fields: Vec::new(),
                    span: span.clone(),
                })),
                span,
            }),
            "prompt" => Ok(AgentProperty::Prompt {
                value: value.unwrap_or(Value::String(String::new())),
                span,
            }),
            "for_each" => Ok(AgentProperty::ForEach {
                collection: value.unwrap_or(Value::Array(Vec::new())),
                identifier: for_each_identifier,
                span,
            }),
            _ => Ok(AgentProperty::Prompt {
                value: Value::String("placeholder".to_string()),
                span,
            }),
        }
    }

    fn parse_value(&self, pair: pest::iterators::Pair<Rule>) -> Result<Value, ParserError> {
        for inner_pair in pair.into_inner() {
            match inner_pair.as_rule() {
                Rule::string_value => {
                    return Ok(Value::String(self.parse_string_value(inner_pair)?));
                }
                Rule::multiline_string => {
                    return Ok(Value::String(self.parse_multiline_string(inner_pair)?));
                }
                Rule::number_value => {
                    let num_str = inner_pair.as_str();
                    let num = num_str.parse::<f64>().unwrap_or(0.0);
                    return Ok(Value::Number(num));
                }
                Rule::boolean_value => {
                    let bool_val = inner_pair.as_str() == "true";
                    return Ok(Value::Boolean(bool_val));
                }
                Rule::null_value => {
                    return Ok(Value::Null);
                }
                Rule::array_value => {
                    return self.parse_array_value(inner_pair);
                }
                Rule::object_value => {
                    return self.parse_object_value(inner_pair);
                }
                Rule::reference => {
                    return self.parse_reference(inner_pair);
                }
                Rule::function_call => {
                    return self.parse_function_call(inner_pair);
                }
                Rule::interpolated_string => {
                    return Ok(Value::Interpolated(self.parse_interpolated_string(inner_pair)?));
                }
                _ => {}
            }
        }

        Ok(Value::String(String::new()))
    }

    fn parse_multiline_string(&self, pair: pest::iterators::Pair<Rule>) -> Result<String, ParserError> {
        let text = pair.as_str();

        if text.starts_with("\"\"\"") && text.ends_with("\"\"\"") && text.len() >= 6 {
            return Ok(text[3..text.len() - 3].to_string());
        }

        Ok(text.to_string())
    }

    fn parse_array_value(&self, pair: pest::iterators::Pair<Rule>) -> Result<Value, ParserError> {
        let mut values = Vec::new();

        for inner_pair in pair.into_inner() {
            if inner_pair.as_rule() == Rule::value {
                values.push(self.parse_value(inner_pair)?);
            }
        }

        Ok(Value::Array(values))
    }

    fn parse_object_value(&self, pair: pest::iterators::Pair<Rule>) -> Result<Value, ParserError> {
        let mut object = HashMap::new();

        for inner_pair in pair.into_inner() {
            if inner_pair.as_rule() == Rule::object_pair {
                let mut key = String::new();
                let mut value = None;

                for pair_inner in inner_pair.into_inner() {
                    match pair_inner.as_rule() {
                        Rule::identifier => {
                            key = pair_inner.as_str().to_string();
                        }
                        Rule::value => {
                            value = Some(self.parse_value(pair_inner)?);
                        }
                        _ => {}
                    }
                }

                if let Some(val) = value {
                    object.insert(key, val);
                }
            }
        }

        Ok(Value::Object(object))
    }

    fn parse_reference(&self, pair: pest::iterators::Pair<Rule>) -> Result<Value, ParserError> {
        for inner_pair in pair.into_inner() {
            match inner_pair.as_rule() {
                Rule::agent_context_reference => {
                    let mut agent_name = String::new();

                    for ref_inner in inner_pair.into_inner() {
                        if ref_inner.as_rule() == Rule::identifier {
                            agent_name = ref_inner.as_str().to_string();
                            break;
                        }
                    }

                    return Ok(Value::Reference(Reference::AgentContext { agent: agent_name }));
                }
                Rule::agent_output_reference => {
                    let mut agent_name = String::new();

                    for ref_inner in inner_pair.into_inner() {
                        if ref_inner.as_rule() == Rule::identifier {
                            agent_name = ref_inner.as_str().to_string();
                            break;
                        }
                    }

                    return Ok(Value::Reference(Reference::AgentOutput { agent: agent_name }));
                }
                Rule::agent_field_reference => {
                    let mut parts = Vec::new();

                    for ref_inner in inner_pair.into_inner() {
                        if ref_inner.as_rule() == Rule::identifier {
                            parts.push(ref_inner.as_str().to_string());
                        }
                    }

                    if parts.len() >= 2 {
                        return Ok(Value::Reference(Reference::Agent {
                            agent: parts[0].clone(),
                            field: parts[1].clone(),
                        }));
                    }
                }
                Rule::input_reference => {
                    let mut field_name = String::new();

                    for ref_inner in inner_pair.into_inner() {
                        if ref_inner.as_rule() == Rule::identifier {
                            field_name = ref_inner.as_str().to_string();
                            break;
                        }
                    }

                    return Ok(Value::Reference(Reference::Input { field: field_name }));
                }
                Rule::schema_name_reference => {
                    let mut schema_name = String::new();

                    for ref_inner in inner_pair.into_inner() {
                        if ref_inner.as_rule() == Rule::identifier {
                            schema_name = ref_inner.as_str().to_string();
                            break;
                        }
                    }

                    return Ok(Value::Reference(Reference::Schema { name: schema_name }));
                }
                _ => {}
            }
        }

        Ok(Value::String(String::new()))
    }

    fn parse_function_call(&self, pair: pest::iterators::Pair<Rule>) -> Result<Value, ParserError> {
        let span = self.pair_to_span(&pair);
        let mut function_name = String::new();
        let mut arguments = HashMap::new();

        for inner_pair in pair.into_inner() {
            match inner_pair.as_rule() {
                Rule::identifier => {
                    if function_name.is_empty() {
                        function_name = inner_pair.as_str().to_string();
                    }
                }
                Rule::string_value => {
                    // First string is the path argument
                    if !arguments.contains_key("path") {
                        arguments.insert("path".to_string(), Value::String(self.parse_string_value(inner_pair)?));
                    }
                }
                Rule::function_binding => {
                    let mut binding_key = String::new();
                    let mut binding_value = None;

                    for binding_inner in inner_pair.into_inner() {
                        match binding_inner.as_rule() {
                            Rule::identifier => {
                                binding_key = binding_inner.as_str().to_string();
                            }
                            Rule::value => {
                                binding_value = Some(self.parse_value(binding_inner)?);
                            }
                            _ => {}
                        }
                    }

                    if let Some(val) = binding_value {
                        arguments.insert(binding_key, val);
                    }
                }
                _ => {}
            }
        }

        Ok(Value::FunctionCall(FunctionCall {
            name: function_name,
            arguments,
            span,
        }))
    }

    fn parse_interpolated_string(&self, pair: pest::iterators::Pair<Rule>) -> Result<String, ParserError> {
        let mut result = String::new();

        for inner_pair in pair.into_inner() {
            if inner_pair.as_rule() == Rule::interpolated_content {
                result.push_str(inner_pair.as_str());
            }
        }

        Ok(result)
    }

    fn parse_schema_reference(&self, pair: pest::iterators::Pair<Rule>) -> Result<SchemaReference, ParserError> {
        let span = self.pair_to_span(&pair);

        for inner_pair in pair.into_inner() {
            match inner_pair.as_rule() {
                Rule::identifier => {
                    return Ok(SchemaReference::Named(inner_pair.as_str().to_string()));
                }
                Rule::inline_schema => {
                    let mut fields = Vec::new();

                    for schema_inner in inner_pair.into_inner() {
                        if schema_inner.as_rule() == Rule::schema_field {
                            fields.push(self.parse_schema_field(schema_inner)?);
                        }
                    }

                    return Ok(SchemaReference::Inline(Schema {
                        fields,
                        span: span.clone(),
                    }));
                }
                Rule::inline_type => {
                    let mut schema_type = None;
                    let mut description = None;

                    for type_inner in inner_pair.into_inner() {
                        match type_inner.as_rule() {
                            Rule::schema_type => {
                                schema_type = Some(self.parse_schema_type(type_inner)?);
                            }
                            Rule::string_value => {
                                description = Some(self.parse_string_value(type_inner)?);
                            }
                            _ => {}
                        }
                    }

                    if let Some(schema_type_value) = schema_type {
                        return Ok(SchemaReference::InlineType {
                            schema_type: schema_type_value,
                            description,
                        });
                    }
                }
                _ => {}
            }
        }

        Ok(SchemaReference::Inline(Schema {
            fields: Vec::new(),
            span,
        }))
    }

    fn parse_input_block(&self, pair: pest::iterators::Pair<Rule>) -> Result<InputBlock, ParserError> {
        let span = self.pair_to_span(&pair);
        let mut fields = Vec::new();

        for inner_pair in pair.into_inner() {
            if inner_pair.as_rule() == Rule::input_field {
                let field_span = self.pair_to_span(&inner_pair);
                let mut name = String::new();
                let mut field_type = SchemaType::String;

                for field_inner in inner_pair.into_inner() {
                    match field_inner.as_rule() {
                        Rule::identifier => {
                            if name.is_empty() {
                                name = field_inner.as_str().to_string();
                            }
                        }
                        Rule::schema_type => {
                            field_type = self.parse_schema_type(field_inner)?;
                        }
                        _ => {}
                    }
                }

                fields.push(InputField {
                    name,
                    field_type,
                    span: field_span,
                });
            }
        }

        Ok(InputBlock { fields, span })
    }

    fn parse_output_block(&self, pair: pest::iterators::Pair<Rule>) -> Result<OutputBlock, ParserError> {
        let span = self.pair_to_span(&pair);
        let mut fields = Vec::new();

        for inner_pair in pair.into_inner() {
            if inner_pair.as_rule() == Rule::output_field {
                let field_span = self.pair_to_span(&inner_pair);
                let mut name = String::new();
                let mut value = Value::Null;

                for field_inner in inner_pair.into_inner() {
                    match field_inner.as_rule() {
                        Rule::identifier => {
                            if name.is_empty() {
                                name = field_inner.as_str().to_string();
                            }
                        }
                        Rule::value => {
                            value = self.parse_value(field_inner)?;
                        }
                        _ => {}
                    }
                }

                fields.push(OutputField {
                    name,
                    value,
                    span: field_span,
                });
            }
        }

        Ok(OutputBlock { fields, span })
    }

    fn parse_string_value(&self, pair: pest::iterators::Pair<Rule>) -> Result<String, ParserError> {
        let text = pair.as_str();

        if text.starts_with('"') && text.ends_with('"') && text.len() >= 2 {
            return Ok(text[1..text.len() - 1].to_string());
        }

        Ok(text.to_string())
    }

    fn pair_to_span(&self, pair: &pest::iterators::Pair<Rule>) -> Span {
        let span = pair.as_span();
        let (line, column) = span.start_pos().line_col();

        Span::new(span.start(), span.end(), line, column)
    }

    fn enhance_pest_error(&self, error: pest::error::Error<Rule>, input_str: &str) -> ParserError {
        let (line, column) = match error.line_col {
            pest::error::LineColLocation::Pos((line, column)) => (line, column),
            pest::error::LineColLocation::Span((line, column), _) => (line, column),
        };

        let message = self.analyze_parsing_context(input_str, line, column, &error);
        let (corrected_line, corrected_column) = self.get_error_line_and_column(input_str, line);
        let source_line = self.get_source_line(input_str, corrected_line);

        ParserError::syntax_error_with_source(
            self.file_path.clone(),
            corrected_line,
            corrected_column,
            message.0,
            message.1,
            source_line,
        )
    }

    fn analyze_parsing_context(
        &self,
        input_str: &str,
        error_line: usize,
        _error_column: usize,
        error: &pest::error::Error<Rule>,
    ) -> (String, Option<String>) {
        let lines: Vec<&str> = input_str.lines().collect();

        if error_line == 0 || error_line > lines.len() {
            return (format!("{}", error), None);
        }

        let current_line_index = error_line - 1;
        let current_line = lines[current_line_index].trim();

        if current_line.contains(':') && !current_line.contains("<-") && !current_line.ends_with('{') {
            let in_braces = current_line.contains('{') || current_line.contains('}');

            if !in_braces {
                let parts: Vec<&str> = current_line.split(':').collect();
                if parts.len() >= 2 {
                    let before_colon = parts[0].trim();
                    let after_colon = parts[1].trim();

                    if !before_colon.is_empty()
                        && !after_colon.is_empty()
                        && !after_colon.starts_with('[')
                        && !after_colon.starts_with('{')
                    {
                        return (
                            "Invalid assignment operator ':'".to_string(),
                            Some("Use '<-' for assignment in output blocks, not ':'. Example: field <- value. Note: ':' is only used for type definitions in schemas".to_string()),
                        );
                    }
                }
            }
        }

        if current_line.contains('=')
            && !current_line.contains("==")
            && !current_line.contains("!=")
            && !current_line.contains("<=")
            && !current_line.contains(">=")
            && (current_line.contains(" = ") || (current_line.contains('=') && !current_line.contains("<-")))
        {
            return (
                "Invalid assignment operator '='".to_string(),
                Some("Use '<-' for assignment, not '='. Example: field <- value".to_string()),
            );
        }

        if current_line.contains('<')
            && !current_line.contains("<-")
            && (current_line.contains(" < ") || current_line.ends_with('<'))
        {
            return (
                "Invalid assignment operator '<'".to_string(),
                Some("Use '<-' for assignment, not '<'. Example: field <- value".to_string()),
            );
        }

        if current_line.contains("<-") && !current_line.ends_with('{') {
            let parts: Vec<&str> = current_line.split("<-").collect();
            if let Some(property_name) = parts.first() {
                let property_name = property_name.trim();
                let valid_properties = [
                    "model",
                    "tools",
                    "context",
                    "output",
                    "prompt",
                    "for_each",
                    "driver",
                    "api_endpoint",
                    "models",
                ];

                if !valid_properties.contains(&property_name) && !property_name.is_empty() {
                    let suggestion = self.find_closest_match(property_name, &valid_properties);
                    return (
                        format!("Unknown property '{}'", property_name),
                        Some(if let Some(closest) = suggestion {
                            format!("Did you mean '{}'?", closest)
                        } else {
                            format!("Valid properties are: {}", valid_properties.join(", "))
                        }),
                    );
                }
            }
        }

        if current_line == "}" {
            let agent_context = self.find_agent_context(&lines, error_line);

            if let Some((agent_name, agent_start_line)) = agent_context {
                let agent_properties = self.extract_agent_properties(&lines, agent_start_line, error_line);

                let has_model = agent_properties.iter().any(|p| p.starts_with("model"));
                let has_prompt = agent_properties.iter().any(|p| p.starts_with("prompt"));
                let has_output = agent_properties.iter().any(|p| p.starts_with("output"));

                if !has_prompt {
                    return (
                        format!("Agent '{}' is missing required property 'prompt'", agent_name),
                        Some(format!("Add a 'prompt <- \"...\"' property to agent '{}'", agent_name)),
                    );
                }

                if !has_model {
                    return (
                        format!("Agent '{}' is missing required property 'model'", agent_name),
                        Some(format!(
                            "Add a 'model <- \"provider/model\"' property to agent '{}'",
                            agent_name
                        )),
                    );
                }

                if !has_output {
                    return (
                        format!("Agent '{}' is missing required property 'output'", agent_name),
                        Some(format!(
                            "Add an 'output <- {{ ... }}' property to agent '{}'",
                            agent_name
                        )),
                    );
                }
            }

            let output_context = self.find_output_context(&lines, error_line);
            if output_context {
                return (
                    "Invalid value in output block".to_string(),
                    Some("Output values must be references like 'agent.name' or literals like \"string\", not bare identifiers".to_string()),
                );
            }
        }

        if error_line > 1 {
            let previous_line = lines[error_line - 2].trim();
            if previous_line.contains("<-") && !previous_line.ends_with('{') {
                let parts: Vec<&str> = previous_line.split("<-").collect();
                if parts.len() == 2 {
                    let value_part = parts[1].trim();
                    if !value_part.is_empty()
                        && !value_part.starts_with('"')
                        && !value_part.starts_with('[')
                        && !value_part.starts_with('{')
                    {
                        let identifier_pattern = regex::Regex::new(r"^[a-zA-Z_][a-zA-Z0-9_]*$").unwrap();
                        if identifier_pattern.is_match(value_part) {
                            return (
                                format!("Invalid reference: '{}'", value_part),
                                Some(format!("Did you mean 'agent.{}'? Bare identifiers are not valid values. Use 'agent.name' to reference an agent, or '\"{}\"' for a string literal", value_part, value_part)),
                            );
                        }
                    }
                }
            }
        }

        let expected_description = self.format_expected_rules(error);

        (
            format!("Unexpected syntax: {}", expected_description),
            Some("Check the syntax of your workflow definition".to_string()),
        )
    }

    fn find_closest_match(&self, input: &str, candidates: &[&str]) -> Option<String> {
        let input_lower = input.to_lowercase();
        let mut best_match = None;
        let mut best_distance = usize::MAX;

        for candidate in candidates {
            let distance = self.levenshtein_distance(&input_lower, &candidate.to_lowercase());
            if distance < best_distance && distance <= 2 {
                best_distance = distance;
                best_match = Some(candidate.to_string());
            }
        }

        best_match
    }

    fn levenshtein_distance(&self, source: &str, target: &str) -> usize {
        let source_len = source.len();
        let target_len = target.len();

        if source_len == 0 {
            return target_len;
        }
        if target_len == 0 {
            return source_len;
        }

        let mut matrix = vec![vec![0; target_len + 1]; source_len + 1];

        for index in 0..=source_len {
            matrix[index][0] = index;
        }
        for index in 0..=target_len {
            matrix[0][index] = index;
        }

        for (source_index, source_char) in source.chars().enumerate() {
            for (target_index, target_char) in target.chars().enumerate() {
                let cost = if source_char == target_char { 0 } else { 1 };
                matrix[source_index + 1][target_index + 1] = std::cmp::min(
                    std::cmp::min(
                        matrix[source_index][target_index + 1] + 1,
                        matrix[source_index + 1][target_index] + 1,
                    ),
                    matrix[source_index][target_index] + cost,
                );
            }
        }

        matrix[source_len][target_len]
    }

    fn get_error_line_and_column(&self, input_str: &str, error_line: usize) -> (usize, usize) {
        let lines: Vec<&str> = input_str.lines().collect();

        if error_line > 0 && error_line <= lines.len() {
            let current_line = lines[error_line - 1];

            if current_line.contains(':') && !current_line.contains("<-") && !current_line.trim().ends_with('{') {
                let in_braces = current_line.contains('{') || current_line.contains('}');

                if !in_braces {
                    let parts: Vec<&str> = current_line.split(':').collect();
                    if parts.len() >= 2 {
                        let before_colon = parts[0].trim();
                        let after_colon = parts[1].trim();

                        if !before_colon.is_empty()
                            && !after_colon.is_empty()
                            && !after_colon.starts_with('[')
                            && !after_colon.starts_with('{')
                        {
                            if let Some(position) = current_line.find(':') {
                                return (error_line, position + 1);
                            }
                        }
                    }
                }
            }

            if current_line.contains('=')
                && !current_line.contains("==")
                && !current_line.contains("!=")
                && !current_line.contains("<=")
                && !current_line.contains(">=")
                && !current_line.contains("<-")
            {
                if let Some(position) = current_line.find('=') {
                    return (error_line, position + 1);
                }
            }

            if current_line.contains('<') && !current_line.contains("<-") {
                if let Some(position) = current_line.find('<') {
                    return (error_line, position + 1);
                }
            }

            if current_line.trim().contains("<-") && !current_line.trim().ends_with('{') {
                let trimmed = current_line.trim();
                let parts: Vec<&str> = trimmed.split("<-").collect();
                if let Some(property_name) = parts.first() {
                    let property_name = property_name.trim();
                    let valid_properties = [
                        "model",
                        "tools",
                        "context",
                        "output",
                        "prompt",
                        "for_each",
                        "driver",
                        "api_endpoint",
                        "models",
                    ];

                    if !valid_properties.contains(&property_name) && !property_name.is_empty() {
                        if let Some(position) = current_line.find(property_name) {
                            return (error_line, position + 1);
                        }
                    }
                }
            }
        }

        if error_line > 1 {
            let previous_line = lines[error_line - 2].trim();
            if previous_line.contains("<-") && !previous_line.ends_with('{') {
                let parts: Vec<&str> = previous_line.split("<-").collect();
                if parts.len() == 2 {
                    let value_part = parts[1].trim();
                    if !value_part.is_empty()
                        && !value_part.starts_with('"')
                        && !value_part.starts_with('[')
                        && !value_part.starts_with('{')
                    {
                        let identifier_pattern = regex::Regex::new(r"^[a-zA-Z_][a-zA-Z0-9_]*$").unwrap();
                        if identifier_pattern.is_match(value_part) {
                            let full_line = lines[error_line - 2];
                            if let Some(arrow_position) = full_line.find("<-") {
                                if let Some(value_position) = full_line[arrow_position..].find(value_part) {
                                    return (error_line - 1, arrow_position + value_position + 1);
                                }
                            }
                        }
                    }
                }
            }
        }

        (error_line, 1)
    }

    fn find_agent_context(&self, lines: &[&str], error_line: usize) -> Option<(String, usize)> {
        let mut brace_count = 0;

        for index in (0..error_line).rev() {
            let line = lines[index].trim();

            if line.ends_with('}') {
                brace_count += 1;
            }

            if line.ends_with('{') {
                if brace_count == 0 {
                    if line.starts_with("agent ") || line.starts_with("<- agent ") {
                        let agent_line = line.trim_start_matches("<-").trim();
                        if let Some(name_part) = agent_line.strip_prefix("agent ") {
                            let agent_name = name_part.trim_end_matches('{').trim().to_string();
                            return Some((agent_name, index + 1));
                        }
                    }
                    return None;
                } else {
                    brace_count -= 1;
                }
            }
        }

        None
    }

    fn find_output_context(&self, lines: &[&str], error_line: usize) -> bool {
        let mut brace_count = 0;

        for index in (0..error_line).rev() {
            let line = lines[index].trim();

            if line.ends_with('}') {
                brace_count += 1;
            }

            if line.ends_with('{') {
                if brace_count == 0 {
                    return line.starts_with("output ");
                } else {
                    brace_count -= 1;
                }
            }
        }

        false
    }

    fn extract_agent_properties(&self, lines: &[&str], start_line: usize, end_line: usize) -> Vec<String> {
        let mut properties = Vec::new();

        for index in start_line..end_line {
            if index >= lines.len() {
                break;
            }

            let line = lines[index].trim();

            if line.contains("<-") {
                if let Some(property_name) = line.split("<-").next() {
                    properties.push(property_name.trim().to_string());
                }
            }
        }

        properties
    }

    fn format_expected_rules(&self, error: &pest::error::Error<Rule>) -> String {
        match &error.variant {
            pest::error::ErrorVariant::ParsingError { positives, .. } => {
                if positives.is_empty() {
                    "unexpected token".to_string()
                } else if positives.len() == 1 {
                    format!("expected {}", self.rule_to_friendly_name(&positives[0]))
                } else {
                    let names: Vec<String> = positives.iter().map(|r| self.rule_to_friendly_name(r)).collect();
                    format!("expected one of: {}", names.join(", "))
                }
            }
            _ => "parsing error".to_string(),
        }
    }

    fn rule_to_friendly_name(&self, rule: &Rule) -> String {
        match rule {
            Rule::string_value => "a string value (e.g., \"text\")".to_string(),
            Rule::agent_property => "an agent property (model, prompt, output, etc.)".to_string(),
            Rule::value => "a value".to_string(),
            Rule::identifier => "an identifier".to_string(),
            Rule::property_prompt => "a prompt property".to_string(),
            Rule::property_model => "a model property".to_string(),
            Rule::property_output => "an output property".to_string(),
            _ => format!("{:?}", rule),
        }
    }

    fn get_source_line(&self, input_str: &str, line_number: usize) -> Option<String> {
        input_str
            .lines()
            .nth(line_number.saturating_sub(1))
            .map(|s| s.to_string())
    }
}
