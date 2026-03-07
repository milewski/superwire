use crate::ast::*;
use crate::parser::{DSLParser, Rule};
use pest::Parser;
use anyhow::{Result, anyhow};
use std::collections::HashMap;

pub fn parse_document(input: &str) -> Result<Document> {
    let mut pairs = DSLParser::parse(Rule::document, input)
        .map_err(|e| anyhow!("Parse error: {}", e))?;

    let mut agents = HashMap::new();
    let mut schemas = HashMap::new();
    let mut providers = HashMap::new();

    // Get the document pair and iterate through its inner pairs
    if let Some(document_pair) = pairs.next() {
        for pair in document_pair.into_inner() {
            match pair.as_rule() {
                Rule::agent => {
                    let agent = parse_agent(pair)?;
                    agents.insert(agent.name.clone(), agent);
                }
                Rule::schema => {
                    let schema = parse_schema(pair)?;
                    if let Some(name) = &schema.name {
                        schemas.insert(name.clone(), schema);
                    }
                }
                Rule::provider => {
                    let provider = parse_provider(pair)?;
                    providers.insert(provider.name.clone(), provider);
                }
                Rule::EOI => {}
                _ => {}
            }
        }
    }

    Ok(Document {
        agents,
        schemas,
        providers,
    })
}

