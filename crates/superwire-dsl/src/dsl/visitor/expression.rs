use super::{source_span_from_pair, AstVisitor};
use crate::dsl::ast::{
    Asset, CallArgument, Expression, FunctionCall, MatchBranch, MatchExpression, NamedArgument, NullFallbackExpression, ObjectField,
    Reference, ReferenceAccess, ReferenceAccessKind, ReferenceRoot, StringTemplate, StringTemplatePart, VariantProjectionExpression,
};
use crate::dsl::parser::{DslParseError, Rule};
use pest::iterators::Pair;

impl AstVisitor {
    pub(super) fn visit_expression(&self, expression_pair: Pair<'_, Rule>) -> Result<Expression, DslParseError> {
        match expression_pair.as_rule() {
            Rule::fallback_expression => self.visit_fallback_expression(expression_pair),
            Rule::match_expression => Ok(Expression::Match(self.visit_match_expression(expression_pair)?)),
            Rule::variant_projection_expression => Ok(Expression::VariantProjection(
                self.visit_variant_projection_expression(expression_pair)?,
            )),
            Rule::agent_context_expression | Rule::explicit_agent_context | Rule::compact_agent_context => {
                Ok(Expression::AgentContext(self.visit_agent_context_value(expression_pair)?))
            }
            Rule::asset_expression => Ok(Expression::Asset(self.visit_asset_expression(expression_pair)?)),
            Rule::function_call => Ok(Expression::FunctionCall(self.visit_function_call(expression_pair)?)),
            Rule::tool_call_expression => Ok(Expression::ToolCall(self.visit_tool_call_expression(expression_pair)?)),
            Rule::mcp_call_expression => Ok(Expression::McpCall(self.visit_mcp_call_expression(expression_pair)?)),
            Rule::object_expression => Ok(Expression::ObjectLiteral(self.visit_object_expression(expression_pair)?)),
            Rule::array_expression => Ok(Expression::ArrayLiteral(self.visit_array_expression(expression_pair)?)),
            Rule::boolean_literal => Ok(Expression::BooleanLiteral(expression_pair.as_str() == "true")),
            Rule::null_literal => Ok(Expression::NullLiteral),
            Rule::number_literal => Ok(Expression::NumberLiteral(expression_pair.as_str().to_owned())),
            Rule::string_expression | Rule::quoted_string_expression | Rule::multiline_string_expression => {
                self.visit_string_expression(expression_pair)
            }
            Rule::reference => Ok(Expression::Reference(self.visit_reference(expression_pair)?)),
            _ => Err(DslParseError::unexpected_with_span(
                expression_pair.as_rule(),
                "expression",
                source_span_from_pair(&expression_pair),
            )),
        }
    }

    pub(super) fn visit_fallback_expression(&self, fallback_expression_pair: Pair<'_, Rule>) -> Result<Expression, DslParseError> {
        let mut inner_pairs = fallback_expression_pair.into_inner();
        let value_pair = self.next_pair(&mut inner_pairs, "fallback value", "fallback expression")?;
        let value = self.visit_expression(value_pair)?;

        let Some(fallback_pair) = inner_pairs.next() else {
            return Ok(value);
        };
        let fallback = self.visit_expression(fallback_pair)?;

        Ok(Expression::NullFallback(NullFallbackExpression {
            value: Box::new(value),
            fallback: Box::new(fallback),
        }))
    }

    pub(super) fn visit_variant_projection_expression(
        &self,
        variant_projection_pair: Pair<'_, Rule>,
    ) -> Result<VariantProjectionExpression, DslParseError> {
        let span = source_span_from_pair(&variant_projection_pair);
        let mut inner_pairs = variant_projection_pair.into_inner();
        let value_pair = self.next_pair(&mut inner_pairs, "variant projection value", "variant projection")?;
        let value = self.visit_reference(value_pair)?;
        let case_name = self.next_identifier(&mut inner_pairs, "variant projection case", "variant projection")?;
        let field_path = inner_pairs.map(|field_pair| field_pair.as_str().to_owned()).collect();

        Ok(VariantProjectionExpression {
            value,
            case_name,
            field_path,
            span,
        })
    }

