use std::ops::Range;

use indexmap::IndexMap;
use pest::iterators::Pair;
use pest::Parser;
use pest_derive::Parser;

use crate::ast::{
    AgentDefinition, ContextSource, Expression, ForEachBinding, FunctionCall, ModelReference, OutputDefinition,
    ProviderDefinition, Reference, SchemaDefinition, SchemaField, SchemaType, WorkflowDocument,
};

pub mod error;
pub mod graph;

use error::ParserError;

#[derive(Parser)]
#[grammar = "grammar.pest"]
struct WorkflowParser;

pub fn parse_workflow(input: &str) -> Result<WorkflowDocument, ParserError> {
    let mut pairs = WorkflowParser::parse(Rule::workflow, input).map_err(|source| ParserError::Grammar {
        message: source.to_string(),
    })?;

    let workflow = pairs.next().ok_or_else(|| ParserError::Grammar {
        message: "missing workflow root".into(),
    })?;

    let mut document = WorkflowDocument::default();

    for pair in workflow.into_inner() {
        match pair.as_rule() {
            Rule::agent_def => document.agents.push(parse_agent(pair)?),
            Rule::schema_def => document.schemas.push(parse_named_schema(pair)?),
            Rule::provider_def => document.providers.push(parse_provider(pair)?),
            Rule::input_def => {
                if document.input.is_some() {
                    return Err(ParserError::DuplicateWorkflowInput);
                }

                document.input = Some(parse_workflow_input(pair)?);
            }
            Rule::output_def => {
                if document.output.is_some() {
                    return Err(ParserError::DuplicateWorkflowOutput);
                }

                document.output = Some(parse_workflow_output(pair)?);
            }
            Rule::EOI => {}
            other => {
                return Err(ParserError::UnexpectedRule {
                    rule: format!("{other:?}"),
                    span: pair.as_span().start()..pair.as_span().end(),
                });
            }
        }
    }

    Ok(document)
}

fn parse_agent(pair: Pair<'_, Rule>) -> Result<AgentDefinition, ParserError> {
    let span = pair.as_span();
    let mut inner = pair.into_inner().peekable();
    let is_terminal = matches!(inner.peek().map(|pair| pair.as_rule()), Some(Rule::terminal_marker));
    if is_terminal {
        inner.next();
    }

    let name = inner
        .next()
        .ok_or_else(|| ParserError::MissingField {
            field: "agent name".into(),
            span: span.start()..span.end(),
        })?
        .as_str()
        .to_owned();

    let block = inner.next().ok_or_else(|| ParserError::MissingField {
        field: format!("block for agent `{name}`"),
        span: span.start()..span.end(),
    })?;

    let mut agent = AgentDefinition {
        name,
        is_terminal,
        model: None,
        tools: Vec::new(),
        context: None,
        output: None,
        prompt: None,
        for_each: None,
        properties: IndexMap::new(),
    };

    for property in block.into_inner() {
        let (property_name, expression) = parse_property(property)?;
        match property_name.as_str() {
            "model" => {
                let raw = expect_string(expression.clone(), "model")?;
                agent.model = Some(parse_model_reference(&raw)?);
                agent.properties.insert(property_name, Expression::String(raw));
            }
            "tools" => {
                let tools = expect_string_array(expression.clone(), "tools")?;
                agent.tools = tools;
                agent.properties.insert(property_name, expression);
            }
            "context" => {
                let reference = expect_reference(expression.clone(), "context")?;
                agent.context = Some(parse_context_source(reference)?);
                agent.properties.insert(property_name, expression);
            }
            "output" => {
                let output = parse_output_definition(expression.clone())?;
                agent.output = Some(output);
                agent.properties.insert(property_name, expression);
            }
            "prompt" => {
                agent.prompt = Some(expression.clone());
                agent.properties.insert(property_name, expression);
            }
            "for_each" => {
                let binding = expect_for_each(expression.clone())?;
                agent.for_each = Some(binding);
                agent.properties.insert(property_name, expression);
            }
            _ => {
                agent.properties.insert(property_name, expression);
            }
        }
    }

    Ok(agent)
}

