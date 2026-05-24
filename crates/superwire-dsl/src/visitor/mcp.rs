use super::tool::ToolImportBlock;
use super::{source_span_from_pair, AstVisitor};
use crate::ast::{
    Declaration, McpBatchImportDeclaration, McpCall, McpCallOperation, McpImportKind, McpImportPropertyName, McpImportSource,
    McpPromptBatchImportDeclaration, McpPromptBatchImportItem, McpPromptImportDeclaration, McpResourceBatchImportDeclaration,
    McpResourceBatchImportItem, McpResourceImportDeclaration, McpServerDeclaration, McpServerPropertyName, McpToolBatchImportDeclaration,
    McpToolBatchImportItem, McpToolBatchImportPropertyName, ObjectField, ToolPropertyName, TypedField,
};
use crate::parser::{DslParseError, Rule};
use pest::iterators::Pair;

struct McpToolBatchImportBlock {
    fixed_binding_fields: Vec<ObjectField>,
    input_fields: Vec<TypedField>,
    max_calls: Option<u64>,
    output_fields: Vec<TypedField>,
    import_items: Vec<McpToolBatchImportItem>,
}

struct McpBatchImportBlock {
    fixed_binding_fields: Vec<ObjectField>,
    input_fields: Vec<TypedField>,
    max_calls: Option<u64>,
    output_fields: Vec<TypedField>,
    tool_items: Vec<McpToolBatchImportItem>,
    resource_items: Vec<McpResourceBatchImportItem>,
    prompt_items: Vec<McpPromptBatchImportItem>,
}

impl AstVisitor {
    pub(super) fn visit_mcp_tool_batch_import_declaration(&self, import_pair: Pair<'_, Rule>) -> Result<Declaration, DslParseError> {
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
            .map(|import_item| {
                import_item.to_tool_declaration(
                    &server_name,
                    &import_block.input_fields,
                    &import_block.fixed_binding_fields,
                    import_block.max_calls,
                    &import_block.output_fields,
                )
            })
            .collect::<Vec<_>>();

        Ok(Declaration::McpToolBatch(McpToolBatchImportDeclaration {
            server_name,
            fixed_binding_fields: import_block.fixed_binding_fields,
            input_fields: import_block.input_fields,
            max_calls: import_block.max_calls,
            output_fields: import_block.output_fields,
            items: import_block.import_items,
            tools,
            span: declaration_span,
        }))
    }

    pub(super) fn visit_mcp_tool_batch_import_source(&self, source_pair: Pair<'_, Rule>) -> Result<String, DslParseError> {
        let mut inner_pairs = source_pair.into_inner();

        self.next_identifier(&mut inner_pairs, "MCP server name", "MCP tool batch import source")
    }

