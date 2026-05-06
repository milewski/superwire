use super::ast::{
    AgentDeclaration, AgentForLoop, AgentForLoopPattern, AgentProperty, AgentPropertyName, AgentResponseFormat, CallArgument, Declaration,
    DynamicBlock, Expression, FunctionCall, InputDeclaration, McpCall, McpCallOperation, McpImportKind, McpImportPropertyName,
    McpImportSource, McpPromptImportDeclaration, McpResourceImportDeclaration, McpServerDeclaration, McpToolBatchImportDeclaration,
    McpToolBatchImportItem, McpToolBatchImportPropertyName, NamedArgument, ObjectField, OutputDeclaration, ProviderDeclaration, Reference,
    ReferenceAccess, ReferenceRoot, SchemaDeclaration, SecretsDeclaration, SourcePosition, SourceSpan, StringTemplate, StringTemplatePart,
    ToolCall, ToolCallPropertyName, ToolDeclaration, ToolPropertyName, ToolSource, TypeExpression, TypedField, Workflow,
};
use super::parser::{DslParseError, Rule};
use pest::iterators::{Pair, Pairs};

#[derive(Debug, Default)]
pub struct AstVisitor;

struct McpToolBatchImportBlock {
    fixed_binding_fields: Vec<ObjectField>,
    max_calls: Option<u64>,
    import_items: Vec<McpToolBatchImportItem>,
}

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

    fn visit_declaration(&self, declaration_pair: Pair<'_, Rule>) -> Result<Declaration, DslParseError> {
        let declaration_span = source_span_from_pair(&declaration_pair);

        match declaration_pair.as_rule() {
            Rule::declaration => {
                let inner_declaration_pair = self.first_inner_pair(declaration_pair, "declaration")?;
                self.visit_declaration(inner_declaration_pair)
            }
            Rule::provider_declaration => self.visit_provider_declaration(declaration_pair),
            Rule::mcp_declaration => self.visit_mcp_declaration(declaration_pair),
            Rule::secrets_declaration => self.visit_secrets_declaration(declaration_pair),
            Rule::input_declaration => self.visit_input_declaration(declaration_pair),
            Rule::schema_declaration => self.visit_schema_declaration(declaration_pair),
            Rule::mcp_tool_batch_import_declaration => self.visit_mcp_tool_batch_import_declaration(declaration_pair),
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

    fn visit_tool_block_declaration(&self, tool_pair: Pair<'_, Rule>) -> Result<Declaration, DslParseError> {
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

    fn visit_tool_import_declaration(&self, tool_pair: Pair<'_, Rule>) -> Result<Declaration, DslParseError> {
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
        let (fixed_binding_fields, max_calls) = inner_pairs
            .next()
            .map(|block_pair| self.visit_tool_import_block(block_pair))
            .transpose()?
            .unwrap_or_else(|| (Vec::new(), None));
        let name = alias.unwrap_or_else(|| source.inferred_local_name());

        Ok(Declaration::Tool(ToolDeclaration {
            name,
            description: None,
            max_calls,
            source: Some(ToolSource::Mcp(source.as_tool_source())),
            imported: true,
            input_fields: Vec::new(),
            binding_fields: Vec::new(),
            fixed_binding_fields,
            output_fields: Vec::new(),
            span: declaration_span,
        }))
    }

    fn visit_mcp_tool_batch_import_declaration(&self, import_pair: Pair<'_, Rule>) -> Result<Declaration, DslParseError> {
        let declaration_span = source_span_from_pair(&import_pair);
        let mut inner_pairs = import_pair.into_inner();
        let source_pair = self.next_pair(
            &mut inner_pairs,
            "MCP tool batch import source",
            "MCP tool batch import declaration",
        )?;
        let block_pair = self.next_pair(&mut inner_pairs, "MCP tool batch import block", "MCP tool batch import declaration")?;
        let server_name = self.visit_mcp_tool_batch_import_source(source_pair)?;
        let import_block = self.visit_mcp_tool_batch_import_block(block_pair)?;
        let tools = import_block
            .import_items
            .iter()
            .map(|import_item| import_item.to_tool_declaration(&server_name, &import_block.fixed_binding_fields, import_block.max_calls))
            .collect::<Vec<_>>();

        Ok(Declaration::McpToolBatch(McpToolBatchImportDeclaration {
            server_name,
            fixed_binding_fields: import_block.fixed_binding_fields,
            max_calls: import_block.max_calls,
            items: import_block.import_items,
            tools,
            span: declaration_span,
        }))
    }

    fn visit_mcp_tool_batch_import_source(&self, source_pair: Pair<'_, Rule>) -> Result<String, DslParseError> {
        let mut inner_pairs = source_pair.into_inner();

        self.next_identifier(&mut inner_pairs, "MCP server name", "MCP tool batch import source")
    }

    fn visit_mcp_tool_batch_import_block(&self, block_pair: Pair<'_, Rule>) -> Result<McpToolBatchImportBlock, DslParseError> {
        let block_span = source_span_from_pair(&block_pair);
        let mut fixed_binding_fields = Vec::new();
        let mut import_items = Vec::new();
        let mut max_calls = None;

        for property_pair in block_pair.into_inner() {
            match property_pair.as_rule() {
                Rule::named_object_property => {
                    let mut inner_pairs = property_pair.into_inner();
                    let property_name = self.next_identifier(&mut inner_pairs, "MCP tool batch property name", "MCP tool batch import")?;
                    let Some(McpToolBatchImportPropertyName::Bindings) =
                        McpToolBatchImportPropertyName::from_identifier(property_name.as_str())
                    else {
                        return Err(DslParseError::unexpected_with_span(
                            Rule::named_object_property,
                            "MCP tool batch import property",
                            block_span,
                        ));
                    };
                    let object_expression_pair = self.next_pair(&mut inner_pairs, "MCP tool batch bindings", "MCP tool batch import")?;
                    fixed_binding_fields.extend(self.visit_object_expression(object_expression_pair)?);
                }
                Rule::named_unsigned_integer_property => {
                    let mut inner_pairs = property_pair.into_inner();
                    let property_name = self.next_identifier(&mut inner_pairs, "MCP tool batch property name", "MCP tool batch import")?;
                    let Some(McpToolBatchImportPropertyName::MaxCalls) =
                        McpToolBatchImportPropertyName::from_identifier(property_name.as_str())
                    else {
                        return Err(DslParseError::unexpected_with_span(
                            Rule::named_unsigned_integer_property,
                            "MCP tool batch import property",
                            block_span,
                        ));
                    };
                    let max_calls_pair = self.next_pair(&mut inner_pairs, "MCP tool batch max calls", "MCP tool batch import")?;
                    max_calls = Some(self.parse_unsigned_integer(max_calls_pair, "MCP tool batch import max calls")?);
                }
                Rule::mcp_tool_batch_import_item => {
                    import_items.push(self.visit_mcp_tool_batch_import_item(property_pair)?);
                }
                _ => unreachable!("MCP tool batch import block should contain only valid properties"),
            }
        }

        Ok(McpToolBatchImportBlock {
            fixed_binding_fields,
            max_calls,
            import_items,
        })
    }

    fn visit_mcp_tool_batch_import_item(&self, item_pair: Pair<'_, Rule>) -> Result<McpToolBatchImportItem, DslParseError> {
        let item_span = source_span_from_pair(&item_pair);
        let mut inner_pairs = item_pair.into_inner();
        let source_name_pair = self.next_pair(&mut inner_pairs, "MCP tool import name", "MCP tool batch import item")?;
        let source_name = source_name_pair.as_str().split_whitespace().collect::<String>();
        let mut local_name = None;
        let mut fixed_binding_fields = Vec::new();

        for item_property_pair in inner_pairs {
            match item_property_pair.as_rule() {
                Rule::identifier => {
                    local_name = Some(item_property_pair.as_str().to_string());
                }
                Rule::object_expression => {
                    fixed_binding_fields.extend(self.visit_object_expression(item_property_pair)?);
                }
                _ => unreachable!("MCP tool batch import item should contain only valid properties"),
            }
        }

        Ok(McpToolBatchImportItem::new(
            source_name,
            local_name,
            fixed_binding_fields,
            item_span,
        ))
    }

    fn visit_resource_import_declaration(&self, resource_pair: Pair<'_, Rule>) -> Result<Declaration, DslParseError> {
        let declaration_span = source_span_from_pair(&resource_pair);
        let (name, source, parameters) = self.visit_named_mcp_import(resource_pair, "resource import declaration")?;

        Ok(Declaration::McpResource(McpResourceImportDeclaration {
            name,
            source,
            parameters,
            span: declaration_span,
        }))
    }

    fn visit_prompt_import_declaration(&self, prompt_pair: Pair<'_, Rule>) -> Result<Declaration, DslParseError> {
        let declaration_span = source_span_from_pair(&prompt_pair);
        let (name, source, parameters) = self.visit_named_mcp_import(prompt_pair, "prompt import declaration")?;

        Ok(Declaration::McpPrompt(McpPromptImportDeclaration {
            name,
            source,
            parameters,
            span: declaration_span,
        }))
    }

    fn visit_named_mcp_import(
        &self,
        import_pair: Pair<'_, Rule>,
        context: &'static str,
    ) -> Result<(String, McpImportSource, Vec<ObjectField>), DslParseError> {
        let declaration_span = source_span_from_pair(&import_pair);
        let mut inner_pairs = import_pair.into_inner().peekable();
        let first_pair = inner_pairs
            .next()
            .ok_or_else(|| DslParseError::missing_with_span("MCP import source", context, declaration_span))?;
        let (alias, source_pair) = if first_pair.as_rule() == Rule::identifier {
            let source_pair = inner_pairs
                .next()
                .ok_or_else(|| DslParseError::missing_with_span("MCP import source", context, declaration_span))?;

            (Some(first_pair.as_str().to_owned()), source_pair)
        } else {
            (None, first_pair)
        };
        let source = self.visit_mcp_import_source(source_pair)?;
        let parameters = inner_pairs
            .next()
            .map(|block_pair| self.visit_mcp_import_block(block_pair))
            .transpose()?
            .unwrap_or_default();
        let name = alias.unwrap_or_else(|| source.inferred_local_name());

        Ok((name, source, parameters))
    }

    fn visit_mcp_import_source(&self, source_pair: Pair<'_, Rule>) -> Result<McpImportSource, DslParseError> {
        let source_span = source_span_from_pair(&source_pair);
        let mut inner_pairs = source_pair.into_inner();
        let server_name = self.next_identifier(&mut inner_pairs, "MCP server name", "MCP import reference")?;
        let kind_pair = self.next_pair(&mut inner_pairs, "MCP import kind", "MCP import reference")?;
        let kind = McpImportKind::from_identifier(kind_pair.as_str()).ok_or_else(|| {
            DslParseError::unexpected_with_span(kind_pair.as_rule(), "MCP import kind", source_span_from_pair(&kind_pair))
        })?;
        let item_name_pair = self.next_pair(&mut inner_pairs, "MCP import name", "MCP import reference")?;

        Ok(McpImportSource {
            server_name,
            kind,
            item_name: item_name_pair.as_str().split_whitespace().collect::<String>(),
            span: source_span,
        })
    }

    fn visit_mcp_import_block(&self, block_pair: Pair<'_, Rule>) -> Result<Vec<ObjectField>, DslParseError> {
        let block_span = source_span_from_pair(&block_pair);
        let mut parameters = Vec::new();

        for property_pair in block_pair.into_inner() {
            match property_pair.as_rule() {
                Rule::named_object_property => {
                    let mut inner_pairs = property_pair.into_inner();
                    let property_name = self.next_identifier(&mut inner_pairs, "MCP import property name", "MCP import block")?;

                    if McpImportPropertyName::from_identifier(property_name.as_str()).is_none() {
                        return Err(DslParseError::unexpected_with_span(
                            Rule::named_object_property,
                            "MCP import property",
                            block_span,
                        ));
                    }

                    let object_expression_pair = self.next_pair(&mut inner_pairs, "MCP import parameters", "MCP import block")?;
                    parameters.extend(self.visit_object_expression(object_expression_pair)?);
                }
                _ => unreachable!("MCP import block should contain only valid properties"),
            }
        }

        Ok(parameters)
    }

    fn visit_tool_import_block(&self, block_pair: Pair<'_, Rule>) -> Result<(Vec<ObjectField>, Option<u64>), DslParseError> {
        let block_span = source_span_from_pair(&block_pair);
        let mut fixed_bindings = Vec::new();
        let mut max_calls = None;

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
                    fixed_bindings.extend(self.visit_object_expression(object_expression_pair)?);
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
                    max_calls = Some(self.parse_unsigned_integer(max_calls_pair, "tool import max calls")?);
                }
                _ => unreachable!("tool import block should contain only valid properties"),
            }
        }

        Ok((fixed_bindings, max_calls))
    }

    fn visit_mcp_declaration(&self, mcp_pair: Pair<'_, Rule>) -> Result<Declaration, DslParseError> {
        let declaration_span = source_span_from_pair(&mcp_pair);
        let mut inner_pairs = mcp_pair.into_inner();

        let server_name = self.next_identifier(&mut inner_pairs, "MCP server name", "MCP declaration")?;
        let object_expression_pair = self.next_pair(&mut inner_pairs, "MCP body", "MCP declaration")?;
        let properties = self.visit_object_expression(object_expression_pair)?;

        Ok(Declaration::McpServer(McpServerDeclaration {
            name: server_name,
            properties,
            span: declaration_span,
        }))
    }

    fn visit_tool_bindings_block(&self, bindings_block_pair: Pair<'_, Rule>) -> Result<(Vec<TypedField>, Vec<ObjectField>), DslParseError> {
        let mut typed_fields = Vec::new();
        let mut fixed_fields = Vec::new();

        for binding_field_pair in bindings_block_pair.into_inner() {
            let binding_field_span = source_span_from_pair(&binding_field_pair);
            let mut inner_pairs = binding_field_pair.into_inner();
            let field_name = self.next_identifier(&mut inner_pairs, "binding field name", "tool bindings field")?;
            let field_value_pair = self.next_pair(&mut inner_pairs, "binding field value", "tool bindings field")?;

            match field_value_pair.as_rule() {
                Rule::tool_binding_type_expression => {
                    let field_type = self.visit_tool_binding_type_expression(field_value_pair)?;

                    if let TypeExpression::StringEnum(string_value) = field_type {
                        fixed_fields.push(ObjectField {
                            name: field_name,
                            value: Expression::StringLiteral(string_value),
                            span: binding_field_span,
                        });

                        continue;
                    }

                    if let TypeExpression::StringEnumReference(reference) = field_type {
                        fixed_fields.push(ObjectField {
                            name: field_name,
                            value: Expression::Reference(reference),
                            span: binding_field_span,
                        });

                        continue;
                    }

                    let description = inner_pairs
                        .next()
                        .map(|description_pair| self.parse_string_literal(description_pair))
                        .transpose()?;

                    typed_fields.push(TypedField {
                        name: field_name,
                        field_type,
                        description,
                        span: binding_field_span,
                    });
                }
                Rule::expression
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

        Ok((typed_fields, fixed_fields))
    }

    fn visit_tool_typed_fields_block(&self, typed_fields_block_pair: Pair<'_, Rule>) -> Result<Vec<TypedField>, DslParseError> {
        let mut typed_fields = Vec::new();

        for field_pair in typed_fields_block_pair.into_inner() {
            let field_span = source_span_from_pair(&field_pair);
            let mut inner_pairs = field_pair.into_inner();
            let field_name = self.next_identifier(&mut inner_pairs, "field name", "tool typed field")?;
            let field_value_pair = self.next_pair(&mut inner_pairs, "field type", "tool typed field")?;

            if field_value_pair.as_rule() != Rule::tool_binding_type_expression {
                return Err(DslParseError::unexpected_with_span(
                    field_value_pair.as_rule(),
                    "tool typed field type",
                    source_span_from_pair(&field_value_pair),
                ));
            }

            let description = inner_pairs
                .next()
                .map(|description_pair| self.parse_string_literal(description_pair))
                .transpose()?;

            typed_fields.push(TypedField {
                name: field_name,
                field_type: self.visit_tool_binding_type_expression(field_value_pair)?,
                description,
                span: field_span,
            });
        }

        Ok(typed_fields)
    }

    fn visit_tool_binding_type_expression(&self, type_expression_pair: Pair<'_, Rule>) -> Result<TypeExpression, DslParseError> {
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

    fn visit_tool_binding_type_term(&self, type_term_pair: Pair<'_, Rule>) -> Result<TypeExpression, DslParseError> {
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

    fn visit_dynamic_declaration(&self, dynamic_pair: Pair<'_, Rule>) -> Result<DynamicBlock, DslParseError> {
        let declaration_span = source_span_from_pair(&dynamic_pair);
        let object_expression_pair = self.first_inner_pair(dynamic_pair, "dynamic declaration")?;
        let fields = self.visit_object_expression(object_expression_pair)?;

        Ok(DynamicBlock {
            fields,
            span: declaration_span,
        })
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

        let pattern_pair = self.next_pair(&mut inner_pairs, "for-loop pattern", "for clause")?;
        let pattern = self.visit_for_loop_pattern(pattern_pair)?;
        let iterable_pair = self.next_pair(&mut inner_pairs, "iterable expression", "for clause")?;
        let iterable = self.visit_expression(iterable_pair)?;

        Ok(AgentForLoop { pattern, iterable })
    }

    fn visit_for_loop_pattern(&self, pattern_pair: Pair<'_, Rule>) -> Result<AgentForLoopPattern, DslParseError> {
        match pattern_pair.as_rule() {
            Rule::for_loop_pattern => {
                let inner_pattern_pair = self.first_inner_pair(pattern_pair, "for-loop pattern")?;

                self.visit_for_loop_pattern(inner_pattern_pair)
            }
            Rule::identifier => Ok(AgentForLoopPattern::Identifier(pattern_pair.as_str().to_owned())),
            Rule::object_destructuring_pattern => {
                let mut field_names = Vec::new();

                for identifier_pair in pattern_pair.into_inner() {
                    if identifier_pair.as_rule() != Rule::identifier {
                        return Err(DslParseError::unexpected_with_span(
                            identifier_pair.as_rule(),
                            "object destructuring pattern",
                            source_span_from_pair(&identifier_pair),
                        ));
                    }

                    field_names.push(identifier_pair.as_str().to_owned());
                }

                Ok(AgentForLoopPattern::ObjectDestructuring(field_names))
            }
            _ => Err(DslParseError::unexpected_with_span(
                pattern_pair.as_rule(),
                "for-loop pattern",
                source_span_from_pair(&pattern_pair),
            )),
        }
    }

    fn visit_agent_block(&self, agent_block_pair: Pair<'_, Rule>) -> Result<Vec<AgentProperty>, DslParseError> {
        let mut properties = Vec::new();

        for property_pair in agent_block_pair.into_inner() {
            properties.push(self.visit_agent_property(property_pair)?);
        }

        Ok(properties)
    }

    fn visit_agent_property(&self, property_pair: Pair<'_, Rule>) -> Result<AgentProperty, DslParseError> {
        let property_span = source_span_from_pair(&property_pair);

        match property_pair.as_rule() {
            Rule::named_object_property => self.visit_agent_object_property(property_pair, property_span),
            Rule::named_agent_value_property => self.visit_agent_value_property(property_pair, property_span),
            _ => unreachable!("agent block should contain only valid agent property rules"),
        }
    }

    fn visit_agent_object_property(
        &self,
        property_pair: Pair<'_, Rule>,
        property_span: SourceSpan,
    ) -> Result<AgentProperty, DslParseError> {
        let mut inner_pairs = property_pair.into_inner();
        let property_name = self.next_identifier(&mut inner_pairs, "agent property name", "agent object property")?;
        let object_expression_pair = self.next_pair(&mut inner_pairs, "agent object property value", "agent object property")?;

        match AgentPropertyName::from_identifier(property_name.as_str()) {
            Some(AgentPropertyName::Dynamic) => Ok(AgentProperty::Dynamic(DynamicBlock {
                fields: self.visit_object_expression(object_expression_pair)?,
                span: property_span,
            })),
            Some(AgentPropertyName::Output) => {
                let expression = self
                    .visit_object_expression(object_expression_pair)
                    .map(Expression::ObjectLiteral)?;
                let Some(output_type_expression) = expression.to_type_expression() else {
                    return Err(DslParseError::unexpected_with_span(
                        Rule::named_object_property,
                        "agent output property",
                        property_span,
                    ));
                };

                Ok(AgentProperty::Output {
                    output_type_expression,
                    description: None,
                })
            }
            Some(_) => Err(DslParseError::unexpected_with_span(
                Rule::named_object_property,
                "agent object property",
                property_span,
            )),
            None => Ok(AgentProperty::Unknown {
                name: property_name,
                span: property_span,
            }),
        }
    }

    fn visit_agent_value_property(&self, property_pair: Pair<'_, Rule>, property_span: SourceSpan) -> Result<AgentProperty, DslParseError> {
        let mut inner_pairs = property_pair.into_inner();
        let property_name = self.next_identifier(&mut inner_pairs, "agent property name", "agent value property")?;
        let Some(agent_property_name) = AgentPropertyName::from_identifier(property_name.as_str()) else {
            return Ok(AgentProperty::Unknown {
                name: property_name,
                span: property_span,
            });
        };
        let value_pair = self.next_pair(&mut inner_pairs, "agent property value", "agent value property")?;

        match agent_property_name {
            AgentPropertyName::Model => Ok(AgentProperty::Model(self.visit_expression(value_pair)?)),
            AgentPropertyName::ResponseFormat => self.visit_agent_response_format_property(value_pair),
            AgentPropertyName::Prompt => Ok(AgentProperty::Prompt(self.visit_expression(value_pair)?)),
            AgentPropertyName::Output => self.visit_agent_output_property(value_pair, inner_pairs, property_span),
            AgentPropertyName::Context => Ok(AgentProperty::Context(self.visit_expression(value_pair)?)),
            AgentPropertyName::Inference => Ok(AgentProperty::Inference(self.visit_expression(value_pair)?)),
            AgentPropertyName::Tools => Ok(AgentProperty::Tools(self.visit_tools_expression(value_pair)?)),
            AgentPropertyName::Dynamic | AgentPropertyName::Unknown => Err(DslParseError::unexpected_with_span(
                Rule::named_agent_value_property,
                "agent value property",
                property_span,
            )),
        }
    }

    fn visit_agent_response_format_property(&self, value_pair: Pair<'_, Rule>) -> Result<AgentProperty, DslParseError> {
        let response_format = AgentResponseFormat::from_identifier(value_pair.as_str()).ok_or_else(|| {
            DslParseError::unexpected_with_span(value_pair.as_rule(), "response format property", source_span_from_pair(&value_pair))
        })?;

        Ok(AgentProperty::ResponseFormat(response_format))
    }

    fn visit_agent_output_property(
        &self,
        value_pair: Pair<'_, Rule>,
        mut remaining_pairs: Pairs<'_, Rule>,
        property_span: SourceSpan,
    ) -> Result<AgentProperty, DslParseError> {
        let output_type_expression = self.visit_agent_output_property_type(value_pair, property_span)?;
        let description = remaining_pairs
            .next()
            .map(|description_pair| self.parse_string_literal(description_pair))
            .transpose()?;

        Ok(AgentProperty::Output {
            output_type_expression,
            description,
        })
    }

    fn visit_agent_output_property_type(
        &self,
        value_pair: Pair<'_, Rule>,
        property_span: SourceSpan,
    ) -> Result<TypeExpression, DslParseError> {
        if value_pair.as_rule() == Rule::type_expression {
            return self.visit_type_expression(value_pair);
        }

        let output_expression = if value_pair.as_rule() == Rule::tools_expression {
            self.visit_tools_expression(value_pair)?
        } else {
            self.visit_expression(value_pair)?
        };

        let Some(output_type_expression) = output_expression.to_type_expression() else {
            return Err(DslParseError::unexpected_with_span(
                Rule::named_agent_value_property,
                "agent output property",
                property_span,
            ));
        };

        Ok(output_type_expression)
    }

    fn visit_tools_expression(&self, tools_expression_pair: Pair<'_, Rule>) -> Result<Expression, DslParseError> {
        let mut tool_bindings = Vec::new();

        for agent_tool_binding_pair in tools_expression_pair.into_inner() {
            tool_bindings.push(self.visit_agent_tool_binding(agent_tool_binding_pair)?);
        }

        Ok(Expression::ArrayLiteral(tool_bindings))
    }

    fn visit_agent_tool_binding(&self, agent_tool_binding_pair: Pair<'_, Rule>) -> Result<Expression, DslParseError> {
        let agent_tool_binding_span = source_span_from_pair(&agent_tool_binding_pair);
        let mut inner_pairs = agent_tool_binding_pair.into_inner();
        let callee_pair = self.next_pair(&mut inner_pairs, "agent tool binding callee", "agent tool binding")?;
        let callee = self.visit_reference(callee_pair)?;

        let Some(block_pair) = inner_pairs.next() else {
            return Ok(Expression::Reference(callee));
        };

        let block_span = source_span_from_pair(&block_pair);
        let mut binding_fields = Vec::new();
        let mut max_calls = None;

        for property_pair in block_pair.into_inner() {
            match property_pair.as_rule() {
                Rule::named_object_property => {
                    let mut inner_pairs = property_pair.into_inner();
                    let property_name = self.next_identifier(&mut inner_pairs, "agent tool binding property name", "agent tool binding")?;
                    let Some(ToolCallPropertyName::Bindings) = ToolCallPropertyName::from_identifier(property_name.as_str()) else {
                        return Err(DslParseError::unexpected_with_span(
                            Rule::named_object_property,
                            "agent tool binding property",
                            block_span,
                        ));
                    };
                    let object_expression_pair = self.next_pair(&mut inner_pairs, "agent tool binding bindings", "agent tool binding")?;
                    binding_fields.extend(self.visit_object_expression(object_expression_pair)?);
                }
                Rule::named_unsigned_integer_property => {
                    let mut inner_pairs = property_pair.into_inner();
                    let property_name = self.next_identifier(&mut inner_pairs, "agent tool binding property name", "agent tool binding")?;
                    let Some(ToolCallPropertyName::MaxCalls) = ToolCallPropertyName::from_identifier(property_name.as_str()) else {
                        return Err(DslParseError::unexpected_with_span(
                            Rule::named_unsigned_integer_property,
                            "agent tool binding property",
                            block_span,
                        ));
                    };
                    let max_calls_pair = self.next_pair(&mut inner_pairs, "agent tool binding max calls", "agent tool binding")?;
                    max_calls = Some(self.parse_unsigned_integer(max_calls_pair, "agent tool binding max calls property")?);
                }
                _ => {
                    return Err(DslParseError::unexpected_with_span(
                        property_pair.as_rule(),
                        "agent tool binding property",
                        source_span_from_pair(&property_pair),
                    ));
                }
            }
        }

        Ok(Expression::ToolCall(ToolCall {
            callee,
            input_fields: Vec::new(),
            binding_fields,
            max_calls,
            span: agent_tool_binding_span,
        }))
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

    fn visit_expression(&self, expression_pair: Pair<'_, Rule>) -> Result<Expression, DslParseError> {
        match expression_pair.as_rule() {
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

    fn visit_object_expression(&self, object_expression_pair: Pair<'_, Rule>) -> Result<Vec<ObjectField>, DslParseError> {
        let mut object_fields = Vec::new();

        for object_field_pair in object_expression_pair.into_inner() {
            object_fields.push(self.visit_object_field(object_field_pair)?);
        }

        Ok(object_fields)
    }

    fn visit_object_field(&self, object_field_pair: Pair<'_, Rule>) -> Result<ObjectField, DslParseError> {
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

    fn visit_object_field_name(&self, object_field_name_pair: Pair<'_, Rule>) -> Result<String, DslParseError> {
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

    fn visit_tool_call_expression(&self, tool_call_pair: Pair<'_, Rule>) -> Result<ToolCall, DslParseError> {
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

    fn visit_mcp_call_expression(&self, mcp_call_pair: Pair<'_, Rule>) -> Result<McpCall, DslParseError> {
        let mcp_call_span = source_span_from_pair(&mcp_call_pair);
        let mut inner_pairs = mcp_call_pair.into_inner();
        let operation_pair = self.next_pair(&mut inner_pairs, "MCP call operation", "MCP call expression")?;
        let operation = McpCallOperation::from_identifier(operation_pair.as_str()).ok_or_else(|| {
            DslParseError::unexpected_with_span(
                operation_pair.as_rule(),
                "MCP call operation",
                source_span_from_pair(&operation_pair),
            )
        })?;
        let callee_pair = self.next_pair(&mut inner_pairs, "MCP call callee", "MCP call expression")?;
        let callee = self.visit_reference(callee_pair)?;
        let mut parameter_fields = Vec::new();

        if let Some(block_pair) = inner_pairs.next() {
            let block_span = source_span_from_pair(&block_pair);

            for property_pair in block_pair.into_inner() {
                match property_pair.as_rule() {
                    Rule::named_object_property => {
                        let mut inner_pairs = property_pair.into_inner();
                        let property_name = self.next_identifier(&mut inner_pairs, "MCP call property name", "MCP call block")?;

                        if McpImportPropertyName::from_identifier(property_name.as_str()).is_none() {
                            return Err(DslParseError::unexpected_with_span(
                                Rule::named_object_property,
                                "MCP call property",
                                block_span,
                            ));
                        }

                        let object_expression_pair = self.next_pair(&mut inner_pairs, "MCP call parameters", "MCP call block")?;
                        parameter_fields.extend(self.visit_object_expression(object_expression_pair)?);
                    }
                    _ => unreachable!("MCP call block should contain only valid MCP call property rules"),
                }
            }
        }

        Ok(McpCall {
            operation,
            callee,
            parameter_fields,
            span: mcp_call_span,
        })
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
            | Rule::mcp_call_expression
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
