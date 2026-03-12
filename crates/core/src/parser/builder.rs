use crate::ast::{
    Agent, AgentProperty, FunctionCall, InputBlock, InputField, NamedSchema, OutputBlock, OutputField,
    Reference, Schema, SchemaField, SchemaReference, SchemaType, Span, Value, Workflow,
};
use crate::parser::ast_constructor::AstConstructor;
use crate::parser::error::ParserError;
use crate::parser::error_analyzer::ErrorAnalyzer;
use crate::parser::{Rule, WorkflowParser};
use pest::Parser;
use std::collections::HashMap;

pub struct AstBuilder {
    file_path: String,
}

impl AstBuilder {
    #[must_use]
    pub const fn new(file_path: String) -> Self {
        Self { file_path }
    }

    pub fn parse(&self, input_str: &str) -> Result<Workflow, ParserError> {
        let pairs = WorkflowParser::parse(Rule::workflow, input_str)
            .map_err(|error| self.enhance_pest_error(error, input_str))?;

        let constructor = AstConstructor::new(self.file_path.clone());
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
                            providers.push(constructor.parse_provider(inner_pair)?);
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
                    return Ok(Value::MultilineString(self.parse_multiline_string(inner_pair)?));
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
                Rule::tool_reference => {
                    let mut tool_name = String::new();

                    for ref_inner in inner_pair.into_inner() {
                        if ref_inner.as_rule() == Rule::identifier {
                            tool_name = ref_inner.as_str().to_string();
                            break;
                        }
                    }

                    return Ok(Value::Reference(Reference::Tool { name: tool_name }));
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
        let text = pair.as_str();

        if text.starts_with('"') && text.ends_with('"') && text.len() >= 2 {
            return Ok(text[1..text.len() - 1].to_string());
        }

        Ok(text.to_string())
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

                    return Ok(SchemaReference::Inline(Schema { fields, span }));
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
        let analyzer = ErrorAnalyzer::new(self.file_path.clone());
        let analyzed = analyzer.analyze(&error, input_str);

        ParserError::syntax_error_with_source(
            self.file_path.clone(),
            analyzed.line,
            analyzed.column,
            analyzed.message,
            analyzed.suggestion,
            analyzed.source_line,
        )
    }
}
