use crate::ast::{Expression, StringFragment, StringTemplate};
use crate::error::WorkflowError;
use crate::parser::{Rule, WorkflowParser};
use pest::Parser;

pub(crate) fn parse_string_template(raw_literal: &str, is_multiline: bool) -> Result<StringTemplate, WorkflowError> {
    let decoded = if is_multiline {
        decode_multiline_string(raw_literal)
    } else {
        decode_string_literal(raw_literal)?
    };

    let fragments = parse_fragments(&decoded)?;

    Ok(StringTemplate { raw: decoded, fragments })
}

fn decode_string_literal(raw_literal: &str) -> Result<String, WorkflowError> {
    let raw_content = &raw_literal[1..raw_literal.len() - 1];
    let mut decoded = String::with_capacity(raw_content.len());
    let mut characters = raw_content.chars();

    while let Some(character) = characters.next() {
        if character != '\\' {
            decoded.push(character);
            continue;
        }

        let escaped_character = characters
            .next()
            .ok_or_else(|| WorkflowError::parse("string ended with an incomplete escape sequence"))?;

        match escaped_character {
            '\\' => decoded.push('\\'),
            '"' => decoded.push('"'),
            'n' => decoded.push('\n'),
            'r' => decoded.push('\r'),
            't' => decoded.push('\t'),
            _ => return Err(WorkflowError::parse(format!("unsupported escape sequence: \\{escaped_character}"))),
        }
    }

    Ok(decoded)
}

fn decode_multiline_string(raw_literal: &str) -> String {
    let mut content = raw_literal[3..raw_literal.len() - 3].replace("\r\n", "\n");

    if content.starts_with('\n') {
        content.remove(0);
    }

    let mut lines = content.lines().map(str::to_string).collect::<Vec<_>>();

    while lines.first().is_some_and(|line| line.trim().is_empty()) {
        lines.remove(0);
    }

    while lines.last().is_some_and(|line| line.trim().is_empty()) {
        lines.pop();
    }

    let indentation = lines
        .iter()
        .filter(|line| !line.trim().is_empty())
        .map(|line| line.chars().take_while(|character| *character == ' ' || *character == '\t').count())
        .min()
        .unwrap_or(0);

    lines
        .into_iter()
        .map(|line| {
            if line.trim().is_empty() {
                String::new()
            } else {
                line.chars().skip(indentation).collect::<String>()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn parse_fragments(raw_template: &str) -> Result<Vec<StringFragment>, WorkflowError> {
    let mut fragments = Vec::new();
    let mut current_offset = 0;

    while let Some(start_offset) = raw_template[current_offset..].find("{{") {
        let start_index = current_offset + start_offset;

        if start_index > current_offset {
            fragments.push(StringFragment::Text(raw_template[current_offset..start_index].to_string()));
        }

        let expression_start = start_index + 2;
        let end_relative = raw_template[expression_start..]
            .find("}}")
            .ok_or_else(|| WorkflowError::parse("unterminated interpolation segment"))?;
        let end_index = expression_start + end_relative;
        let interpolation_source = raw_template[expression_start..end_index].trim();

        let mut interpolation_pairs = WorkflowParser::parse(Rule::interpolation_root, interpolation_source)
            .map_err(|error| WorkflowError::parse(error.to_string()))?;
        let interpolation_pair = interpolation_pairs.next().expect("interpolation rule should always produce a pair");
        let reference_pair = interpolation_pair
            .into_inner()
            .next()
            .expect("interpolation root should contain a reference expression");
        let expression = super::builder::AstBuilder::build_reference_expression(reference_pair)?;

        fragments.push(StringFragment::Expression(Expression::Reference(expression)));
        current_offset = end_index + 2;
    }

    if current_offset < raw_template.len() {
        fragments.push(StringFragment::Text(raw_template[current_offset..].to_string()));
    }

    Ok(fragments)
}