    pub(super) fn visit_asset_expression(&self, asset_pair: Pair<'_, Rule>) -> Result<Asset, DslParseError> {
        let span = source_span_from_pair(&asset_pair);
        let mut inner_pairs = asset_pair.into_inner();
        let source_pair = self.next_pair(&mut inner_pairs, "asset source", "asset expression")?;
        let source = self.visit_expression(source_pair)?;
        let options = if let Some(asset_block_pair) = inner_pairs.next() {
            self.visit_asset_block(asset_block_pair)?
        } else {
            Vec::new()
        };

        Ok(Asset {
            source: Box::new(source),
            options,
            span,
        })
    }

    pub(super) fn visit_asset_block(&self, asset_block_pair: Pair<'_, Rule>) -> Result<Vec<ObjectField>, DslParseError> {
        let mut options = Vec::new();

        for asset_property_pair in asset_block_pair.into_inner() {
            options.push(self.visit_object_field(asset_property_pair)?);
        }

        Ok(options)
    }

    pub(super) fn visit_match_expression(&self, match_expression_pair: Pair<'_, Rule>) -> Result<MatchExpression, DslParseError> {
        let span = source_span_from_pair(&match_expression_pair);
        let mut inner_pairs = match_expression_pair.into_inner();
        let value_pair = self.next_pair(&mut inner_pairs, "match value", "match expression")?;
        let value = self.visit_expression(value_pair)?;
        let mut branches = Vec::new();

        for branch_pair in inner_pairs {
            branches.push(self.visit_match_branch(branch_pair)?);
        }

        Ok(MatchExpression {
            value: Box::new(value),
            branches,
            span,
        })
    }

    pub(super) fn visit_match_branch(&self, branch_pair: Pair<'_, Rule>) -> Result<MatchBranch, DslParseError> {
        let span = source_span_from_pair(&branch_pair);

        match branch_pair.as_rule() {
            Rule::match_fallback_branch => {
                let value_pair = self.first_inner_pair(branch_pair, "match fallback branch")?;

                Ok(MatchBranch::Fallback {
                    value: self.visit_expression(value_pair)?,
                    span,
                })
            }
            Rule::match_variant_branch => {
                let mut inner_pairs = branch_pair.into_inner();
                let label_pair = self.next_pair(&mut inner_pairs, "match case label", "match branch")?;
                let case_name = self.visit_variant_case_label(label_pair)?;
                let field_path = inner_pairs.map(|field_pair| field_pair.as_str().to_owned()).collect();

                Ok(MatchBranch::Variant {
                    case_name,
                    field_path,
                    span,
                })
            }
            _ => Err(DslParseError::unexpected_with_span(
                branch_pair.as_rule(),
                "match branch",
                source_span_from_pair(&branch_pair),
            )),
        }
    }

    pub(super) fn visit_object_expression(&self, object_expression_pair: Pair<'_, Rule>) -> Result<Vec<ObjectField>, DslParseError> {
        let mut object_fields = Vec::new();

        for object_field_pair in object_expression_pair.into_inner() {
            object_fields.push(self.visit_object_field(object_field_pair)?);
        }

        Ok(object_fields)
    }

    pub(super) fn visit_object_field(&self, object_field_pair: Pair<'_, Rule>) -> Result<ObjectField, DslParseError> {
        let object_field_span = source_span_from_pair(&object_field_pair);
        let mut inner_pairs = object_field_pair.into_inner();

        let field_name_pair = self.next_pair(&mut inner_pairs, "object field name", "object field")?;
        let field_name = self.visit_object_field_name(field_name_pair)?;
        let expression_pair = self.next_pair(&mut inner_pairs, "object field value", "object field")?;
        let value = self.visit_expression(expression_pair)?;

        Ok(ObjectField {
            name: field_name,
            value,
            span: object_field_span,
        })
    }

