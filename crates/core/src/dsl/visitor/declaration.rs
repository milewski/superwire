use super::{source_span_from_pair, AstVisitor};
use crate::dsl::ast::{
    Declaration, DynamicBlock, Expression, InputDeclaration, ModelDeclaration, ObjectField, OutputDeclaration, ProviderDeclaration,
    SchemaDeclaration, SecretsDeclaration,
};
use crate::dsl::parser::{DslParseError, Rule};
use pest::iterators::Pair;

impl AstVisitor {
    pub(super) fn visit_declaration(&self, declaration_pair: Pair<'_, Rule>) -> Result<Declaration, DslParseError> {
        let declaration_span = source_span_from_pair(&declaration_pair);

        match declaration_pair.as_rule() {
            Rule::declaration => {
                let inner_declaration_pair = self.first_inner_pair(declaration_pair, "declaration")?;
                self.visit_declaration(inner_declaration_pair)
            }
            Rule::provider_declaration => self.visit_provider_declaration(declaration_pair),
            Rule::model_declaration => self.visit_model_declaration(declaration_pair),
            Rule::mcp_declaration => self.visit_mcp_declaration(declaration_pair),
            Rule::secrets_declaration => self.visit_secrets_declaration(declaration_pair),
            Rule::input_declaration => self.visit_input_declaration(declaration_pair),
            Rule::schema_declaration => self.visit_schema_declaration(declaration_pair),
            Rule::mcp_tool_batch_import_declaration => self.visit_mcp_tool_batch_import_declaration(declaration_pair),
            Rule::mcp_resource_batch_import_declaration => self.visit_mcp_resource_batch_import_declaration(declaration_pair),
            Rule::mcp_prompt_batch_import_declaration => self.visit_mcp_prompt_batch_import_declaration(declaration_pair),
            Rule::mcp_batch_import_declaration => self.visit_mcp_batch_import_declaration(declaration_pair),
            Rule::tool_block_declaration => self.visit_tool_block_declaration(declaration_pair),
            Rule::tool_import_declaration => self.visit_tool_import_declaration(declaration_pair),
            Rule::resource_import_declaration => self.visit_resource_import_declaration(declaration_pair),
            Rule::prompt_import_declaration => self.visit_prompt_import_declaration(declaration_pair),
            Rule::dynamic_declaration => self.visit_dynamic_declaration(declaration_pair).map(Declaration::Dynamic),
            Rule::agent_declaration => self.visit_agent_declaration(declaration_pair),
            Rule::output_declaration => self.visit_output_declaration(declaration_pair),
            _ => Err(DslParseError::unexpected_with_span(
                declaration_pair.as_rule(),
                "declaration",
                declaration_span,
            )),
        }
    }

    pub(super) fn visit_provider_declaration(&self, provider_pair: Pair<'_, Rule>) -> Result<Declaration, DslParseError> {
        let declaration_span = source_span_from_pair(&provider_pair);
        let mut inner_pairs = provider_pair.into_inner();

        let provider_name = self.next_identifier(&mut inner_pairs, "provider name", "provider declaration")?;
        let driver_name = self.next_identifier(&mut inner_pairs, "provider driver", "provider declaration")?;
        let config_block_pair = self.next_pair(&mut inner_pairs, "provider body", "provider declaration")?;
        let properties = self.visit_config_block(config_block_pair)?;

        Ok(Declaration::Provider(ProviderDeclaration {
            name: provider_name,
            driver_name,
            properties,
            span: declaration_span,
        }))
    }

    pub(super) fn visit_model_declaration(&self, model_pair: Pair<'_, Rule>) -> Result<Declaration, DslParseError> {
        let declaration_span = source_span_from_pair(&model_pair);
        let mut inner_pairs = model_pair.into_inner();

        let model_name = self.next_identifier(&mut inner_pairs, "model name", "model declaration")?;
        let provider_name = self.next_identifier(&mut inner_pairs, "model provider", "model declaration")?;
        let config_block_pair = self.next_pair(&mut inner_pairs, "model body", "model declaration")?;
        let properties = self.visit_config_block(config_block_pair)?;

        Ok(Declaration::Model(ModelDeclaration {
            name: model_name,
            provider_name,
            properties,
            span: declaration_span,
        }))
    }

    pub(super) fn visit_config_block(&self, config_block_pair: Pair<'_, Rule>) -> Result<Vec<ObjectField>, DslParseError> {
        let mut fields = Vec::new();

        for property_pair in config_block_pair.into_inner() {
            fields.push(self.visit_config_property(property_pair)?);
        }

        Ok(fields)
    }

