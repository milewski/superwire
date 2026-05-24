use super::{source_span_from_pair, AstVisitor};
use crate::dsl::ast::{
    Declaration, Expression, McpToolBatchImportPropertyName, ObjectField, ToolCall, ToolCallPropertyName, ToolDeclaration,
    ToolPropertyName, ToolSource, TypeExpression, TypedField,
};
use crate::dsl::parser::{DslParseError, Rule};
use pest::iterators::Pair;

#[derive(Default)]
pub(super) struct ToolImportBlock {
    pub(super) input_fields: Vec<TypedField>,
    pub(super) fixed_binding_fields: Vec<ObjectField>,
    pub(super) max_calls: Option<u64>,
    pub(super) output_fields: Vec<TypedField>,
}

impl AstVisitor {
    pub(super) fn visit_tool_block_declaration(&self, tool_pair: Pair<'_, Rule>) -> Result<Declaration, DslParseError> {
        let declaration_span = source_span_from_pair(&tool_pair);
        let mut inner_pairs = tool_pair.into_inner();

        let tool_name = self.next_identifier(&mut inner_pairs, "tool name", "tool declaration")?;
        let tool_block_pair = self.next_pair(&mut inner_pairs, "tool block", "tool declaration")?;
        let mut description = None;
        let mut input_fields = Vec::new();
        let mut binding_fields = Vec::new();
        let mut max_calls = None;
        let mut fixed_binding_fields = Vec::new();
        let mut output_fields = Vec::new();

        for tool_property_pair in tool_block_pair.into_inner() {
            match tool_property_pair.as_rule() {
                Rule::named_plain_string_property => {
                    let mut inner_pairs = tool_property_pair.into_inner();
                    let property_name = self.next_identifier(&mut inner_pairs, "tool property name", "tool string property")?;
                    let Some(ToolPropertyName::Description) = ToolPropertyName::from_identifier(property_name.as_str()) else {
                        return Err(DslParseError::unexpected_with_span(
                            Rule::named_plain_string_property,
                            "tool string property",
                            declaration_span,
                        ));
                    };
                    let description_pair = self.next_pair(&mut inner_pairs, "tool description", "tool string property")?;
                    description = Some(self.parse_string_literal(description_pair)?);
                }
                Rule::named_unsigned_integer_property => {
                    let mut inner_pairs = tool_property_pair.into_inner();
                    let property_name = self.next_identifier(&mut inner_pairs, "tool property name", "tool integer property")?;
                    let Some(ToolPropertyName::MaxCalls) = ToolPropertyName::from_identifier(property_name.as_str()) else {
                        return Err(DslParseError::unexpected_with_span(
                            Rule::named_unsigned_integer_property,
                            "tool integer property",
                            declaration_span,
                        ));
                    };
                    let max_calls_pair = self.next_pair(&mut inner_pairs, "tool max calls", "tool integer property")?;
                    max_calls = Some(self.parse_unsigned_integer(max_calls_pair, "tool max calls property")?);
                }
                Rule::named_tool_block_property => {
                    let mut inner_pairs = tool_property_pair.into_inner();
                    let property_name = self.next_identifier(&mut inner_pairs, "tool property name", "tool block property")?;
                    let block_pair = self.next_pair(&mut inner_pairs, "tool block property value", "tool block property")?;

                    match ToolPropertyName::from_identifier(property_name.as_str()) {
                        Some(ToolPropertyName::Input) => input_fields.extend(self.visit_tool_typed_fields_block(block_pair)?),
                        Some(ToolPropertyName::Output) => output_fields.extend(self.visit_tool_typed_fields_block(block_pair)?),
                        Some(ToolPropertyName::Bindings) => {
                            let (typed_fields, fixed_fields) = self.visit_tool_bindings_block(block_pair)?;
                            binding_fields.extend(typed_fields);
                            fixed_binding_fields.extend(fixed_fields);
                        }
                        _ => {
                            return Err(DslParseError::unexpected_with_span(
                                Rule::named_tool_block_property,
                                "tool block property",
                                declaration_span,
                            ));
                        }
                    }
                }
                Rule::tool_input_field => {
                    let typed_field_pair = self.first_inner_pair(tool_property_pair, "tool input field")?;
                    input_fields.push(self.visit_typed_field(typed_field_pair)?);
                }
                _ => unreachable!("tool block should contain only valid tool property rules"),
            }
        }

        Ok(Declaration::Tool(ToolDeclaration {
            name: tool_name,
            description,
            max_calls,
            source: None,
            imported: false,
            input_fields,
            binding_fields,
            fixed_binding_fields,
            output_fields,
            span: declaration_span,
        }))
    }