    pub(super) fn visit_object_field_name(&self, object_field_name_pair: Pair<'_, Rule>) -> Result<String, DslParseError> {
        let mut inner_pairs = object_field_name_pair.into_inner();
        let field_name_pair = self.next_pair(&mut inner_pairs, "object field name", "object field")?;

        match field_name_pair.as_rule() {
            Rule::identifier => Ok(field_name_pair.as_str().to_owned()),
            Rule::plain_quoted_string | Rule::plain_multiline_string => self.parse_string_literal(field_name_pair),
            _ => Err(DslParseError::unexpected_with_span(
                field_name_pair.as_rule(),
                "object field name",
                source_span_from_pair(&field_name_pair),
            )),
        }
    }

    pub(super) fn visit_array_expression(&self, array_expression_pair: Pair<'_, Rule>) -> Result<Vec<Expression>, DslParseError> {
        let mut array_values = Vec::new();

        for array_item_pair in array_expression_pair.into_inner() {
            array_values.push(self.visit_expression(array_item_pair)?);
        }

        Ok(array_values)
    }

    pub(super) fn visit_function_call(&self, function_call_pair: Pair<'_, Rule>) -> Result<FunctionCall, DslParseError> {
        let mut inner_pairs = function_call_pair.into_inner();

        let callee_pair = self.next_pair(&mut inner_pairs, "function callee", "function call")?;
        let callee = self.visit_reference(callee_pair)?;

        let arguments = if let Some(arguments_pair) = inner_pairs.next() {
            self.visit_call_arguments(arguments_pair)?
        } else {
            Vec::new()
        };

        Ok(FunctionCall { callee, arguments })
    }

    pub(super) fn visit_call_arguments(&self, call_arguments_pair: Pair<'_, Rule>) -> Result<Vec<CallArgument>, DslParseError> {
        let mut arguments = Vec::new();

        for call_argument_pair in call_arguments_pair.into_inner() {
            arguments.push(self.visit_call_argument(call_argument_pair)?);
        }

        Ok(arguments)
    }

    pub(super) fn visit_call_argument(&self, call_argument_pair: Pair<'_, Rule>) -> Result<CallArgument, DslParseError> {
        if call_argument_pair.as_rule() != Rule::call_argument {
            return Err(DslParseError::unexpected_with_span(
                call_argument_pair.as_rule(),
                "call argument",
                source_span_from_pair(&call_argument_pair),
            ));
        }

        let argument_value_pair = self.first_inner_pair(call_argument_pair, "call argument")?;

        match argument_value_pair.as_rule() {
            Rule::named_argument => {
                let mut inner_pairs = argument_value_pair.into_inner();

                let argument_name = self.next_identifier(&mut inner_pairs, "named argument name", "named argument")?;
                let expression_pair = self.next_pair(&mut inner_pairs, "named argument value", "named argument")?;
                let argument_value = self.visit_expression(expression_pair)?;

                Ok(CallArgument::Named(NamedArgument {
                    name: argument_name,
                    value: argument_value,
                }))
            }
            Rule::fallback_expression
            | Rule::match_expression
            | Rule::variant_projection_expression
            | Rule::agent_context_expression
            | Rule::explicit_agent_context
            | Rule::compact_agent_context
            | Rule::asset_expression
            | Rule::function_call
            | Rule::mcp_call_expression
            | Rule::tool_call_expression
            | Rule::object_expression
            | Rule::array_expression
            | Rule::boolean_literal
            | Rule::null_literal
            | Rule::number_literal
            | Rule::string_expression
            | Rule::quoted_string_expression
            | Rule::multiline_string_expression
            | Rule::reference => Ok(CallArgument::Positional(self.visit_expression(argument_value_pair)?)),
            _ => Err(DslParseError::unexpected_with_span(
                argument_value_pair.as_rule(),
                "call argument value",
                source_span_from_pair(&argument_value_pair),
            )),
        }
    }

