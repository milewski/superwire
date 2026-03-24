use super::ast::{
    AgentDeclaration, AgentForLoop, AgentProperty, CallArgument, Declaration, Expression, FunctionCall, InputDeclaration, NamedArgument,
    ObjectField, OutputDeclaration, ProviderDeclaration, Reference, ReferenceAccess, ReferenceRoot, SchemaDeclaration, SecretsDeclaration,
    SourcePosition, SourceSpan, StringTemplate, StringTemplatePart, TypeExpression, TypedField, Workflow,
};
use super::parser::{DslParseError, Rule};
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

        Ok(Workflow { declarations })
    }

    fn visit_declaration(&self, declaration_pair: Pair<'_, Rule>) -> Result<Declaration, DslParseError> {
        let declaration_span = source_span_from_pair(&declaration_pair);

        match declaration_pair.as_rule() {
            Rule::declaration => {
                let inner_declaration_pair = self.first_inner_pair(declaration_pair, "declaration")?;
                self.visit_declaration(inner_declaration_pair)
            }
            Rule::provider_declaration => self.visit_provider_declaration(declaration_pair),
            Rule::secrets_declaration => self.visit_secrets_declaration(declaration_pair),
            Rule::input_declaration => self.visit_input_declaration(declaration_pair),
            Rule::schema_declaration => self.visit_schema_declaration(declaration_pair),
            Rule::agent_declaration => self.visit_agent_declaration(declaration_pair),
            Rule::output_declaration => self.visit_output_declaration(declaration_pair),
            _ => Err(DslParseError::unexpected_with_span(
                declaration_pair.as_rule(),
                "declaration",
                declaration_span,
            )),
        }
    }

    fn visit_provider_declaration(&self, provider_pair: Pair<'_, Rule>) -> Result<Declaration, DslParseError> {
        let declaration_span = source_span_from_pair(&provider_pair);
        let mut inner_pairs = provider_pair.into_inner();

        let provider_name = self.next_identifier(&mut inner_pairs, "provider name", "provider declaration")?;
        let object_expression_pair = self.next_pair(&mut inner_pairs, "provider body", "provider declaration")?;
        let properties = self.visit_object_expression(object_expression_pair)?;

        Ok(Declaration::Provider(ProviderDeclaration {
            name: provider_name,
            properties,
            span: declaration_span,
        }))
    }

    fn visit_secrets_declaration(&self, secrets_pair: Pair<'_, Rule>) -> Result<Declaration, DslParseError> {
        let declaration_span = source_span_from_pair(&secrets_pair);
        let mut inner_pairs = secrets_pair.into_inner();

        let typed_block_pair = self.next_pair(&mut inner_pairs, "secrets block", "secrets declaration")?;
        let fields = self.visit_typed_block(typed_block_pair)?;

        Ok(Declaration::Secrets(SecretsDeclaration {
            fields,
            span: declaration_span,
        }))
    }

    fn visit_input_declaration(&self, input_pair: Pair<'_, Rule>) -> Result<Declaration, DslParseError> {
        let declaration_span = source_span_from_pair(&input_pair);
        let mut inner_pairs = input_pair.into_inner();

        let typed_block_pair = self.next_pair(&mut inner_pairs, "input block", "input declaration")?;
        let fields = self.visit_typed_block(typed_block_pair)?;

        Ok(Declaration::Input(InputDeclaration {
            fields,
            span: declaration_span,
        }))
    }

    fn visit_schema_declaration(&self, schema_pair: Pair<'_, Rule>) -> Result<Declaration, DslParseError> {
        let declaration_span = source_span_from_pair(&schema_pair);
        let mut inner_pairs = schema_pair.into_inner();

        let schema_name = self.next_identifier(&mut inner_pairs, "schema name", "schema declaration")?;
        let typed_block_pair = self.next_pair(&mut inner_pairs, "schema block", "schema declaration")?;
        let fields = self.visit_typed_block(typed_block_pair)?;

        Ok(Declaration::Schema(SchemaDeclaration {
            name: schema_name,
            fields,
            span: declaration_span,
        }))
    }

    fn visit_agent_declaration(&self, agent_pair: Pair<'_, Rule>) -> Result<Declaration, DslParseError> {
        let declaration_span = source_span_from_pair(&agent_pair);
        let mut inner_pairs = agent_pair.into_inner();

        let agent_name = self.next_identifier(&mut inner_pairs, "agent name", "agent declaration")?;
        let mut for_loop: Option<AgentForLoop> = None;
        let mut properties = Vec::new();

        for inner_pair in inner_pairs {
            match inner_pair.as_rule() {
                Rule::for_clause => {
                    for_loop = Some(self.visit_for_clause(inner_pair)?);
                }
                Rule::agent_block => {
                    properties = self.visit_agent_block(inner_pair)?;
                }
                _ => unreachable!("agent declaration should include for clause or block"),
            }
        }

        Ok(Declaration::Agent(AgentDeclaration {
            name: agent_name,
            for_loop,
            properties,
            span: declaration_span,
        }))
    }

    fn visit_for_clause(&self, for_clause_pair: Pair<'_, Rule>) -> Result<AgentForLoop, DslParseError> {
        let mut inner_pairs = for_clause_pair.into_inner();

        let iterator_name = self.next_identifier(&mut inner_pairs, "iterator name", "for clause")?;
        let iterable_pair = self.next_pair(&mut inner_pairs, "iterable expression", "for clause")?;
        let iterable = self.visit_expression(iterable_pair)?;

        Ok(AgentForLoop { iterator_name, iterable })
    }

    fn visit_agent_block(&self, agent_block_pair: Pair<'_, Rule>) -> Result<Vec<AgentProperty>, DslParseError> {
        let mut properties = Vec::new();

        for property_pair in agent_block_pair.into_inner() {
            properties.push(self.visit_agent_property(property_pair)?);
        }

        Ok(properties)
    }

    fn visit_agent_property(&self, property_pair: Pair<'_, Rule>) -> Result<AgentProperty, DslParseError> {
        match property_pair.as_rule() {
            Rule::model_property => {
                let expression_pair = self.first_inner_pair(property_pair, "model property")?;
                Ok(AgentProperty::Model(self.visit_expression(expression_pair)?))
            }
            Rule::prompt_property => {
                let expression_pair = self.first_inner_pair(property_pair, "prompt property")?;
                Ok(AgentProperty::Prompt(self.visit_expression(expression_pair)?))
            }
            Rule::output_property => {
                let type_pair = self.first_inner_pair(property_pair, "agent output property")?;
                Ok(AgentProperty::Output(self.visit_type_expression(type_pair)?))
            }
            Rule::context_property => {
                let expression_pair = self.first_inner_pair(property_pair, "context property")?;
                Ok(AgentProperty::Context(self.visit_expression(expression_pair)?))
            }
            Rule::inference_property => {
                let expression_pair = self.first_inner_pair(property_pair, "inference property")?;
                Ok(AgentProperty::Inference(self.visit_expression(expression_pair)?))
            }
            Rule::tools_property => {
                let expression_pair = self.first_inner_pair(property_pair, "tools property")?;
                Ok(AgentProperty::Tools(self.visit_expression(expression_pair)?))
            }
            Rule::custom_property => {
                let mut inner_pairs = property_pair.into_inner();
                let property_name = self.next_identifier(&mut inner_pairs, "custom property name", "custom property")?;
                let expression_pair = self.next_pair(&mut inner_pairs, "custom property value", "custom property")?;
                let value = self.visit_expression(expression_pair)?;

                Ok(AgentProperty::Custom {
                    name: property_name,
                    value,
                })
            }
            _ => unreachable!("agent block should contain only valid agent property rules"),
        }
    }

    fn visit_output_declaration(&self, output_pair: Pair<'_, Rule>) -> Result<Declaration, DslParseError> {
        let declaration_span = source_span_from_pair(&output_pair);
        let mut inner_pairs = output_pair.into_inner();

        let object_expression_pair = self.next_pair(&mut inner_pairs, "output body", "output declaration")?;
        let fields = self.visit_object_expression(object_expression_pair)?;

        Ok(Declaration::Output(OutputDeclaration {
            fields,
            span: declaration_span,
        }))
    }

    fn visit_typed_block(&self, typed_block_pair: Pair<'_, Rule>) -> Result<Vec<TypedField>, DslParseError> {
        let mut typed_fields = Vec::new();

        for typed_field_pair in typed_block_pair.into_inner() {
            typed_fields.push(self.visit_typed_field(typed_field_pair)?);
        }

        Ok(typed_fields)
    }

    fn visit_typed_field(&self, typed_field_pair: Pair<'_, Rule>) -> Result<TypedField, DslParseError> {
        let typed_field_span = source_span_from_pair(&typed_field_pair);
        let mut inner_pairs = typed_field_pair.into_inner();

        let field_name = self.next_identifier(&mut inner_pairs, "field name", "typed field")?;
        let field_type_pair = self.next_pair(&mut inner_pairs, "field type", "typed field")?;
        let field_type = self.visit_type_expression(field_type_pair)?;

        let description = inner_pairs
            .next()
            .map(|description_pair| self.parse_string_literal(description_pair))
            .transpose()?;

        Ok(TypedField {
            name: field_name,
            field_type,
            description,
            span: typed_field_span,
        })
    }

    fn visit_type_expression(&self, type_expression_pair: Pair<'_, Rule>) -> Result<TypeExpression, DslParseError> {
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

    fn visit_type_term(&self, type_term_pair: Pair<'_, Rule>) -> Result<TypeExpression, DslParseError> {
        match type_term_pair.as_rule() {
            Rule::scalar_type => {
                let scalar_type = match type_term_pair.as_str() {
                    "string" => TypeExpression::String,
                    "number" => TypeExpression::Number,
                    "float" => TypeExpression::Float,
                    "boolean" => TypeExpression::Boolean,
                    "null" => TypeExpression::Null,
                    _ => unreachable!("scalar type should be one of the grammar literals"),
                };

                Ok(scalar_type)
            }
            Rule::schema_reference => {
                let mut inner_pairs = type_term_pair.into_inner();
                let schema_name = self.next_identifier(&mut inner_pairs, "schema name", "schema reference")?;
                Ok(TypeExpression::SchemaReference(schema_name))
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

    fn visit_expression(&self, expression_pair: Pair<'_, Rule>) -> Result<Expression, DslParseError> {
        match expression_pair.as_rule() {
            Rule::function_call => Ok(Expression::FunctionCall(self.visit_function_call(expression_pair)?)),
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

    fn visit_object_expression(&self, object_expression_pair: Pair<'_, Rule>) -> Result<Vec<ObjectField>, DslParseError> {
        let mut object_fields = Vec::new();

        for object_field_pair in object_expression_pair.into_inner() {
            object_fields.push(self.visit_object_field(object_field_pair)?);
        }

        Ok(object_fields)
    }

    fn visit_object_field(&self, object_field_pair: Pair<'_, Rule>) -> Result<ObjectField, DslParseError> {
        let mut inner_pairs = object_field_pair.into_inner();

        let field_name = self.next_identifier(&mut inner_pairs, "object field name", "object field")?;
        let expression_pair = self.next_pair(&mut inner_pairs, "object field value", "object field")?;
        let value = self.visit_expression(expression_pair)?;

        Ok(ObjectField { name: field_name, value })
    }

    fn visit_array_expression(&self, array_expression_pair: Pair<'_, Rule>) -> Result<Vec<Expression>, DslParseError> {
        let mut array_values = Vec::new();

        for array_item_pair in array_expression_pair.into_inner() {
            array_values.push(self.visit_expression(array_item_pair)?);
        }

        Ok(array_values)
    }

    fn visit_function_call(&self, function_call_pair: Pair<'_, Rule>) -> Result<FunctionCall, DslParseError> {
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

    fn visit_call_arguments(&self, call_arguments_pair: Pair<'_, Rule>) -> Result<Vec<CallArgument>, DslParseError> {
        let mut arguments = Vec::new();

        for call_argument_pair in call_arguments_pair.into_inner() {
            arguments.push(self.visit_call_argument(call_argument_pair)?);
        }

        Ok(arguments)
    }

    fn visit_call_argument(&self, call_argument_pair: Pair<'_, Rule>) -> Result<CallArgument, DslParseError> {
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
            Rule::function_call
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

    fn visit_reference(&self, reference_pair: Pair<'_, Rule>) -> Result<Reference, DslParseError> {
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

            let optional = match reference_operator_pair.as_str() {
                "." => false,
                "?." => true,
                _ => unreachable!("reference operator should be either . or ?."),
            };

            accesses.push(ReferenceAccess {
                field: next_field_name,
                optional,
            });
        }

        Ok(Reference {
            root: ReferenceRoot::from_identifier(root_identifier),
            accesses,
            span: reference_span,
        })
    }

    fn visit_string_expression(&self, string_expression_pair: Pair<'_, Rule>) -> Result<Expression, DslParseError> {
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

    fn push_string_template_part(
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

    fn parse_string_literal(&self, string_pair: Pair<'_, Rule>) -> Result<String, DslParseError> {
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

    fn unescape_quoted_string(&self, raw_string: &str) -> String {
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

    fn unescape_character(&self, escaped_character: &str) -> String {
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

    fn parse_unsigned_integer(&self, integer_pair: Pair<'_, Rule>, context: &'static str) -> Result<u64, DslParseError> {
        let normalized_literal = integer_pair.as_str().replace('_', "");

        normalized_literal.parse::<u64>().map_err(|_| {
            DslParseError::invalid_integer_literal_with_span(integer_pair.as_str(), context, source_span_from_pair(&integer_pair))
        })
    }

    fn first_inner_pair<'pair>(&self, pair: Pair<'pair, Rule>, context: &'static str) -> Result<Pair<'pair, Rule>, DslParseError> {
        let pair_span = source_span_from_pair(&pair);

        pair.into_inner()
            .next()
            .ok_or_else(|| DslParseError::missing_with_span("inner pair", context, pair_span))
    }

    fn next_pair<'pair>(
        &self,
        inner_pairs: &mut Pairs<'pair, Rule>,
        expected: &'static str,
        context: &'static str,
    ) -> Result<Pair<'pair, Rule>, DslParseError> {
        inner_pairs.next().ok_or_else(|| DslParseError::missing(expected, context))
    }

    fn next_identifier(
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

fn source_span_from_pair(pair: &Pair<'_, Rule>) -> SourceSpan {
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