    pub(super) fn visit_config_property(&self, property_pair: Pair<'_, Rule>) -> Result<ObjectField, DslParseError> {
        match property_pair.as_rule() {
            Rule::object_field => self.visit_object_field(property_pair),
            Rule::named_object_property => self.visit_named_object_property_as_field(property_pair),
            _ => Err(DslParseError::unexpected_with_span(
                property_pair.as_rule(),
                "config property",
                source_span_from_pair(&property_pair),
            )),
        }
    }

    pub(super) fn visit_named_object_property_as_field(&self, property_pair: Pair<'_, Rule>) -> Result<ObjectField, DslParseError> {
        let property_span = source_span_from_pair(&property_pair);
        let mut inner_pairs = property_pair.into_inner();
        let property_name = self.next_identifier(&mut inner_pairs, "property name", "object block property")?;
        let object_expression_pair = self.next_pair(&mut inner_pairs, "property body", "object block property")?;
        let fields = self.visit_object_expression(object_expression_pair)?;

        Ok(ObjectField {
            name: property_name,
            value: Expression::ObjectLiteral(fields),
            span: property_span,
        })
    }

    pub(super) fn visit_secrets_declaration(&self, secrets_pair: Pair<'_, Rule>) -> Result<Declaration, DslParseError> {
        let declaration_span = source_span_from_pair(&secrets_pair);
        let mut inner_pairs = secrets_pair.into_inner();

        let typed_block_pair = self.next_pair(&mut inner_pairs, "secrets block", "secrets declaration")?;
        let fields = self.visit_typed_block(typed_block_pair)?;

        Ok(Declaration::Secrets(SecretsDeclaration {
            fields,
            span: declaration_span,
        }))
    }

    pub(super) fn visit_input_declaration(&self, input_pair: Pair<'_, Rule>) -> Result<Declaration, DslParseError> {
        let declaration_span = source_span_from_pair(&input_pair);
        let mut inner_pairs = input_pair.into_inner();

        let typed_block_pair = self.next_pair(&mut inner_pairs, "input block", "input declaration")?;
        let fields = self.visit_typed_block(typed_block_pair)?;

        Ok(Declaration::Input(InputDeclaration {
            fields,
            span: declaration_span,
        }))
    }

    pub(super) fn visit_schema_declaration(&self, schema_pair: Pair<'_, Rule>) -> Result<Declaration, DslParseError> {
        let declaration_span = source_span_from_pair(&schema_pair);
        let mut inner_pairs = schema_pair.into_inner();

        let schema_name = self.next_identifier(&mut inner_pairs, "schema name", "schema declaration")?;
        let schema_body_pair = self.next_pair(&mut inner_pairs, "schema body", "schema declaration")?;
        let (fields, root_variant) = match schema_body_pair.as_rule() {
            Rule::typed_block => (self.visit_typed_block(schema_body_pair)?, None),
            Rule::object_level_variant_type => {
                let variant_pair = self.first_inner_pair(schema_body_pair, "object-level variant")?;

                (Vec::new(), Some(self.visit_variant_type(variant_pair)?))
            }
            _ => {
                return Err(DslParseError::unexpected_with_span(
                    schema_body_pair.as_rule(),
                    "schema body",
                    source_span_from_pair(&schema_body_pair),
                ));
            }
        };

        Ok(Declaration::Schema(SchemaDeclaration {
            name: schema_name,
            fields,
            root_variant,
            span: declaration_span,
        }))
    }

    pub(super) fn visit_dynamic_declaration(&self, dynamic_pair: Pair<'_, Rule>) -> Result<DynamicBlock, DslParseError> {
        let declaration_span = source_span_from_pair(&dynamic_pair);
        let object_expression_pair = self.first_inner_pair(dynamic_pair, "dynamic declaration")?;
        let fields = self.visit_object_expression(object_expression_pair)?;

        Ok(DynamicBlock {
            fields,
            span: declaration_span,
        })
    }
    pub(super) fn visit_output_declaration(&self, output_pair: Pair<'_, Rule>) -> Result<Declaration, DslParseError> {
        let declaration_span = source_span_from_pair(&output_pair);
        let mut inner_pairs = output_pair.into_inner();

        let object_expression_pair = self.next_pair(&mut inner_pairs, "output body", "output declaration")?;
        let fields = self.visit_object_expression(object_expression_pair)?;

        Ok(Declaration::Output(OutputDeclaration {
            fields,
            span: declaration_span,
        }))
    }
}
