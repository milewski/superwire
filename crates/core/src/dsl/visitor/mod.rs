mod agent;
mod declaration;
mod expression;
mod mcp;
mod tool;
mod types;

use crate::dsl::ast::{SourcePosition, SourceSpan, Workflow};
use crate::dsl::parser::{DslParseError, Rule};
use pest::iterators::{Pair, Pairs};

#[derive(Debug, Default)]
pub struct AstVisitor;

impl AstVisitor {
    pub fn new() -> Self {
        Self
    }

    pub fn visit_workflow(&self, workflow_pair: Pair<'_, Rule>) -> Result<Workflow, DslParseError> {
        if workflow_pair.as_rule() != Rule::workflow {
            return Err(DslParseError::unexpected_with_span(
                workflow_pair.as_rule(),
                "workflow",
                source_span_from_pair(&workflow_pair),
            ));
        }

        let mut declarations = Vec::new();

        for declaration_pair in workflow_pair.into_inner() {
            if declaration_pair.as_rule() == Rule::EOI {
                continue;
            }

            declarations.push(self.visit_declaration(declaration_pair)?);
        }

        Ok(Workflow {
            declarations,
            source_text: None,
        })
    }

    pub(super) fn parse_string_literal(&self, string_pair: Pair<'_, Rule>) -> Result<String, DslParseError> {
        match string_pair.as_rule() {
            Rule::plain_quoted_string => Ok(self.unescape_quoted_string(string_pair.as_str())),
            Rule::plain_multiline_string => {
                let raw_string = string_pair.as_str();

                if raw_string.len() < 6 {
                    return Ok(String::new());
                }

                Ok(raw_string[3..raw_string.len() - 3].to_owned())
            }
            _ => Err(DslParseError::unexpected_with_span(
                string_pair.as_rule(),
                "string literal",
                source_span_from_pair(&string_pair),
            )),
        }
    }

    pub(super) fn unescape_quoted_string(&self, raw_string: &str) -> String {
        if raw_string.len() < 2 {
            return String::new();
        }

        let mut parsed_string = String::new();
        let mut string_characters = raw_string[1..raw_string.len() - 1].chars();

        while let Some(character) = string_characters.next() {
            if character != '\\' {
                parsed_string.push(character);
                continue;
            }

            let Some(escaped_character) = string_characters.next() else {
                parsed_string.push('\\');
                continue;
            };

            let unescaped_character = match escaped_character {
                'n' => '\n',
                'r' => '\r',
                't' => '\t',
                '\\' => '\\',
                '"' => '"',
                _ => escaped_character,
            };

            parsed_string.push(unescaped_character);
        }

        parsed_string
    }

    pub(super) fn unescape_character(&self, escaped_character: &str) -> String {
        match escaped_character {
            "\\n" => "\n".to_owned(),
            "\\r" => "\r".to_owned(),
            "\\t" => "\t".to_owned(),
            "\\\\" => "\\".to_owned(),
            "\\\"" => "\"".to_owned(),
            "\\{" => "{".to_owned(),
            "\\}" => "}".to_owned(),
            _ => escaped_character.to_owned(),
        }
    }

    pub(super) fn parse_unsigned_integer(&self, integer_pair: Pair<'_, Rule>, context: &'static str) -> Result<u64, DslParseError> {
        let normalized_literal = integer_pair.as_str().replace('_', "");

        normalized_literal.parse::<u64>().map_err(|_| {
            DslParseError::invalid_integer_literal_with_span(integer_pair.as_str(), context, source_span_from_pair(&integer_pair))
        })
    }

    pub(super) fn first_inner_pair<'pair>(
        &self,
        pair: Pair<'pair, Rule>,
        context: &'static str,
    ) -> Result<Pair<'pair, Rule>, DslParseError> {
        let pair_span = source_span_from_pair(&pair);

        pair.into_inner()
            .next()
            .ok_or_else(|| DslParseError::missing_with_span("inner pair", context, pair_span))
    }

    pub(super) fn next_pair<'pair>(
        &self,
        inner_pairs: &mut Pairs<'pair, Rule>,
        expected: &'static str,
        context: &'static str,
    ) -> Result<Pair<'pair, Rule>, DslParseError> {
        inner_pairs.next().ok_or_else(|| DslParseError::missing(expected, context))
    }

    pub(super) fn next_identifier(
        &self,
        inner_pairs: &mut Pairs<'_, Rule>,
        expected: &'static str,
        context: &'static str,
    ) -> Result<String, DslParseError> {
        let identifier_pair = self.next_pair(inner_pairs, expected, context)?;

        if identifier_pair.as_rule() != Rule::identifier {
            return Err(DslParseError::unexpected_with_span(
                identifier_pair.as_rule(),
                context,
                source_span_from_pair(&identifier_pair),
            ));
        }

        Ok(identifier_pair.as_str().to_owned())
    }
}

pub(super) fn source_span_from_pair(pair: &Pair<'_, Rule>) -> SourceSpan {
    let pair_span = pair.as_span();
    let (start_line, start_column) = pair_span.start_pos().line_col();
    let (end_line, end_column) = pair_span.end_pos().line_col();

    SourceSpan {
        start: SourcePosition {
            line: start_line,
            column: start_column,
        },
        end: SourcePosition {
            line: end_line,
            column: end_column,
        },
    }
}
