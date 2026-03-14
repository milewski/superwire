use crate::ast::{FunctionCall, Provider, Reference, Span, Value};
use crate::parser::error::ParserError;
use crate::parser::Rule;
use std::collections::HashMap;

/// Handles AST construction from parsed pest pairs
pub struct AstConstructor {
    file_path: String,
}

impl AstConstructor {
    #[must_use]
    pub const fn new(file_path: String) -> Self {
        Self { file_path }
    }

    #[must_use]
    pub fn pair_to_span(&self, pair: &pest::iterators::Pair<Rule>) -> Span {
        let span = pair.as_span();
        let (line, column) = span.start_pos().line_col();

        Span::new(span.start(), span.end(), line, column)
    }

    pub fn parse_provider(&self, pair: pest::iterators::Pair<Rule>) -> Result<Provider, ParserError> {
        let span = self.pair_to_span(&pair);
        let mut name = String::new();
        let mut driver = String::new();
        let mut models = Vec::new();
        let mut config = HashMap::new();

        for inner_pair in pair.into_inner() {
            match inner_pair.as_rule() {
                Rule::identifier if name.is_empty() => {
                    name = inner_pair.as_str().to_string();
                }
                Rule::provider_property => {
                    let full_text = inner_pair.as_str();
                    let property_name = if full_text.starts_with("driver") {
                        "driver"
                    } else if full_text.starts_with("models") {
                        "models"
                    } else if full_text.starts_with("config") {
                        "config"
                    } else {
                        ""
                    };

                    let mut property_value = None;

                    for prop_pair in inner_pair.into_inner() {
                        match prop_pair.as_rule() {
                            Rule::string_value => {
                                property_value = Some(Value::String(self.parse_string_value(prop_pair)?));
                            }
                            Rule::array_value => {
                                property_value = Some(self.parse_array_value(prop_pair)?);
                            }
                            Rule::object_value => {
                                property_value = Some(self.parse_object_value(prop_pair)?);
                            }
                            _ => {}
                        }
                    }

                    if let Some(value) = property_value {
                        match property_name {
                            "driver" => {
                                if let Value::String(string) = value {
                                    driver = string;
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
                            "config" => {
                                if let Value::Object(obj) = value {
                                    config = obj;
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
            models,
            config,
            span,
        })
    }

    pub fn parse_string_value(&self, pair: pest::iterators::Pair<Rule>) -> Result<String, ParserError> {
        let text = pair.as_str();

        if text.starts_with('"') && text.ends_with('"') && text.len() >= 2 {
            return Ok(text[1..text.len() - 1].to_string());
        }

        Ok(text.to_string())
    }

    pub fn parse_array_value(&self, pair: pest::iterators::Pair<Rule>) -> Result<Value, ParserError> {
        let mut values = Vec::new();

        for inner_pair in pair.into_inner() {
            if inner_pair.as_rule() == Rule::value {
                values.push(self.parse_value(inner_pair)?);
            }
        }

        Ok(Value::Array(values))
    }

    pub fn parse_value(&self, pair: pest::iterators::Pair<Rule>) -> Result<Value, ParserError> {
        for inner_pair in pair.into_inner() {
            match inner_pair.as_rule() {
                Rule::string_value => {
                    return Ok(Value::String(self.parse_string_value(inner_pair)?));
                }
                Rule::multiline_string => {
                    return Ok(Value::String(self.parse_multiline_string(inner_pair)?));
                }
                Rule::number_value => {
                    let number_string = inner_pair.as_str();
                    let number = number_string.parse::<f64>().map_err(|error| {
                        ParserError::syntax_error(
                            self.file_path.clone(),
                            0,
                            0,
                            format!("Failed to parse number '{number_string}': {error}"),
                            None,
                        )
                    })?;
                    return Ok(Value::Number(number));
                }
                Rule::boolean_value => {
                    let boolean_value = inner_pair.as_str() == "true";
                    return Ok(Value::Boolean(boolean_value));
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

        Err(ParserError::syntax_error(
            self.file_path.clone(),
            0,
            0,
            "Invalid value".to_string(),
            None,
        ))
    }

    pub fn parse_multiline_string(&self, pair: pest::iterators::Pair<Rule>) -> Result<String, ParserError> {
        let text = pair.as_str();

        if text.starts_with("\"\"\"") && text.ends_with("\"\"\"") && text.len() >= 6 {
            let content = &text[3..text.len() - 3];

            let lines: Vec<&str> = content.lines().collect();

            if lines.is_empty() {
                return Ok(String::new());
            }

            let start_index = usize::from(lines[0].trim().is_empty());
            let end_index = if lines.len() > 1 && lines[lines.len() - 1].trim().is_empty() {
                lines.len() - 1
            } else {
                lines.len()
            };

            if start_index >= end_index {
                return Ok(String::new());
            }

            let content_lines = &lines[start_index..end_index];

            let min_indent = content_lines
                .iter()
                .filter(|line| !line.trim().is_empty())
                .map(|line| line.len() - line.trim_start().len())
                .min()
                .unwrap_or(0);

            let normalized = content_lines
                .iter()
                .map(|line| {
                    if line.trim().is_empty() {
                        ""
                    } else if line.len() >= min_indent {
                        &line[min_indent..]
                    } else {
                        line
                    }
                })
                .collect::<Vec<_>>()
                .join("\n");

            return Ok(normalized);
        }

        let normalized = text
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .collect::<Vec<_>>()
            .join("\n");

        Ok(normalized)
    }

    pub fn parse_object_value(&self, pair: pest::iterators::Pair<Rule>) -> Result<Value, ParserError> {
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

    pub fn parse_reference(&self, pair: pest::iterators::Pair<Rule>) -> Result<Value, ParserError> {
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

    pub fn parse_function_call(&self, pair: pest::iterators::Pair<Rule>) -> Result<Value, ParserError> {
        let span = self.pair_to_span(&pair);
        let mut function_name = String::new();
        let mut arguments = HashMap::new();

        for inner_pair in pair.into_inner() {
            match inner_pair.as_rule() {
                Rule::identifier if function_name.is_empty() => {
                    function_name = inner_pair.as_str().to_string();
                }
                Rule::string_value if !arguments.contains_key("path") => {
                    // First string is the path argument
                    arguments.insert("path".to_string(), Value::String(self.parse_string_value(inner_pair)?));
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

    pub fn parse_interpolated_string(&self, pair: pest::iterators::Pair<Rule>) -> Result<String, ParserError> {
        let text = pair.as_str();

        if text.starts_with('"') && text.ends_with('"') && text.len() >= 2 {
            return Ok(text[1..text.len() - 1].to_string());
        }

        Ok(text.to_string())
    }
}