    pub(super) fn visit_tool_import_declaration(&self, tool_pair: Pair<'_, Rule>) -> Result<Declaration, DslParseError> {
        let declaration_span = source_span_from_pair(&tool_pair);
        let mut inner_pairs = tool_pair.into_inner().peekable();
        let first_pair = inner_pairs
            .next()
            .ok_or_else(|| DslParseError::missing_with_span("MCP import source", "tool import declaration", declaration_span))?;
        let (alias, source_pair) = if first_pair.as_rule() == Rule::identifier {
            let source_pair = inner_pairs
                .next()
                .ok_or_else(|| DslParseError::missing_with_span("MCP import source", "tool import declaration", declaration_span))?;

            (Some(first_pair.as_str().to_owned()), source_pair)
        } else {
            (None, first_pair)
        };
        let source = self.visit_mcp_import_source(source_pair)?;
        let import_block = inner_pairs
            .next()
            .map(|block_pair| self.visit_tool_import_block(block_pair))
            .transpose()?
            .unwrap_or_default();
        let name = alias.unwrap_or_else(|| source.inferred_local_name());

        Ok(Declaration::Tool(ToolDeclaration {
            name,
            description: None,
            max_calls: import_block.max_calls,
            source: Some(ToolSource::Mcp(source.as_tool_source())),
            imported: true,
            input_fields: import_block.input_fields,
            binding_fields: Vec::new(),
            fixed_binding_fields: import_block.fixed_binding_fields,
            output_fields: import_block.output_fields,
            span: declaration_span,
        }))
    }

    pub(super) fn visit_tool_import_block(&self, block_pair: Pair<'_, Rule>) -> Result<ToolImportBlock, DslParseError> {
        let block_span = source_span_from_pair(&block_pair);
        let mut import_block = ToolImportBlock::default();

        for property_pair in block_pair.into_inner() {
            match property_pair.as_rule() {
                Rule::named_object_property => {
                    let mut inner_pairs = property_pair.into_inner();
                    let property_name = self.next_identifier(&mut inner_pairs, "tool import property name", "tool import block")?;
                    let Some(McpToolBatchImportPropertyName::Bindings) =
                        McpToolBatchImportPropertyName::from_identifier(property_name.as_str())
                    else {
                        return Err(DslParseError::unexpected_with_span(
                            Rule::named_object_property,
                            "tool import property",
                            block_span,
                        ));
                    };
                    let object_expression_pair = self.next_pair(&mut inner_pairs, "tool import bindings", "tool import block")?;
                    import_block
                        .fixed_binding_fields
                        .extend(self.visit_object_expression(object_expression_pair)?);
                }
                Rule::named_unsigned_integer_property => {
                    let mut inner_pairs = property_pair.into_inner();
                    let property_name = self.next_identifier(&mut inner_pairs, "tool import property name", "tool import block")?;
                    let Some(McpToolBatchImportPropertyName::MaxCalls) =
                        McpToolBatchImportPropertyName::from_identifier(property_name.as_str())
                    else {
                        return Err(DslParseError::unexpected_with_span(
                            Rule::named_unsigned_integer_property,
                            "tool import property",
                            block_span,
                        ));
                    };
                    let max_calls_pair = self.next_pair(&mut inner_pairs, "tool import max calls", "tool import block")?;
                    import_block.max_calls = Some(self.parse_unsigned_integer(max_calls_pair, "tool import max calls")?);
                }
                Rule::named_tool_block_property => {
                    let mut inner_pairs = property_pair.into_inner();
                    let property_name = self.next_identifier(&mut inner_pairs, "tool import property name", "tool import block")?;
                    let block_pair = self.next_pair(&mut inner_pairs, "tool import block property value", "tool import block")?;

                    match ToolPropertyName::from_identifier(property_name.as_str()) {
                        Some(ToolPropertyName::Input) => {
                            import_block.input_fields.extend(self.visit_tool_typed_fields_block(block_pair)?);
                        }
                        Some(ToolPropertyName::Bindings) => {
                            let (_, fixed_fields) = self.visit_tool_bindings_block(block_pair)?;
                            import_block.fixed_binding_fields.extend(fixed_fields);
                        }
                        Some(ToolPropertyName::Output) => {
                            import_block.output_fields.extend(self.visit_tool_typed_fields_block(block_pair)?);
                        }
                        _ => {
                            return Err(DslParseError::unexpected_with_span(
                                Rule::named_tool_block_property,
                                "tool import property",
                                block_span,
                            ));
                        }
                    }
                }
                _ => unreachable!("tool import block should contain only valid properties"),
            }
        }

        Ok(import_block)
    }