    fn visit_mcp_tool_batch_import_block(&self, block_pair: Pair<'_, Rule>) -> Result<McpToolBatchImportBlock, DslParseError> {
        let block_span = source_span_from_pair(&block_pair);
        let mut fixed_binding_fields = Vec::new();
        let mut input_fields = Vec::new();
        let mut import_items = Vec::new();
        let mut max_calls = None;
        let mut output_fields = Vec::new();

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
                Rule::named_tool_block_property => {
                    let mut inner_pairs = property_pair.into_inner();
                    let property_name = self.next_identifier(&mut inner_pairs, "MCP tool batch property name", "MCP tool batch import")?;
                    let block_pair = self.next_pair(&mut inner_pairs, "MCP tool batch block property value", "MCP tool batch import")?;

                    match ToolPropertyName::from_identifier(property_name.as_str()) {
                        Some(ToolPropertyName::Input) => input_fields.extend(self.visit_tool_typed_fields_block(block_pair)?),
                        Some(ToolPropertyName::Bindings) => {
                            let (_, fixed_fields) = self.visit_tool_bindings_block(block_pair)?;
                            fixed_binding_fields.extend(fixed_fields);
                        }
                        Some(ToolPropertyName::Output) => output_fields.extend(self.visit_tool_typed_fields_block(block_pair)?),
                        _ => {
                            return Err(DslParseError::unexpected_with_span(
                                Rule::named_tool_block_property,
                                "MCP tool batch import property",
                                block_span,
                            ));
                        }
                    }
                }
                Rule::mcp_tool_batch_import_item => {
                    import_items.push(self.visit_mcp_tool_batch_import_item(property_pair)?);
                }
                _ => unreachable!("MCP tool batch import block should contain only valid properties"),
            }
        }

        Ok(McpToolBatchImportBlock {
            fixed_binding_fields,
            input_fields,
            max_calls,
            output_fields,
            import_items,
        })
    }

    pub(super) fn visit_mcp_tool_batch_import_item(&self, item_pair: Pair<'_, Rule>) -> Result<McpToolBatchImportItem, DslParseError> {
        let item_span = source_span_from_pair(&item_pair);
        let mut inner_pairs = item_pair.into_inner();
        let source_name_pair = self.next_pair(&mut inner_pairs, "MCP tool import name", "MCP tool batch import item")?;
        let source_name = self.parse_wire_tool_name(source_name_pair, "MCP tool batch import item")?;
        let mut local_name = None;
        let mut import_block = ToolImportBlock::default();

        for item_property_pair in inner_pairs {
            match item_property_pair.as_rule() {
                Rule::identifier => {
                    local_name = Some(item_property_pair.as_str().to_string());
                }
                Rule::tool_import_block => {
                    import_block = self.visit_tool_import_block(item_property_pair)?;
                }
                _ => unreachable!("MCP tool batch import item should contain only valid properties"),
            }
        }

        Ok(McpToolBatchImportItem::new(
            source_name,
            local_name,
            import_block.input_fields,
            import_block.max_calls,
            import_block.fixed_binding_fields,
            import_block.output_fields,
            item_span,
        ))
    }

    pub(super) fn visit_mcp_resource_batch_import_declaration(&self, import_pair: Pair<'_, Rule>) -> Result<Declaration, DslParseError> {
        let declaration_span = source_span_from_pair(&import_pair);
        let mut inner_pairs = import_pair.into_inner();
        let source_pair = self.next_pair(
            &mut inner_pairs,
            "MCP resource batch import source",
            "MCP resource batch import declaration",
        )?;
        let block_pair = self.next_pair(
            &mut inner_pairs,
            "MCP resource batch import block",
            "MCP resource batch import declaration",
        )?;
        let server_name = self.visit_mcp_tool_batch_import_source(source_pair)?;
        let (parameters, items) = self.visit_mcp_resource_batch_import_block(block_pair)?;
        let resources = items
            .iter()
            .map(|item| item.to_resource_import_declaration(&server_name, &parameters))
            .collect::<Vec<_>>();

        Ok(Declaration::McpResourceBatch(McpResourceBatchImportDeclaration {
            server_name,
            parameters,
            items,
            resources,
            span: declaration_span,
        }))
    }

    pub(super) fn visit_mcp_prompt_batch_import_declaration(&self, import_pair: Pair<'_, Rule>) -> Result<Declaration, DslParseError> {
        let declaration_span = source_span_from_pair(&import_pair);
        let mut inner_pairs = import_pair.into_inner();
        let source_pair = self.next_pair(
            &mut inner_pairs,
            "MCP prompt batch import source",
            "MCP prompt batch import declaration",
        )?;
        let block_pair = self.next_pair(
            &mut inner_pairs,
            "MCP prompt batch import block",
            "MCP prompt batch import declaration",
        )?;
        let server_name = self.visit_mcp_tool_batch_import_source(source_pair)?;
        let (parameters, items) = self.visit_mcp_prompt_batch_import_block(block_pair)?;
        let prompts = items
            .iter()
            .map(|item| item.to_prompt_import_declaration(&server_name, &parameters))
            .collect::<Vec<_>>();

        Ok(Declaration::McpPromptBatch(McpPromptBatchImportDeclaration {
            server_name,
            parameters,
            items,
            prompts,
            span: declaration_span,
        }))
    }

    pub(super) fn visit_mcp_batch_import_declaration(&self, import_pair: Pair<'_, Rule>) -> Result<Declaration, DslParseError> {
        let declaration_span = source_span_from_pair(&import_pair);
        let mut inner_pairs = import_pair.into_inner();
        let source_pair = self.next_pair(&mut inner_pairs, "MCP batch import source", "MCP batch import declaration")?;
        let block_pair = self.next_pair(&mut inner_pairs, "MCP batch import block", "MCP batch import declaration")?;
        let server_name = self.visit_mcp_tool_batch_import_source(source_pair)?;
        let import_block = self.visit_mcp_batch_import_block(block_pair)?;
        let tools = import_block
            .tool_items
            .iter()
            .map(|import_item| {
                import_item.to_tool_declaration(
                    &server_name,
                    &import_block.input_fields,
                    &import_block.fixed_binding_fields,
                    import_block.max_calls,
                    &import_block.output_fields,
                )
            })
            .collect::<Vec<_>>();
        let resources = import_block
            .resource_items
            .iter()
            .map(|item| item.to_resource_import_declaration(&server_name, &import_block.fixed_binding_fields))
            .collect::<Vec<_>>();
        let prompts = import_block
            .prompt_items
            .iter()
            .map(|item| item.to_prompt_import_declaration(&server_name, &import_block.fixed_binding_fields))
            .collect::<Vec<_>>();

        Ok(Declaration::McpBatch(McpBatchImportDeclaration {
            server_name,
            fixed_binding_fields: import_block.fixed_binding_fields,
            input_fields: import_block.input_fields,
            max_calls: import_block.max_calls,
            output_fields: import_block.output_fields,
            tool_items: import_block.tool_items,
            resource_items: import_block.resource_items,
            prompt_items: import_block.prompt_items,
            tools,
            resources,
            prompts,
            span: declaration_span,
        }))
    }

    fn visit_mcp_batch_import_block(&self, block_pair: Pair<'_, Rule>) -> Result<McpBatchImportBlock, DslParseError> {
        let block_span = source_span_from_pair(&block_pair);
        let mut fixed_binding_fields = Vec::new();
        let mut input_fields = Vec::new();
        let mut max_calls = None;
        let mut output_fields = Vec::new();
        let mut tool_items = Vec::new();
        let mut resource_items = Vec::new();
        let mut prompt_items = Vec::new();

        for property_pair in block_pair.into_inner() {
            match property_pair.as_rule() {
                Rule::named_object_property => {
                    let mut inner_pairs = property_pair.into_inner();
                    let property_name = self.next_identifier(&mut inner_pairs, "MCP batch property name", "MCP batch import")?;
                    let Some(McpImportPropertyName::Bindings) = McpImportPropertyName::from_identifier(property_name.as_str()) else {
                        return Err(DslParseError::unexpected_with_span(
                            Rule::named_object_property,
                            "MCP batch import property",
                            block_span,
                        ));
                    };
                    let object_expression_pair = self.next_pair(&mut inner_pairs, "MCP batch bindings", "MCP batch import")?;
                    fixed_binding_fields.extend(self.visit_object_expression(object_expression_pair)?);
                }
                Rule::named_unsigned_integer_property => {
                    let mut inner_pairs = property_pair.into_inner();
                    let property_name = self.next_identifier(&mut inner_pairs, "MCP batch property name", "MCP batch import")?;
                    let Some(McpToolBatchImportPropertyName::MaxCalls) =
                        McpToolBatchImportPropertyName::from_identifier(property_name.as_str())
                    else {
                        return Err(DslParseError::unexpected_with_span(
                            Rule::named_unsigned_integer_property,
                            "MCP batch import property",
                            block_span,
                        ));
                    };
                    let max_calls_pair = self.next_pair(&mut inner_pairs, "MCP batch max calls", "MCP batch import")?;
                    max_calls = Some(self.parse_unsigned_integer(max_calls_pair, "MCP batch import max calls")?);
                }
                Rule::named_tool_block_property => {
                    let mut inner_pairs = property_pair.into_inner();
                    let property_name = self.next_identifier(&mut inner_pairs, "MCP batch property name", "MCP batch import")?;
                    let block_pair = self.next_pair(&mut inner_pairs, "MCP batch block property value", "MCP batch import")?;

                    match ToolPropertyName::from_identifier(property_name.as_str()) {
                        Some(ToolPropertyName::Input) => input_fields.extend(self.visit_tool_typed_fields_block(block_pair)?),
                        Some(ToolPropertyName::Bindings) => {
                            let (_, fixed_fields) = self.visit_tool_bindings_block(block_pair)?;
                            fixed_binding_fields.extend(fixed_fields);
                        }
                        Some(ToolPropertyName::Output) => output_fields.extend(self.visit_tool_typed_fields_block(block_pair)?),
                        _ => {
                            return Err(DslParseError::unexpected_with_span(
                                Rule::named_tool_block_property,
                                "MCP batch import property",
                                block_span,
                            ));
                        }
                    }
                }
                Rule::mcp_tool_batch_import_item => tool_items.push(self.visit_mcp_tool_batch_import_item(property_pair)?),
                Rule::mcp_resource_batch_import_item => {
                    resource_items.push(self.visit_mcp_resource_batch_import_item(property_pair)?);
                }
                Rule::mcp_prompt_batch_import_item => prompt_items.push(self.visit_mcp_prompt_batch_import_item(property_pair)?),
                _ => unreachable!("MCP batch import block should contain only valid properties"),
            }
        }

        Ok(McpBatchImportBlock {
            fixed_binding_fields,
            input_fields,
            max_calls,
            output_fields,
            tool_items,
            resource_items,
            prompt_items,
        })
    }

    pub(super) fn visit_mcp_resource_batch_import_block(
        &self,
        block_pair: Pair<'_, Rule>,
    ) -> Result<(Vec<ObjectField>, Vec<McpResourceBatchImportItem>), DslParseError> {
        let block_span = source_span_from_pair(&block_pair);
        let context = "MCP resource batch import";
        let mut parameters = Vec::new();
        let mut items = Vec::new();

        for property_pair in block_pair.into_inner() {
            match property_pair.as_rule() {
                Rule::named_object_property => {
                    let mut inner_pairs = property_pair.into_inner();
                    let property_name = self.next_identifier(&mut inner_pairs, "MCP import property name", context)?;

                    if McpImportPropertyName::from_identifier(property_name.as_str()).is_none() {
                        return Err(DslParseError::unexpected_with_span(
                            Rule::named_object_property,
                            "MCP import property",
                            block_span,
                        ));
                    }

                    let object_expression_pair = self.next_pair(&mut inner_pairs, "MCP import parameters", context)?;
                    parameters.extend(self.visit_object_expression(object_expression_pair)?);
                }
                Rule::mcp_resource_batch_import_item => items.push(self.visit_mcp_resource_batch_import_item(property_pair)?),
                _ => unreachable!("MCP named batch import block should contain only valid properties"),
            }
        }

        Ok((parameters, items))
    }

    pub(super) fn visit_mcp_resource_batch_import_item(
        &self,
        item_pair: Pair<'_, Rule>,
    ) -> Result<McpResourceBatchImportItem, DslParseError> {
        let context = "MCP resource batch import";
        let item_span = source_span_from_pair(&item_pair);
        let mut inner_pairs = item_pair.into_inner();
        let source_name_pair = self.next_pair(&mut inner_pairs, "MCP import name", context)?;
        let source_name = source_name_pair.as_str().to_string();

        if !McpImportKind::Resource.wire_item_name_is_snake_case(&source_name) {
            return Err(DslParseError::Pest {
                message: "MCP resource names in .wire files must be snake_case".to_string(),
                expected_rules: Vec::new(),
                span: source_span_from_pair(&source_name_pair),
            });
        }

        let mut local_name = None;
        let mut parameters = Vec::new();

        for item_property_pair in inner_pairs {
            match item_property_pair.as_rule() {
                Rule::identifier => {
                    local_name = Some(item_property_pair.as_str().to_string());
                }
                Rule::mcp_import_block => {
                    parameters = self.visit_mcp_import_block(item_property_pair)?;
                }
                _ => unreachable!("MCP named batch import item should contain only valid properties"),
            }
        }

        Ok(McpResourceBatchImportItem::new(source_name, local_name, parameters, item_span))
    }

    pub(super) fn visit_mcp_prompt_batch_import_block(
        &self,
        block_pair: Pair<'_, Rule>,
    ) -> Result<(Vec<ObjectField>, Vec<McpPromptBatchImportItem>), DslParseError> {
        let block_span = source_span_from_pair(&block_pair);
        let context = "MCP prompt batch import";
        let mut parameters = Vec::new();
        let mut items = Vec::new();

        for property_pair in block_pair.into_inner() {
            match property_pair.as_rule() {
                Rule::named_object_property => {
                    let mut inner_pairs = property_pair.into_inner();
                    let property_name = self.next_identifier(&mut inner_pairs, "MCP import property name", context)?;

                    if McpImportPropertyName::from_identifier(property_name.as_str()).is_none() {
                        return Err(DslParseError::unexpected_with_span(
                            Rule::named_object_property,
                            "MCP import property",
                            block_span,
                        ));
                    }

                    let object_expression_pair = self.next_pair(&mut inner_pairs, "MCP import parameters", context)?;
                    parameters.extend(self.visit_object_expression(object_expression_pair)?);
                }
                Rule::mcp_prompt_batch_import_item => items.push(self.visit_mcp_prompt_batch_import_item(property_pair)?),
                _ => unreachable!("MCP named batch import block should contain only valid properties"),
            }
        }

        Ok((parameters, items))
    }

    pub(super) fn visit_mcp_prompt_batch_import_item(&self, item_pair: Pair<'_, Rule>) -> Result<McpPromptBatchImportItem, DslParseError> {
        let context = "MCP prompt batch import";
        let item_span = source_span_from_pair(&item_pair);
        let mut inner_pairs = item_pair.into_inner();
        let source_name_pair = self.next_pair(&mut inner_pairs, "MCP import name", context)?;
        let source_name = source_name_pair.as_str().to_string();

        if !McpImportKind::Prompt.wire_item_name_is_snake_case(&source_name) {
            return Err(DslParseError::Pest {
                message: "MCP prompt names in .wire files must be snake_case".to_string(),
                expected_rules: Vec::new(),
                span: source_span_from_pair(&source_name_pair),
            });
        }

        let mut local_name = None;
        let mut parameters = Vec::new();

        for item_property_pair in inner_pairs {
            match item_property_pair.as_rule() {
                Rule::identifier => {
                    local_name = Some(item_property_pair.as_str().to_string());
                }
                Rule::mcp_import_block => {
                    parameters = self.visit_mcp_import_block(item_property_pair)?;
                }
                _ => unreachable!("MCP named batch import item should contain only valid properties"),
            }
        }

        Ok(McpPromptBatchImportItem::new(source_name, local_name, parameters, item_span))
    }

    pub(super) fn visit_resource_import_declaration(&self, resource_pair: Pair<'_, Rule>) -> Result<Declaration, DslParseError> {
        let declaration_span = source_span_from_pair(&resource_pair);
        let (name, source, parameters) = self.visit_named_mcp_import(resource_pair, "resource import declaration")?;

        Ok(Declaration::McpResource(McpResourceImportDeclaration {
            name,
            source,
            parameters,
            span: declaration_span,
        }))
    }

    pub(super) fn visit_prompt_import_declaration(&self, prompt_pair: Pair<'_, Rule>) -> Result<Declaration, DslParseError> {
        let declaration_span = source_span_from_pair(&prompt_pair);
        let (name, source, parameters) = self.visit_named_mcp_import(prompt_pair, "prompt import declaration")?;

        Ok(Declaration::McpPrompt(McpPromptImportDeclaration {
            name,
            source,
            parameters,
            span: declaration_span,
        }))
    }

    pub(super) fn visit_named_mcp_import(
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

    pub(super) fn visit_mcp_import_source(&self, source_pair: Pair<'_, Rule>) -> Result<McpImportSource, DslParseError> {
        let source_span = source_span_from_pair(&source_pair);
        let mut inner_pairs = source_pair.into_inner();
        let server_name = self.next_identifier(&mut inner_pairs, "MCP server name", "MCP import reference")?;
        let kind_pair = self.next_pair(&mut inner_pairs, "MCP import kind", "MCP import reference")?;
        let kind = McpImportKind::from_identifier(kind_pair.as_str()).ok_or_else(|| {
            DslParseError::unexpected_with_span(kind_pair.as_rule(), "MCP import kind", source_span_from_pair(&kind_pair))
        })?;
        let item_name_pair = self.next_pair(&mut inner_pairs, "MCP import name", "MCP import reference")?;
        let item_name = item_name_pair.as_str().split_whitespace().collect::<String>();

        if !kind.wire_item_name_is_snake_case(&item_name) {
            return Err(DslParseError::Pest {
                message: format!("MCP {} names in .wire files must be snake_case", kind.as_str()),
                expected_rules: Vec::new(),
                span: source_span_from_pair(&item_name_pair),
            });
        }

        Ok(McpImportSource {
            server_name,
            kind,
            item_name: kind.normalize_tool_name_from_wire(&item_name),
            span: source_span,
        })
    }

    pub(super) fn parse_wire_tool_name(&self, tool_name_pair: Pair<'_, Rule>, context: &'static str) -> Result<String, DslParseError> {
        let wire_tool_name = tool_name_pair.as_str().split_whitespace().collect::<String>();

        if !McpImportKind::wire_tool_name_is_snake_case(&wire_tool_name) {
            return Err(DslParseError::Pest {
                message: "MCP tool names in .wire files must be snake_case".to_string(),
                expected_rules: Vec::new(),
                span: source_span_from_pair(&tool_name_pair),
            });
        }

        if wire_tool_name.is_empty() {
            return Err(DslParseError::missing_with_span(
                "MCP tool import name",
                context,
                source_span_from_pair(&tool_name_pair),
            ));
        }

        Ok(wire_tool_name)
    }

    pub(super) fn visit_mcp_import_block(&self, block_pair: Pair<'_, Rule>) -> Result<Vec<ObjectField>, DslParseError> {
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

    pub(super) fn visit_mcp_declaration(&self, mcp_pair: Pair<'_, Rule>) -> Result<Declaration, DslParseError> {
        let declaration_span = source_span_from_pair(&mcp_pair);
        let mut inner_pairs = mcp_pair.into_inner();

        let server_name = self.next_identifier(&mut inner_pairs, "MCP server name", "MCP declaration")?;
        let server_block_pair = self.next_pair(&mut inner_pairs, "MCP body", "MCP declaration")?;
        let properties = self.visit_mcp_server_block(server_block_pair)?;

        Ok(Declaration::McpServer(McpServerDeclaration {
            name: server_name,
            properties,
            span: declaration_span,
        }))
    }

    pub(super) fn visit_mcp_server_block(&self, server_block_pair: Pair<'_, Rule>) -> Result<Vec<ObjectField>, DslParseError> {
        let mut properties = Vec::new();

        for property_pair in server_block_pair.into_inner() {
            let property = match property_pair.as_rule() {
                Rule::named_object_property => self.visit_named_object_property_as_field(property_pair)?,
                Rule::object_field => {
                    let property = self.visit_object_field(property_pair)?;

                    if McpServerPropertyName::from_identifier(&property.name) == Some(McpServerPropertyName::Headers) {
                        return Err(DslParseError::unexpected_with_span(
                            Rule::object_field,
                            "MCP headers block property",
                            property.span,
                        ));
                    }

                    property
                }
                _ => {
                    return Err(DslParseError::unexpected_with_span(
                        property_pair.as_rule(),
                        "MCP server property",
                        source_span_from_pair(&property_pair),
                    ));
                }
            };

            properties.push(property);
        }

        Ok(properties)
    }

    pub(super) fn visit_mcp_call_expression(&self, mcp_call_pair: Pair<'_, Rule>) -> Result<McpCall, DslParseError> {
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
}