fn parse_named_schema(pair: Pair<'_, Rule>) -> Result<SchemaDefinition, ParserError> {
    let span = pair.as_span();
    let mut inner = pair.into_inner();
    let name = inner
        .next()
        .ok_or_else(|| ParserError::MissingField {
            field: "schema name".into(),
            span: span.start()..span.end(),
        })?
        .as_str()
        .to_owned();
    let schema_block = inner.next().ok_or_else(|| ParserError::MissingField {
        field: format!("block for schema `{name}`"),
        span: span.start()..span.end(),
    })?;

    Ok(SchemaDefinition {
        name: Some(name),
        fields: parse_schema_fields(schema_block)?,
    })
}

fn parse_workflow_input(pair: Pair<'_, Rule>) -> Result<SchemaDefinition, ParserError> {
    let span = pair.as_span();
    let schema_block = pair.into_inner().next().ok_or_else(|| ParserError::MissingField {
        field: "workflow input block".into(),
        span: span.start()..span.end(),
    })?;

    Ok(SchemaDefinition {
        name: None,
        fields: parse_schema_fields(schema_block)?,
    })
}

fn parse_workflow_output(pair: Pair<'_, Rule>) -> Result<Expression, ParserError> {
    let span = pair.as_span();
    let output_block = pair.into_inner().next().ok_or_else(|| ParserError::MissingField {
        field: "workflow output block".into(),
        span: span.start()..span.end(),
    })?;

    parse_output_object(output_block)
}

fn parse_provider(pair: Pair<'_, Rule>) -> Result<ProviderDefinition, ParserError> {
    let span = pair.as_span();
    let mut inner = pair.into_inner();
    let name = inner
        .next()
        .ok_or_else(|| ParserError::MissingField {
            field: "provider name".into(),
            span: span.start()..span.end(),
        })?
        .as_str()
        .to_owned();
    let block = inner.next().ok_or_else(|| ParserError::MissingField {
        field: format!("block for provider `{name}`"),
        span: span.start()..span.end(),
    })?;

    let mut properties = IndexMap::new();
    let mut driver = None;
    let mut api_endpoint = None;
    let mut models = Vec::new();

    for property in block.into_inner() {
        let (property_name, expression) = parse_property(property)?;
        match property_name.as_str() {
            "driver" => driver = Some(expect_string(expression.clone(), "driver")?),
            "api_endpoint" => api_endpoint = Some(expect_string(expression.clone(), "api_endpoint")?),
            "models" => models = expect_string_array(expression.clone(), "models")?,
            _ => {}
        }

        properties.insert(property_name, expression);
    }

    Ok(ProviderDefinition {
        name,
        driver: driver.unwrap_or_default(),
        api_endpoint,
        models,
        properties,
    })
}

fn parse_property(pair: Pair<'_, Rule>) -> Result<(String, Expression), ParserError> {
    let span = pair.as_span();
    let mut inner = pair.into_inner();
    let name = inner
        .next()
        .ok_or_else(|| ParserError::MissingField {
            field: "property name".into(),
            span: span.start()..span.end(),
        })?
        .as_str()
        .to_owned();
    let value = inner.next().ok_or_else(|| ParserError::MissingField {
        field: format!("value for property `{name}`"),
        span: span.start()..span.end(),
    })?;

    Ok((name, parse_expression(value)?))
}

fn parse_expression(pair: Pair<'_, Rule>) -> Result<Expression, ParserError> {
    match pair.as_rule() {
        Rule::property_value => parse_expression(single_inner(pair)?),
        Rule::string => Ok(Expression::String(unquote(pair.as_str()))),
        Rule::multiline_string => Ok(Expression::MultilineString(unquote_multiline(pair.as_str()))),
        Rule::number => {
            pair.as_str()
                .parse::<f64>()
                .map(Expression::Number)
                .map_err(|source| ParserError::NumberParse {
                    value: pair.as_str().to_owned(),
                    source,
                })
        }
        Rule::boolean => Ok(Expression::Boolean(pair.as_str() == "true")),
        Rule::null => Ok(Expression::Null),
        Rule::identifier => Ok(Expression::Identifier(pair.as_str().to_owned())),
        Rule::reference => Ok(Expression::Reference(parse_reference(pair.as_str()))),
        Rule::array => parse_array(pair),
        Rule::object => parse_object(pair),
        Rule::function_call => parse_function_call(pair),
        Rule::inline_schema => Ok(Expression::InlineSchema(parse_inline_schema(pair)?)),
        Rule::for_each_expr => parse_for_each(pair),
        other => Err(ParserError::UnexpectedRule {
            rule: format!("{other:?}"),
            span: pair.as_span().start()..pair.as_span().end(),
        }),
    }
}

