use super::{source_span_from_pair, AstVisitor};
use crate::dsl::ast::{TypeExpression, TypedField, VariantCase};
use crate::dsl::parser::{DslParseError, Rule};
use pest::iterators::Pair;

impl AstVisitor {
    pub(super) fn visit_tool_binding_type_expression(&self, type_expression_pair: Pair<'_, Rule>) -> Result<TypeExpression, DslParseError> {
        let mut type_terms = Vec::new();

        for type_term_pair in type_expression_pair.into_inner() {
            type_terms.push(self.visit_tool_binding_type_term(type_term_pair)?);
        }

        if type_terms.len() == 1 {
            Ok(type_terms.remove(0))
        } else {
            Ok(TypeExpression::Union(type_terms))
        }
    }

    pub(super) fn visit_tool_binding_type_term(&self, type_term_pair: Pair<'_, Rule>) -> Result<TypeExpression, DslParseError> {
        match type_term_pair.as_rule() {
            Rule::nullable_type => {
                let inner_type_pair = self.first_inner_pair(type_term_pair, "nullable type")?;

                Ok(TypeExpression::nullable(self.visit_tool_binding_type_term(inner_type_pair)?))
            }
            Rule::enum_type => self.visit_enum_type(type_term_pair),
            Rule::variant_type => self.visit_variant_type(type_term_pair),
            Rule::scalar_type => {
                let scalar_type = match type_term_pair.as_str() {
                    "string" => TypeExpression::String,
                    "number" => TypeExpression::Number,
                    "float" => TypeExpression::Float,
                    "boolean" => TypeExpression::Boolean,
                    "object" => TypeExpression::AnyObject,
                    _ => unreachable!("scalar type should be one of the grammar literals"),
                };

                Ok(scalar_type)
            }
            Rule::schema_reference => {
                let mut inner_pairs = type_term_pair.into_inner();
                let schema_name = self.next_identifier(&mut inner_pairs, "schema name", "schema reference")?;
                Ok(TypeExpression::SchemaReference(schema_name))
            }
            Rule::reference => {
                let enum_reference = self.visit_reference(type_term_pair)?;

                Ok(TypeExpression::StringEnumReference(enum_reference))
            }
            Rule::plain_quoted_string | Rule::plain_multiline_string => {
                let enum_value = self.parse_string_literal(type_term_pair)?;
                Ok(TypeExpression::StringEnum(enum_value))
            }
            Rule::array_type => {
                let mut inner_pairs = type_term_pair.into_inner();
                let item_type_pair = self.next_pair(&mut inner_pairs, "array item type", "array type")?;
                let item_type = self.visit_type_expression(item_type_pair)?;

                let fixed_length = if let Some(length_pair) = inner_pairs.next() {
                    Some(self.parse_unsigned_integer(length_pair, "array fixed length")?)
                } else {
                    None
                };

                Ok(TypeExpression::Array {
                    item_type: Box::new(item_type),
                    fixed_length,
                })
            }
            Rule::tuple_type => {
                let mut tuple_items = Vec::new();

                for tuple_item_pair in type_term_pair.into_inner() {
                    tuple_items.push(self.visit_type_expression(tuple_item_pair)?);
                }

                Ok(TypeExpression::Tuple(tuple_items))
            }
            Rule::tool_binding_type_object => {
                let fields = self.visit_typed_block(type_term_pair)?;
                Ok(TypeExpression::Object(fields))
            }
            _ => Err(DslParseError::unexpected_with_span(
                type_term_pair.as_rule(),
                "tool binding type term",
                source_span_from_pair(&type_term_pair),
            )),
        }
    }

    pub(super) fn visit_typed_block(&self, typed_block_pair: Pair<'_, Rule>) -> Result<Vec<TypedField>, DslParseError> {
        let mut typed_fields = Vec::new();

        for typed_field_pair in typed_block_pair.into_inner() {
            typed_fields.push(self.visit_typed_field(typed_field_pair)?);
        }

        Ok(typed_fields)
    }

    pub(super) fn visit_typed_field(&self, typed_field_pair: Pair<'_, Rule>) -> Result<TypedField, DslParseError> {
        let typed_field_span = source_span_from_pair(&typed_field_pair);
        let mut inner_pairs = typed_field_pair.into_inner();
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

        let field_name = field_name.ok_or_else(|| DslParseError::missing_with_span("field name", "typed field", typed_field_span))?;
        let field_type_pair = self.next_pair(&mut inner_pairs, "field type", "typed field")?;
        let field_type = self.visit_type_expression(field_type_pair)?;

        Ok(TypedField {
            name: field_name,
            field_type,
            description: Self::description_from_doc_comments(doc_comments),
            span: typed_field_span,
        })
    }

    pub(super) fn parse_doc_comment(comment_text: &str) -> String {
        comment_text.trim_start_matches("///").trim_start().to_string()
    }

    pub(super) fn description_from_doc_comments(doc_comments: Vec<String>) -> Option<String> {
        (!doc_comments.is_empty()).then(|| doc_comments.join("\n"))
    }