    pub(super) fn visit_tool_bindings_block(
        &self,
        bindings_block_pair: Pair<'_, Rule>,
    ) -> Result<(Vec<TypedField>, Vec<ObjectField>), DslParseError> {
        let mut fixed_fields = Vec::new();

        for binding_field_pair in bindings_block_pair.into_inner() {
            let binding_field_span = source_span_from_pair(&binding_field_pair);
            let mut inner_pairs = binding_field_pair.into_inner();
            let mut field_name = None;

            for inner_pair in inner_pairs.by_ref() {
                if inner_pair.as_rule() == Rule::doc_comment {
                    continue;
                }

                field_name = Some(inner_pair.as_str().to_owned());

                break;
            }

            let field_name = field_name
                .ok_or_else(|| DslParseError::missing_with_span("binding field name", "tool bindings field", binding_field_span))?;
            let field_value_pair = self.next_pair(&mut inner_pairs, "binding field value", "tool bindings field")?;

            match field_value_pair.as_rule() {
                Rule::tool_binding_type_expression => {
                    let field_type = self.visit_tool_binding_type_expression(field_value_pair.clone())?;

                    if let Some(value) = field_type.fixed_binding_literal_expression() {
                        fixed_fields.push(ObjectField {
                            name: field_name,
                            value,
                            span: binding_field_span,
                        });

                        continue;
                    }

                    return Err(DslParseError::unexpected_with_span(
                        field_value_pair.as_rule(),
                        "tool bindings field value",
                        source_span_from_pair(&field_value_pair),
                    ));
                }
                Rule::expression
                | Rule::fallback_expression
                | Rule::match_expression
                | Rule::variant_projection_expression
                | Rule::tool_call_expression
                | Rule::mcp_call_expression
                | Rule::function_call
                | Rule::object_expression
                | Rule::array_expression
                | Rule::boolean_literal
                | Rule::null_literal
                | Rule::number_literal
                | Rule::string_expression
                | Rule::quoted_string_expression
                | Rule::multiline_string_expression
                | Rule::reference => {
                    fixed_fields.push(ObjectField {
                        name: field_name,
                        value: self.visit_expression(field_value_pair)?,
                        span: binding_field_span,
                    });
                }
                _ => {
                    return Err(DslParseError::unexpected_with_span(
                        field_value_pair.as_rule(),
                        "tool bindings field value",
                        source_span_from_pair(&field_value_pair),
                    ));
                }
            }
        }

        Ok((Vec::new(), fixed_fields))
    }

    pub(super) fn visit_tool_typed_fields_block(&self, typed_fields_block_pair: Pair<'_, Rule>) -> Result<Vec<TypedField>, DslParseError> {
        let mut typed_fields = Vec::new();

        for field_pair in typed_fields_block_pair.into_inner() {
            let field_span = source_span_from_pair(&field_pair);
            let mut inner_pairs = field_pair.into_inner();
            let mut doc_comments = Vec::new();
            let mut field_name = None;

            for inner_pair in inner_pairs.by_ref() {
                if inner_pair.as_rule() == Rule::doc_comment {
                    doc_comments.push(Self::parse_doc_comment(inner_pair.as_str()));

                    continue;
                }

                field_name = Some(inner_pair.as_str().to_owned());

                break;
            }

            let field_name = field_name.ok_or_else(|| DslParseError::missing_with_span("field name", "tool typed field", field_span))?;
            let field_value_pair = self.next_pair(&mut inner_pairs, "field type", "tool typed field")?;

            if field_value_pair.as_rule() != Rule::tool_binding_type_expression {
                return Err(DslParseError::unexpected_with_span(
                    field_value_pair.as_rule(),
                    "tool typed field type",
                    source_span_from_pair(&field_value_pair),
                ));
            }

            typed_fields.push(TypedField {
                name: field_name,
                field_type: self.visit_tool_binding_type_expression(field_value_pair)?,
                description: Self::description_from_doc_comments(doc_comments),
                span: field_span,
            });
        }

        Ok(typed_fields)
    }