fn parse_array(pair: Pair<'_, Rule>) -> Result<Expression, ParserError> {
    let items = pair.into_inner().map(parse_expression).collect::<Result<Vec<_>, _>>()?;
    Ok(Expression::Array(items))
}

fn parse_object(pair: Pair<'_, Rule>) -> Result<Expression, ParserError> {
    let span = pair.as_span();
    let mut values = IndexMap::new();
    for property in pair.into_inner() {
        let mut inner = property.into_inner();
        let key = inner
            .next()
            .ok_or_else(|| ParserError::MissingField {
                field: "object property key".into(),
                span: span.start()..span.end(),
            })?
            .as_str()
            .to_owned();
        let value = inner.next().ok_or_else(|| ParserError::MissingField {
            field: format!("object property `{key}` value"),
            span: span.start()..span.end(),
        })?;
        values.insert(key, parse_expression(value)?);
    }
    Ok(Expression::Object(values))
}

fn parse_output_object(pair: Pair<'_, Rule>) -> Result<Expression, ParserError> {
    let span = pair.as_span();
    let mut values = IndexMap::new();
    for field in pair.into_inner() {
        let mut inner = field.into_inner();
        let key = inner
            .next()
            .ok_or_else(|| ParserError::MissingField {
                field: "output field key".into(),
                span: span.start()..span.end(),
            })?
            .as_str()
            .to_owned();
        let value = inner.next().ok_or_else(|| ParserError::MissingField {
            field: format!("output field `{key}` value"),
            span: span.start()..span.end(),
        })?;
        values.insert(key, parse_expression(value)?);
    }

    Ok(Expression::Object(values))
}

fn parse_function_call(pair: Pair<'_, Rule>) -> Result<Expression, ParserError> {
    let span = pair.as_span();
    let mut inner = pair.into_inner();
    let name = inner
        .next()
        .ok_or_else(|| ParserError::MissingField {
            field: "function name".into(),
            span: span.start()..span.end(),
        })?
        .as_str()
        .to_owned();
    let target = parse_expression(inner.next().ok_or_else(|| ParserError::MissingField {
        field: format!("target for function `{name}`"),
        span: span.start()..span.end(),
    })?)?;
    let arguments = match parse_object(inner.next().ok_or_else(|| ParserError::MissingField {
        field: format!("argument block for function `{name}`"),
        span: span.start()..span.end(),
    })?)? {
        Expression::Object(arguments) => arguments,
        _ => unreachable!(),
    };

    Ok(Expression::FunctionCall(FunctionCall {
        name,
        target: Box::new(target),
        arguments,
    }))
}

fn parse_for_each(pair: Pair<'_, Rule>) -> Result<Expression, ParserError> {
    let span = pair.as_span();
    let mut inner = pair.into_inner();
    let collection = parse_expression(inner.next().ok_or_else(|| ParserError::MissingField {
        field: "for_each collection".into(),
        span: span.start()..span.end(),
    })?)?;
    let binding = inner
        .next()
        .ok_or_else(|| ParserError::MissingField {
            field: "for_each binding".into(),
            span: span.start()..span.end(),
        })?
        .as_str()
        .to_owned();

    Ok(Expression::ForEach(ForEachBinding {
        collection: Box::new(collection),
        binding,
    }))
}