    pub(super) fn visit_reference(&self, reference_pair: Pair<'_, Rule>) -> Result<Reference, DslParseError> {
        let reference_span = source_span_from_pair(&reference_pair);
        if reference_pair.as_rule() != Rule::reference {
            return Err(DslParseError::unexpected_with_span(
                reference_pair.as_rule(),
                "reference",
                reference_span,
            ));
        }

        let mut inner_pairs = reference_pair.into_inner();

        let root_identifier = self.next_identifier(&mut inner_pairs, "reference root", "reference")?;
        let mut accesses = Vec::new();

        while let Some(reference_operator_pair) = inner_pairs.next() {
            let next_field_name = self.next_identifier(&mut inner_pairs, "reference field", "reference")?;

            let reference_access_kind = ReferenceAccessKind::from_operator(reference_operator_pair.as_str())
                .expect("reference operator should be a known operator");

            accesses.push(ReferenceAccess {
                field: next_field_name,
                kind: reference_access_kind,
            });
        }

        Ok(Reference {
            root: ReferenceRoot::from_identifier(root_identifier),
            accesses,
            span: reference_span,
        })
    }

    pub(super) fn visit_string_expression(&self, string_expression_pair: Pair<'_, Rule>) -> Result<Expression, DslParseError> {
        let string_container_pair = match string_expression_pair.as_rule() {
            Rule::string_expression => self.first_inner_pair(string_expression_pair, "string expression")?,
            Rule::quoted_string_expression | Rule::multiline_string_expression => string_expression_pair,
            _ => {
                return Err(DslParseError::unexpected_with_span(
                    string_expression_pair.as_rule(),
                    "string expression",
                    source_span_from_pair(&string_expression_pair),
                ));
            }
        };
        let is_multiline_string = string_container_pair.as_rule() == Rule::multiline_string_expression;

        let mut string_template_parts = Vec::new();

        for string_part_pair in string_container_pair.into_inner() {
            match string_part_pair.as_rule() {
                Rule::quoted_string_part | Rule::multiline_string_part => {
                    let nested_part_pair = self.first_inner_pair(string_part_pair, "string part")?;
                    self.push_string_template_part(nested_part_pair, &mut string_template_parts)?;
                }
                Rule::quoted_string_text | Rule::multiline_string_text | Rule::escaped_character | Rule::interpolation => {
                    self.push_string_template_part(string_part_pair, &mut string_template_parts)?;
                }
                _ => {
                    return Err(DslParseError::unexpected_with_span(
                        string_part_pair.as_rule(),
                        "string part",
                        source_span_from_pair(&string_part_pair),
                    ));
                }
            }
        }

        if is_multiline_string {
            string_template_parts = StringTemplate {
                parts: string_template_parts,
            }
            .normalized_multiline_indentation()
            .parts;
        }

        if string_template_parts.is_empty() {
            return Ok(Expression::StringLiteral(String::new()));
        }

        if string_template_parts.iter().all(|part| matches!(part, StringTemplatePart::Text(_))) {
            let mut concatenated_string = String::new();

            for string_template_part in string_template_parts {
                let StringTemplatePart::Text(string_text) = string_template_part else {
                    unreachable!("all string template parts should be text after guard");
                };

                concatenated_string.push_str(&string_text);
            }

            return Ok(Expression::StringLiteral(concatenated_string));
        }

        Ok(Expression::StringTemplate(StringTemplate {
            parts: string_template_parts,
        }))
    }

    pub(super) fn push_string_template_part(
        &self,
        string_part_pair: Pair<'_, Rule>,
        string_template_parts: &mut Vec<StringTemplatePart>,
    ) -> Result<(), DslParseError> {
        match string_part_pair.as_rule() {
            Rule::quoted_string_text | Rule::multiline_string_text => {
                string_template_parts.push(StringTemplatePart::Text(string_part_pair.as_str().to_owned()));
            }
            Rule::escaped_character => {
                string_template_parts.push(StringTemplatePart::Text(self.unescape_character(string_part_pair.as_str())));
            }
            Rule::interpolation => {
                let interpolation_expression_pair = self.first_inner_pair(string_part_pair, "interpolation")?;
                let interpolation_expression = self.visit_expression(interpolation_expression_pair)?;

                string_template_parts.push(StringTemplatePart::Interpolation(interpolation_expression));
            }
            _ => {
                return Err(DslParseError::unexpected_with_span(
                    string_part_pair.as_rule(),
                    "string template part",
                    source_span_from_pair(&string_part_pair),
                ));
            }
        }

        Ok(())
    }
}