fn parse_agent(pair: pest::iterators::Pair<Rule>) -> Result<Agent> {
    let mut inner = pair.into_inner();
    let mut is_terminal = false;
    let mut name = String::new();
    let mut model = None;
    let mut tools = Vec::new();
    let mut context = None;
    let mut output = None;
    let mut prompt = PromptValue::Inline(String::new());
    let mut for_each = None;

    for pair in inner {
        match pair.as_rule() {
            Rule::terminal_marker => {
                is_terminal = true;
            }
            Rule::identifier => {
                if name.is_empty() {
                    name = pair.as_str().to_string();
                }
            }
            Rule::agent_property => {
                // Get the raw text to determine which property this is
                let raw_text = pair.as_str();
                let mut prop_inner = pair.into_inner();

                // Iterate through the inner pairs to find the value
                while let Some(item) = prop_inner.next() {
                    match item.as_rule() {
                        Rule::string => {
                            if raw_text.starts_with("model") {
                                model = Some(parse_string(item)?);
                            } else if raw_text.starts_with("prompt") {
                                prompt = PromptValue::Inline(parse_string(item)?);
                            }
                        }
                        Rule::string_array => {
                            if raw_text.starts_with("tools") {
                                tools = parse_string_array(item)?;
                            }
                        }
                        Rule::context_ref => {
                            context = Some(parse_context_ref(item)?);
                        }
                        Rule::schema_ref => {
                            output = Some(parse_schema_ref(item)?);
                        }
                        Rule::for_each_expr => {
                            for_each = Some(parse_for_each(item)?);
                        }
                        Rule::prompt_value => {
                            if raw_text.starts_with("prompt") {
                                prompt = parse_prompt_value(item)?;
                            }
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }

    Ok(Agent {
        name,
        model,
        tools,
        context,
        output,
        prompt,
        for_each,
        is_terminal,
    })
}

fn parse_schema(pair: pest::iterators::Pair<Rule>) -> Result<Schema> {
    let mut inner = pair.into_inner();
    let mut name = None;
    let mut fields = HashMap::new();

    for pair in inner {
        match pair.as_rule() {
            Rule::identifier => {
                if name.is_none() {
                    name = Some(pair.as_str().to_string());
                }
            }
            Rule::schema_field => {
                let (field_name, field_type) = parse_schema_field(pair)?;
                fields.insert(field_name, field_type);
            }
            _ => {}
        }
    }

    Ok(Schema { name, fields })
}

fn parse_provider(pair: pest::iterators::Pair<Rule>) -> Result<Provider> {
    let mut inner = pair.into_inner();
    let mut name = String::new();
    let mut driver = String::new();
    let mut api_endpoint = String::new();
    let mut models = Vec::new();

    for pair in inner {
        match pair.as_rule() {
            Rule::identifier => {
                if name.is_empty() {
                    name = pair.as_str().to_string();
                }
            }
            Rule::provider_property => {
                // Get the raw text to determine which property this is
                let raw_text = pair.as_str();
                let mut prop_inner = pair.into_inner();

                // The first item should be the keyword (driver, api_endpoint, or models)
                // But since it's a choice in the grammar, we need to check what we got
                while let Some(item) = prop_inner.next() {
                    match item.as_rule() {
                        Rule::string => {
                            // This is a string value, we need to determine which property it belongs to
                            if raw_text.starts_with("driver") {
                                driver = parse_string(item)?;
                            } else if raw_text.starts_with("api_endpoint") {
                                api_endpoint = parse_string(item)?;
                            }
                        }
                        Rule::string_array => {
                            models = parse_string_array(item)?;
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
    })
}

fn parse_string(pair: pest::iterators::Pair<Rule>) -> Result<String> {
    let s = pair.as_str();
    Ok(s.trim_matches('"').to_string())
}

fn parse_string_array(pair: pest::iterators::Pair<Rule>) -> Result<Vec<String>> {
    let mut result = Vec::new();
    for inner_pair in pair.into_inner() {
        if inner_pair.as_rule() == Rule::string {
            result.push(parse_string(inner_pair)?);
        }
    }
    Ok(result)
}

fn parse_context_ref(pair: pest::iterators::Pair<Rule>) -> Result<ContextRef> {
    let text = pair.as_str();
    if text.ends_with(".summary") {
        let agent_name = text.split('.').nth(1).unwrap_or("").to_string();
        Ok(ContextRef::Summary(agent_name))
    } else {
        let agent_name = text.split('.').nth(1).unwrap_or("").to_string();
        Ok(ContextRef::Full(agent_name))
    }
}

fn parse_schema_ref(pair: pest::iterators::Pair<Rule>) -> Result<SchemaRef> {
    let mut inner = pair.into_inner();
    if let Some(first) = inner.next() {
        match first.as_rule() {
            Rule::identifier => {
                Ok(SchemaRef::Named(first.as_str().to_string()))
            }
            Rule::inline_schema => {
                let schema = parse_inline_schema(first)?;
                Ok(SchemaRef::Inline(schema))
            }
            _ => Err(anyhow!("Invalid schema reference"))
        }
    } else {
        Err(anyhow!("Empty schema reference"))
    }
}

fn parse_inline_schema(pair: pest::iterators::Pair<Rule>) -> Result<Schema> {
    let mut fields = HashMap::new();
    for inner_pair in pair.into_inner() {
        if inner_pair.as_rule() == Rule::schema_field {
            let (field_name, field_type) = parse_schema_field(inner_pair)?;
            fields.insert(field_name, field_type);
        }
    }
    Ok(Schema { name: None, fields })
}

fn parse_schema_field(pair: pest::iterators::Pair<Rule>) -> Result<(String, SchemaType)> {
    let mut inner = pair.into_inner();
    let name = inner.next().ok_or_else(|| anyhow!("Missing field name"))?.as_str().to_string();
    let type_pair = inner.next().ok_or_else(|| anyhow!("Missing field type"))?;
    let field_type = parse_schema_type(type_pair)?;
    Ok((name, field_type))
}

fn parse_schema_type(pair: pest::iterators::Pair<Rule>) -> Result<SchemaType> {
    let mut inner = pair.into_inner();
    if let Some(first) = inner.next() {
        match first.as_rule() {
            Rule::schema_primitive => {
                match first.as_str() {
                    "string" => Ok(SchemaType::String),
                    "number" => Ok(SchemaType::Number),
                    "boolean" => Ok(SchemaType::Boolean),
                    "null" => Ok(SchemaType::Null),
                    _ => Err(anyhow!("Unknown primitive type"))
                }
            }
            Rule::schema_array => {
                let inner_type = parse_schema_type(first.into_inner().next().unwrap())?;
                Ok(SchemaType::Array(Box::new(inner_type)))
            }
            Rule::schema_union_type => {
                let mut types = Vec::new();
                for type_pair in first.into_inner() {
                    if type_pair.as_rule() == Rule::schema_primitive {
                        let t = match type_pair.as_str() {
                            "string" => SchemaType::String,
                            "number" => SchemaType::Number,
                            "boolean" => SchemaType::Boolean,
                            "null" => SchemaType::Null,
                            s => SchemaType::Enum(vec![s.to_string()]),
                        };
                        types.push(t);
                    } else if type_pair.as_rule() == Rule::string {
                        types.push(SchemaType::Enum(vec![parse_string(type_pair)?]));
                    }
                }
                Ok(SchemaType::Union(types))
            }
            _ => Err(anyhow!("Unknown schema type"))
        }
    } else {
        Err(anyhow!("Empty schema type"))
    }
}

fn parse_prompt_value(pair: pest::iterators::Pair<Rule>) -> Result<PromptValue> {
    let inner = pair.into_inner().next().ok_or_else(|| anyhow!("Empty prompt value"))?;
    match inner.as_rule() {
        Rule::string => Ok(PromptValue::Inline(parse_string(inner)?)),
        Rule::multiline_string => {
            let s = inner.as_str();
            Ok(PromptValue::Multiline(s.trim_matches('"').to_string()))
        }
        Rule::function_call => Ok(PromptValue::Function(parse_function_call(inner)?)),
        _ => Err(anyhow!("Invalid prompt value"))
    }
}

fn parse_function_call(pair: pest::iterators::Pair<Rule>) -> Result<FunctionCall> {
    let mut inner = pair.into_inner();
    let name = inner.next().ok_or_else(|| anyhow!("Missing function name"))?.as_str().to_string();
    let _path = inner.next(); // Skip the path string
    let mut args = HashMap::new();

    for arg_pair in inner {
        if arg_pair.as_rule() == Rule::function_arg {
            let (arg_name, arg_value) = parse_function_arg(arg_pair)?;
            args.insert(arg_name, arg_value);
        }
    }

    Ok(FunctionCall { name, args })
}

fn parse_function_arg(pair: pest::iterators::Pair<Rule>) -> Result<(String, FunctionArg)> {
    let mut inner = pair.into_inner();
    let name = inner.next().ok_or_else(|| anyhow!("Missing arg name"))?.as_str().to_string();
    let value_pair = inner.next().ok_or_else(|| anyhow!("Missing arg value"))?;

    let value = match value_pair.as_rule() {
        Rule::string => FunctionArg::String(parse_string(value_pair)?),
        Rule::function_call => FunctionArg::Function(Box::new(parse_function_call(value_pair)?)),
        _ => return Err(anyhow!("Invalid function arg type"))
    };

    Ok((name, value))
}

fn parse_for_each(pair: pest::iterators::Pair<Rule>) -> Result<ForEach> {
    let mut inner = pair.into_inner();
    let expr_pair = inner.next().ok_or_else(|| anyhow!("Missing for_each expression"))?;
    let item_name = inner.next().ok_or_else(|| anyhow!("Missing for_each item name"))?.as_str().to_string();

    let collection = parse_expression(expr_pair)?;

    Ok(ForEach {
        collection,
        item_name,
    })
}

fn parse_expression(pair: pest::iterators::Pair<Rule>) -> Result<Expression> {
    let inner = pair.into_inner().next().ok_or_else(|| anyhow!("Empty expression"))?;
    match inner.as_rule() {
        Rule::reference => {
            Ok(Expression::Reference(inner.as_str().to_string()))
        }
        Rule::literal_array => {
            let mut values = Vec::new();
            for lit_pair in inner.into_inner() {
                if lit_pair.as_rule() == Rule::literal {
                    values.push(parse_literal(lit_pair)?);
                }
            }
            Ok(Expression::Literal(values))
        }
        _ => Err(anyhow!("Invalid expression type"))
    }
}

fn parse_literal(pair: pest::iterators::Pair<Rule>) -> Result<serde_json::Value> {
    let inner = pair.into_inner().next().ok_or_else(|| anyhow!("Empty literal"))?;
    match inner.as_rule() {
        Rule::string => Ok(serde_json::Value::String(parse_string(inner)?)),
        Rule::number => {
            let num_str = inner.as_str();
            if num_str.contains('.') {
                Ok(serde_json::Value::Number(serde_json::Number::from_f64(num_str.parse()?).unwrap()))
            } else {
                Ok(serde_json::Value::Number(serde_json::Number::from(num_str.parse::<i64>()?)))
            }
        }
        Rule::boolean => {
            Ok(serde_json::Value::Bool(inner.as_str() == "true"))
        }
        _ => Ok(serde_json::Value::Null)
    }
}