fn parse_inline_schema(pair: Pair<'_, Rule>) -> Result<SchemaDefinition, ParserError> {
    let schema_block = pair.into_inner().next().ok_or_else(|| ParserError::MissingField {
        field: "inline schema block".into(),
        span: 0..0,
    })?;

    Ok(SchemaDefinition {
        name: None,
        fields: parse_schema_fields(schema_block)?,
    })
}

fn parse_schema_fields(pair: Pair<'_, Rule>) -> Result<Vec<SchemaField>, ParserError> {
    pair.into_inner()
        .filter(|inner| inner.as_rule() == Rule::schema_field)
        .map(parse_schema_field)
        .collect()
}

fn parse_schema_field(pair: Pair<'_, Rule>) -> Result<SchemaField, ParserError> {
    let span = pair.as_span();
    let mut inner = pair.into_inner();
    let name = inner
        .next()
        .ok_or_else(|| ParserError::MissingField {
            field: "schema field name".into(),
            span: span.start()..span.end(),
        })?
        .as_str()
        .to_owned();
    let ty = parse_schema_type(inner.next().ok_or_else(|| ParserError::MissingField {
        field: format!("type for schema field `{name}`"),
        span: span.start()..span.end(),
    })?)?;
    let description = inner.next().map(|pair| unquote(pair.as_str()));

    Ok(SchemaField { name, ty, description })
}

fn parse_schema_type(pair: Pair<'_, Rule>) -> Result<SchemaType, ParserError> {
    match pair.as_rule() {
        Rule::schema_type => parse_schema_type(single_inner(pair)?),
        Rule::schema_array => {
            let inner = pair.into_inner().next().ok_or_else(|| ParserError::MissingField {
                field: "array schema inner type".into(),
                span: 0..0,
            })?;
            Ok(SchemaType::Array(Box::new(parse_schema_type(inner)?)))
        }
        Rule::schema_union => {
            let variants = pair
                .into_inner()
                .map(parse_schema_type)
                .collect::<Result<Vec<_>, _>>()?;
            if variants.len() == 1 {
                Ok(variants.into_iter().next().expect("single variant exists"))
            } else {
                Ok(SchemaType::Union(variants))
            }
        }
        Rule::schema_primary => match pair.as_str() {
            "string" => Ok(SchemaType::String),
            "number" => Ok(SchemaType::Number),
            "boolean" => Ok(SchemaType::Boolean),
            "null" => Ok(SchemaType::Null),
            _ => parse_schema_type(single_inner(pair)?),
        },
        Rule::schema_reference => {
            let identifier = pair
                .into_inner()
                .last()
                .ok_or_else(|| ParserError::MissingField {
                    field: "schema reference identifier".into(),
                    span: 0..0,
                })?
                .as_str()
                .to_owned();
            Ok(SchemaType::Reference(identifier))
        }
        Rule::string => Ok(SchemaType::LiteralString(unquote(pair.as_str()))),
        _ if pair.as_str() == "string" => Ok(SchemaType::String),
        _ if pair.as_str() == "number" => Ok(SchemaType::Number),
        _ if pair.as_str() == "boolean" => Ok(SchemaType::Boolean),
        _ if pair.as_str() == "null" => Ok(SchemaType::Null),
        other => Err(ParserError::UnexpectedRule {
            rule: format!("{other:?}"),
            span: pair.as_span().start()..pair.as_span().end(),
        }),
    }
}

fn parse_output_definition(expression: Expression) -> Result<OutputDefinition, ParserError> {
    match expression {
        Expression::Reference(reference)
            if matches!(reference.segments.first().map(String::as_str), Some("schema")) =>
        {
            let name = reference
                .segments
                .get(1)
                .ok_or_else(|| ParserError::InvalidReference {
                    reference: reference.as_string(),
                    expected: "schema.<name>".into(),
                })?
                .clone();
            Ok(OutputDefinition::SchemaReference(name))
        }
        Expression::InlineSchema(schema) => Ok(OutputDefinition::Inline(schema)),
        other => Err(ParserError::InvalidPropertyType {
            property: "output".into(),
            expected: "schema reference or inline schema".into(),
            actual: expression_kind(&other).into(),
        }),
    }
}