    pub(super) fn visit_tool_call_expression(&self, tool_call_pair: Pair<'_, Rule>) -> Result<ToolCall, DslParseError> {
        let tool_call_span = source_span_from_pair(&tool_call_pair);
        let mut inner_pairs = tool_call_pair.into_inner();
        let callee_pair = self.next_pair(&mut inner_pairs, "tool call callee", "tool call expression")?;
        let callee = self.visit_reference(callee_pair)?;
        let mut input_fields = Vec::new();
        let mut binding_fields = Vec::new();
        let mut max_calls = None;

        if let Some(block_pair) = inner_pairs.next() {
            let block_span = source_span_from_pair(&block_pair);

            for property_pair in block_pair.into_inner() {
                match property_pair.as_rule() {
                    Rule::named_object_property => {
                        let mut inner_pairs = property_pair.into_inner();
                        let property_name = self.next_identifier(&mut inner_pairs, "tool call property name", "tool call block")?;
                        let object_expression_pair = self.next_pair(&mut inner_pairs, "tool call object property", "tool call block")?;

                        match ToolCallPropertyName::from_identifier(property_name.as_str()) {
                            Some(ToolCallPropertyName::Input) => input_fields.extend(self.visit_object_expression(object_expression_pair)?),
                            Some(ToolCallPropertyName::Bindings) => {
                                binding_fields.extend(self.visit_object_expression(object_expression_pair)?);
                            }
                            _ => {
                                return Err(DslParseError::unexpected_with_span(
                                    Rule::named_object_property,
                                    "tool call property",
                                    block_span,
                                ));
                            }
                        }
                    }
                    Rule::named_unsigned_integer_property => {
                        let mut inner_pairs = property_pair.into_inner();
                        let property_name = self.next_identifier(&mut inner_pairs, "tool call property name", "tool call block")?;
                        let Some(ToolCallPropertyName::MaxCalls) = ToolCallPropertyName::from_identifier(property_name.as_str()) else {
                            return Err(DslParseError::unexpected_with_span(
                                Rule::named_unsigned_integer_property,
                                "tool call property",
                                block_span,
                            ));
                        };
                        let max_calls_pair = self.next_pair(&mut inner_pairs, "tool call max calls", "tool call block")?;
                        max_calls = Some(self.parse_unsigned_integer(max_calls_pair, "tool call max calls property")?);
                    }
                    _ => unreachable!("tool call block should contain only valid tool call property rules"),
                }
            }
        }

        Ok(ToolCall {
            callee,
            input_fields,
            binding_fields,
            max_calls,
            span: tool_call_span,
        })
    }
}

impl TypeExpression {
    fn fixed_binding_literal_expression(self) -> Option<Expression> {
        match self {
            Self::StringEnum(string_value) => Some(Expression::StringLiteral(string_value)),
            Self::StringEnumReference(reference) => Some(Expression::Reference(reference)),
            Self::Array {
                item_type,
                fixed_length: None,
            } => item_type
                .fixed_binding_array_item_expression()
                .map(|array_item| Expression::ArrayLiteral(vec![array_item])),
            Self::String
            | Self::Number
            | Self::Float
            | Self::Boolean
            | Self::Null
            | Self::AnyObject
            | Self::SchemaReference(_)
            | Self::Array {
                item_type: _,
                fixed_length: Some(_),
            }
            | Self::Tuple(_)
            | Self::Object(_)
            | Self::Variant {
                discriminator: _,
                cases: _,
            }
            | Self::Union(_) => None,
        }
    }

    fn fixed_binding_array_item_expression(self) -> Option<Expression> {
        match self {
            Self::StringEnum(string_value) => Some(Expression::StringLiteral(string_value)),
            Self::String
            | Self::Number
            | Self::Float
            | Self::Boolean
            | Self::Null
            | Self::AnyObject
            | Self::SchemaReference(_)
            | Self::StringEnumReference(_)
            | Self::Array {
                item_type: _,
                fixed_length: _,
            }
            | Self::Tuple(_)
            | Self::Object(_)
            | Self::Variant {
                discriminator: _,
                cases: _,
            }
            | Self::Union(_) => None,
        }
    }
}