    pub(super) fn visit_type_expression(&self, type_expression_pair: Pair<'_, Rule>) -> Result<TypeExpression, DslParseError> {
        if type_expression_pair.as_rule() != Rule::type_expression {
            return Err(DslParseError::unexpected_with_span(
                type_expression_pair.as_rule(),
                "type expression",
                source_span_from_pair(&type_expression_pair),
            ));
        }

        let mut type_terms = Vec::new();

        for type_term_pair in type_expression_pair.into_inner() {
            type_terms.push(self.visit_type_term(type_term_pair)?);
        }

        if type_terms.len() == 1 {
            Ok(type_terms.remove(0))
        } else {
            Ok(TypeExpression::Union(type_terms))
        }
    }

    pub(super) fn visit_type_term(&self, type_term_pair: Pair<'_, Rule>) -> Result<TypeExpression, DslParseError> {
        match type_term_pair.as_rule() {
            Rule::nullable_type => {
                let inner_type_pair = self.first_inner_pair(type_term_pair, "nullable type")?;

                Ok(TypeExpression::nullable(self.visit_type_term(inner_type_pair)?))
            }
            Rule::enum_type => self.visit_enum_type(type_term_pair),
            Rule::variant_type => self.visit_variant_type(type_term_pair),
            Rule::scalar_type => {
                let scalar_type = match type_term_pair.as_str() {
                    "string" => TypeExpression::String,
                    "number" => TypeExpression::Number,
                    "float" => TypeExpression::Float,
                    "boolean" => TypeExpression::Boolean,
                    "object" => TypeExpression::AnyObject,
                    _ => unreachable!("scalar type should be one of the grammar literals"),
                };

                Ok(scalar_type)
            }
            Rule::schema_reference => {
                let mut inner_pairs = type_term_pair.into_inner();
                let schema_name = self.next_identifier(&mut inner_pairs, "schema name", "schema reference")?;
                Ok(TypeExpression::SchemaReference(schema_name))
            }
            Rule::reference => {
                let enum_reference = self.visit_reference(type_term_pair)?;

                Ok(TypeExpression::StringEnumReference(enum_reference))
            }
            Rule::array_type => {
                let mut inner_pairs = type_term_pair.into_inner();

                let item_type_pair = self.next_pair(&mut inner_pairs, "array item type", "array type")?;
                let item_type = self.visit_type_expression(item_type_pair)?;

                let fixed_length = if let Some(length_pair) = inner_pairs.next() {
                    Some(self.parse_unsigned_integer(length_pair, "array fixed length")?)
                } else {
                    None
                };

                Ok(TypeExpression::Array {
                    item_type: Box::new(item_type),
                    fixed_length,
                })
            }
            Rule::tuple_type => {
                let mut tuple_items = Vec::new();

                for tuple_item_pair in type_term_pair.into_inner() {
                    tuple_items.push(self.visit_type_expression(tuple_item_pair)?);
                }

                Ok(TypeExpression::Tuple(tuple_items))
            }
            Rule::type_object => {
                let fields = self.visit_typed_block(type_term_pair)?;
                Ok(TypeExpression::Object(fields))
            }
            Rule::plain_quoted_string | Rule::plain_multiline_string => {
                let enum_value = self.parse_string_literal(type_term_pair)?;
                Ok(TypeExpression::StringEnum(enum_value))
            }
            _ => unreachable!("type term should map to known type variants"),
        }
    }

    pub(super) fn visit_enum_type(&self, enum_type_pair: Pair<'_, Rule>) -> Result<TypeExpression, DslParseError> {
        let mut enum_values = Vec::new();

        for enum_case_pair in enum_type_pair.into_inner() {
            let enum_value = self.first_inner_pair(enum_case_pair, "enum case")?.as_str().to_owned();

            enum_values.push(TypeExpression::StringEnum(enum_value));
        }

        Ok(TypeExpression::Union(enum_values))
    }

    pub(super) fn visit_variant_type(&self, variant_type_pair: Pair<'_, Rule>) -> Result<TypeExpression, DslParseError> {
        let mut inner_pairs = variant_type_pair.into_inner();
        let discriminator = self.next_identifier(&mut inner_pairs, "variant discriminator", "variant type")?;
        let mut cases = Vec::new();

        for variant_case_pair in inner_pairs {
            let case_span = source_span_from_pair(&variant_case_pair);
            let mut case_pairs = variant_case_pair.into_inner();
            let label_pair = self.next_pair(&mut case_pairs, "variant case label", "variant case")?;
            let name = self.visit_variant_case_label(label_pair)?;
            let object_pair = self.next_pair(&mut case_pairs, "variant case fields", "variant case")?;

            cases.push(VariantCase {
                name,
                fields: self.visit_typed_block(object_pair)?,
                span: case_span,
            });
        }

        Ok(TypeExpression::Variant { discriminator, cases })
    }

    pub(super) fn visit_variant_case_label(&self, label_pair: Pair<'_, Rule>) -> Result<String, DslParseError> {
        let label_pair = self.first_inner_pair(label_pair, "variant case label")?;

        match label_pair.as_rule() {
            Rule::identifier => Ok(label_pair.as_str().to_owned()),
            Rule::plain_quoted_string | Rule::plain_multiline_string => self.parse_string_literal(label_pair),
            _ => Err(DslParseError::unexpected_with_span(
                label_pair.as_rule(),
                "variant case label",
                source_span_from_pair(&label_pair),
            )),
        }
    }
}