fn parse_model_reference(raw: &str) -> Result<ModelReference, ParserError> {
    let (provider, model) = raw
        .split_once('/')
        .ok_or_else(|| ParserError::InvalidModelReference { value: raw.to_owned() })?;

    Ok(ModelReference {
        provider: provider.to_owned(),
        model: model.to_owned(),
        raw: raw.to_owned(),
    })
}

fn parse_context_source(reference: Reference) -> Result<ContextSource, ParserError> {
    let segments = &reference.segments;
    if segments.len() == 3 && segments[0] == "agent" && segments[2] == "context" {
        Ok(ContextSource::Full(reference))
    } else if segments.len() == 4 && segments[0] == "agent" && segments[2] == "context" && segments[3] == "summary" {
        Ok(ContextSource::Summary(reference))
    } else {
        Err(ParserError::InvalidReference {
            reference: reference.as_string(),
            expected: "agent.<name>.context or agent.<name>.context.summary".into(),
        })
    }
}

fn parse_reference(input: &str) -> Reference {
    Reference {
        segments: input.split('.').map(ToOwned::to_owned).collect(),
    }
}

fn expect_string(expression: Expression, property: &str) -> Result<String, ParserError> {
    match expression {
        Expression::String(value) | Expression::MultilineString(value) | Expression::InterpolatedString(value) => {
            Ok(value)
        }
        other => Err(ParserError::InvalidPropertyType {
            property: property.into(),
            expected: "string".into(),
            actual: expression_kind(&other).into(),
        }),
    }
}

fn expect_string_array(expression: Expression, property: &str) -> Result<Vec<String>, ParserError> {
    match expression {
        Expression::Array(items) => items.into_iter().map(|item| expect_string(item, property)).collect(),
        other => Err(ParserError::InvalidPropertyType {
            property: property.into(),
            expected: "array of strings".into(),
            actual: expression_kind(&other).into(),
        }),
    }
}

fn expect_reference(expression: Expression, property: &str) -> Result<Reference, ParserError> {
    match expression {
        Expression::Reference(reference) => Ok(reference),
        other => Err(ParserError::InvalidPropertyType {
            property: property.into(),
            expected: "reference".into(),
            actual: expression_kind(&other).into(),
        }),
    }
}

fn expect_for_each(expression: Expression) -> Result<ForEachBinding, ParserError> {
    match expression {
        Expression::ForEach(binding) => Ok(binding),
        other => Err(ParserError::InvalidPropertyType {
            property: "for_each".into(),
            expected: "collection as identifier".into(),
            actual: expression_kind(&other).into(),
        }),
    }
}

fn single_inner(pair: Pair<'_, Rule>) -> Result<Pair<'_, Rule>, ParserError> {
    let span = pair.as_span();
    pair.into_inner().next().ok_or_else(|| ParserError::MissingField {
        field: "nested value".into(),
        span: span.start()..span.end(),
    })
}

fn unquote(input: &str) -> String {
    let inner = &input[1..input.len().saturating_sub(1)];
    inner.replace("\\\"", "\"").replace("\\\\", "\\")
}

fn unquote_multiline(input: &str) -> String {
    input.trim_start_matches("\"\"\"").trim_end_matches("\"\"\"").to_owned()
}

fn expression_kind(expression: &Expression) -> &'static str {
    match expression {
        Expression::String(_) => "string",
        Expression::MultilineString(_) => "multiline string",
        Expression::Number(_) => "number",
        Expression::Boolean(_) => "boolean",
        Expression::Null => "null",
        Expression::Array(_) => "array",
        Expression::Object(_) => "object",
        Expression::Identifier(_) => "identifier",
        Expression::Reference(_) => "reference",
        Expression::FunctionCall(_) => "function call",
        Expression::InlineSchema(_) => "inline schema",
        Expression::ForEach(_) => "for_each",
        Expression::InterpolatedString(_) => "interpolated string",
    }
}

#[allow(dead_code)]
fn span_range(pair: &Pair<'_, Rule>) -> Range<usize> {
    pair.as_span().start()..pair.as_span().end()
}
