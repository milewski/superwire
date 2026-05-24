use std::collections::{BTreeMap, HashMap};

use superwire_core::dsl::{
    AgentForLoopPattern, AgentProperty, Declaration, DeclarationKeyword, Expression, ModelDeclaration, ModelDeclarationPropertyName,
    ModelUsagePropertyName, ObjectField, ProviderDeclaration, ReferenceKeyword, SingletonDeclarationKind, SourceSpan, ToolSource,
    TypeExpression, TypedField, Workflow,
};
use superwire_core::mcp::McpLock;
use superwire_core::semantic::{ProviderDriver, SemanticToolingSnapshot, ToolingSymbolCategory, WorkflowSemanticIndex};
use superwire_core::WorkflowDocument;

use super::types::{AgentSummary, FieldMetadata, ModelSummary, NamedSpan, ProviderSummary, SchemaSummary, SemanticIndex, ToolSummary};

impl SemanticIndex {
    pub fn from_workflow_document(workflow_document: &WorkflowDocument) -> Self {
        let Some(workflow) = workflow_document.workflow() else {
            return Self::from_text_fallback(workflow_document.source_text());
        };

        let workflow_semantics = workflow_document.semantic_index().cloned();

        Self::from_workflow_parts(workflow, workflow_document.mcp_lock().cloned(), workflow_semantics)
    }

    fn from_workflow_parts(workflow: &Workflow, mcp_lock: Option<McpLock>, workflow_semantics: Option<WorkflowSemanticIndex>) -> Self {
        let tooling_snapshot = SemanticToolingSnapshot::from_workflow(workflow);
        let mcp_server_tool_lookups = Self::mcp_server_tool_lookups(mcp_lock.as_ref());
        let mut semantic_index = Self {
            providers: HashMap::new(),
            provider_locations: Vec::new(),
            provider_location_index: HashMap::new(),
            models: HashMap::new(),
            model_locations: Vec::new(),
            model_location_index: HashMap::new(),
            schemas: HashMap::new(),
            schema_names: Vec::new(),
            schema_locations: Vec::new(),
            schema_location_index: HashMap::new(),
            schema_field_locations: HashMap::new(),
            tools: HashMap::new(),
            tool_names: Vec::new(),
            tool_locations: Vec::new(),
            tool_location_index: HashMap::new(),
            resource_names: Vec::new(),
            resource_locations: Vec::new(),
            resource_location_index: HashMap::new(),
            prompt_names: Vec::new(),
            prompt_locations: Vec::new(),
            prompt_location_index: HashMap::new(),
            mcp_server_names: Vec::new(),
            mcp_server_locations: Vec::new(),
            input_fields: BTreeMap::new(),
            input_field_metadata: BTreeMap::new(),
            input_field_locations: HashMap::new(),
            secrets_fields: BTreeMap::new(),
            secrets_field_metadata: BTreeMap::new(),
            secrets_field_locations: HashMap::new(),
            dynamic_fields: BTreeMap::new(),
            dynamic_field_metadata: BTreeMap::new(),
            dynamic_field_locations: HashMap::new(),
            agents: HashMap::new(),
            agent_dynamic_fields: HashMap::new(),
            agent_dynamic_field_metadata: HashMap::new(),
            agent_dynamic_field_locations: HashMap::new(),
            agent_output_field_locations: HashMap::new(),
            agent_for_loop_bindings: HashMap::new(),
            agent_for_loop_iterable_item_types: HashMap::new(),
            agent_names: Vec::new(),
            model_usage_locations: Vec::new(),
            inference_setting_locations: Vec::new(),
            output_locations: Vec::new(),
            typed_declaration_locations: Vec::new(),
            agent_output_locations: Vec::new(),
            agent_locations: Vec::new(),
            agent_location_index: HashMap::new(),
            has_input_declaration: false,
            has_secrets_declaration: false,
            has_output_declaration: false,
            tooling_snapshot,
            mcp_lock,
            mcp_server_tool_lookups,
            workflow_semantics,
        };

        for declaration in workflow.declarations() {
            semantic_index.insert_declaration(declaration);
        }

        semantic_index.schema_names.sort();
        semantic_index.schema_names.dedup();

        semantic_index.agent_names.sort();
        semantic_index.agent_names.dedup();

        semantic_index.tool_names.sort();
        semantic_index.tool_names.dedup();

        semantic_index.resource_names.sort();
        semantic_index.resource_names.dedup();

        semantic_index.prompt_names.sort();
        semantic_index.prompt_names.dedup();

        semantic_index
    }

    fn insert_declaration(&mut self, declaration: &Declaration) {
        match declaration {
            Declaration::Provider(provider_declaration) => {
                self.insert_provider(provider_declaration);
            }
            Declaration::Model(model_declaration) => {
                self.insert_model(model_declaration);
            }
            Declaration::McpServer(mcp_server_declaration) => {
                self.mcp_server_names.push(mcp_server_declaration.name.clone());
                self.mcp_server_locations.push(NamedSpan {
                    name: mcp_server_declaration.name.clone(),
                    span: mcp_server_declaration.span,
                });
            }
            Declaration::Schema(schema_declaration) => {
                self.insert_schema_declaration(schema_declaration);
            }
            Declaration::Input(input_declaration) => {
                self.insert_input_declaration(input_declaration);
            }
            Declaration::Secrets(secrets_declaration) => {
                self.insert_secrets_declaration(secrets_declaration);
            }
            Declaration::Agent(agent_declaration) => {
                self.insert_agent_declaration(agent_declaration);
            }
            Declaration::Tool(_) | Declaration::McpToolBatch(_) => {
                for tool_declaration in declaration.tool_declarations() {
                    self.insert_tool_declaration(tool_declaration);
                }
            }
            Declaration::McpResource(resource_import_declaration) => {
                self.insert_resource_import_declaration(resource_import_declaration);
            }
            Declaration::McpBatch(batch_import_declaration) => {
                for tool_declaration in declaration.tool_declarations() {
                    self.insert_tool_declaration(tool_declaration);
                }

                for resource_import_declaration in &batch_import_declaration.resources {
                    self.insert_resource_import_declaration(resource_import_declaration);
                }

                for prompt_import_declaration in &batch_import_declaration.prompts {
                    self.insert_prompt_import_declaration(prompt_import_declaration);
                }
            }
            Declaration::McpResourceBatch(resource_batch_import_declaration) => {
                for resource_import_declaration in &resource_batch_import_declaration.resources {
                    self.insert_resource_import_declaration(resource_import_declaration);
                }
            }
            Declaration::McpPrompt(prompt_import_declaration) => {
                self.insert_prompt_import_declaration(prompt_import_declaration);
            }
            Declaration::McpPromptBatch(prompt_batch_import_declaration) => {
                for prompt_import_declaration in &prompt_batch_import_declaration.prompts {
                    self.insert_prompt_import_declaration(prompt_import_declaration);
                }
            }
            Declaration::Dynamic(dynamic_block) => {
                self.insert_workflow_dynamic_block(dynamic_block);
            }
            Declaration::Output(output_declaration) => {
                self.has_output_declaration = true;
                self.output_locations.push(output_declaration.span);
            }
        }
    }

    fn insert_schema_declaration(&mut self, schema_declaration: &superwire_core::dsl::SchemaDeclaration) {
        self.insert_schema_field_locations(schema_declaration.name.as_str(), &schema_declaration.fields);

        let schema_fields = TypedField::type_map(&schema_declaration.fields);
        let schema_field_metadata = typed_fields_to_metadata_map(&schema_declaration.fields);

        self.schemas.insert(
            schema_declaration.name.clone(),
            SchemaSummary {
                fields: schema_fields,
                field_metadata: schema_field_metadata,
            },
        );

        self.schema_names.push(schema_declaration.name.clone());
        NamedSpan::push_indexed(
            &mut self.schema_locations,
            &mut self.schema_location_index,
            schema_declaration.name.clone(),
            schema_declaration.span,
        );
        self.typed_declaration_locations.push(schema_declaration.span);
    }

    fn insert_input_declaration(&mut self, input_declaration: &superwire_core::dsl::InputDeclaration) {
        self.has_input_declaration = true;

        if self.input_fields.is_empty() {
            self.input_fields = TypedField::type_map(&input_declaration.fields);
            self.input_field_metadata = typed_fields_to_metadata_map(&input_declaration.fields);
            self.insert_singleton_field_locations(SingletonDeclarationKind::Input, &input_declaration.fields);
        }

        self.typed_declaration_locations.push(input_declaration.span);
    }

    fn insert_secrets_declaration(&mut self, secrets_declaration: &superwire_core::dsl::SecretsDeclaration) {
        self.has_secrets_declaration = true;

        if self.secrets_fields.is_empty() {
            self.secrets_fields = TypedField::type_map(&secrets_declaration.fields);
            self.secrets_field_metadata = typed_fields_to_metadata_map(&secrets_declaration.fields);
            self.insert_singleton_field_locations(SingletonDeclarationKind::Secrets, &secrets_declaration.fields);
        }

        self.typed_declaration_locations.push(secrets_declaration.span);
    }

    fn insert_tool_declaration(&mut self, tool_declaration: &superwire_core::dsl::ToolDeclaration) {
        let (mcp_server_name, mcp_tool_name) = match &tool_declaration.source {
            Some(ToolSource::Mcp(mcp_source)) => (mcp_source.server_name.clone(), Some(mcp_source.tool_name.clone())),
            None => (None, None),
        };
        let output_type_expression = if tool_declaration.has_untyped_mcp_output() {
            None
        } else {
            Some(TypeExpression::Object(tool_declaration.output_fields.clone()))
        };

        self.tools.insert(
            tool_declaration.name.clone(),
            ToolSummary {
                description: tool_declaration.description.clone(),
                bounded_fields: TypedField::type_map(&tool_declaration.binding_fields),
                bounded_field_metadata: typed_fields_to_metadata_map(&tool_declaration.binding_fields),
                output_type_expression,
                mcp_server_name,
                mcp_tool_name,
            },
        );

        self.tool_names.push(tool_declaration.name.clone());
        NamedSpan::push_indexed(
            &mut self.tool_locations,
            &mut self.tool_location_index,
            tool_declaration.name.clone(),
            tool_declaration.span,
        );
        self.typed_declaration_locations.push(tool_declaration.span);
    }

    fn insert_resource_import_declaration(&mut self, resource_import_declaration: &superwire_core::dsl::McpResourceImportDeclaration) {
        self.resource_names.push(resource_import_declaration.name.clone());
        NamedSpan::push_indexed(
            &mut self.resource_locations,
            &mut self.resource_location_index,
            resource_import_declaration.name.clone(),
            resource_import_declaration.span,
        );
    }

    fn insert_prompt_import_declaration(&mut self, prompt_import_declaration: &superwire_core::dsl::McpPromptImportDeclaration) {
        self.prompt_names.push(prompt_import_declaration.name.clone());
        NamedSpan::push_indexed(
            &mut self.prompt_locations,
            &mut self.prompt_location_index,
            prompt_import_declaration.name.clone(),
            prompt_import_declaration.span,
        );
    }

    fn insert_agent_declaration(&mut self, agent_declaration: &superwire_core::dsl::AgentDeclaration) {
        let mut agent_dynamic_fields = self.dynamic_fields.clone();
        let mut agent_dynamic_field_metadata = self.dynamic_field_metadata.clone();
        let mut agent_dynamic_field_locations = self.dynamic_field_locations.clone();

        for dynamic_block in agent_declaration.dynamic_blocks() {
            self.insert_dynamic_block_fields(
                dynamic_block,
                &mut agent_dynamic_fields,
                &mut agent_dynamic_field_metadata,
                &mut agent_dynamic_field_locations,
            );
        }

        for agent_property in &agent_declaration.properties {
            if let AgentProperty::Model(model_usage) = agent_property {
                self.model_usage_locations.push(model_usage.span);
                self.insert_inference_setting_locations(&model_usage.properties, ModelUsagePropertyName::Inference.as_str());
            }

            if let AgentProperty::Output { fields: _, span } = agent_property {
                self.typed_declaration_locations.push(*span);
                self.agent_output_locations.push(*span);
            }
        }

        let output_type_expression = agent_declaration.output_type();

        if let Some(output_type_expression) = &output_type_expression {
            self.insert_agent_output_field_locations(agent_declaration.name.as_str(), output_type_expression);
        }

        let output_type = output_type_expression;

        self.agents.insert(
            agent_declaration.name.clone(),
            AgentSummary {
                output_type: if agent_declaration.for_loop.is_some() {
                    output_type.map(|agent_output_type| TypeExpression::Array {
                        item_type: Box::new(agent_output_type),
                        fixed_length: None,
                    })
                } else {
                    output_type
                },
            },
        );

        self.agent_dynamic_fields
            .insert(agent_declaration.name.clone(), agent_dynamic_fields);
        self.agent_dynamic_field_metadata
            .insert(agent_declaration.name.clone(), agent_dynamic_field_metadata);
        self.agent_dynamic_field_locations
            .insert(agent_declaration.name.clone(), agent_dynamic_field_locations);

        if let Some(agent_for_loop) = &agent_declaration.for_loop {
            if let Some(iterable_item_type) = self.iterable_item_type(&agent_for_loop.iterable) {
                self.agent_for_loop_iterable_item_types
                    .insert(agent_declaration.name.clone(), iterable_item_type.clone());

                let for_loop_binding_types = self.for_loop_binding_types(agent_for_loop, iterable_item_type);

                if !for_loop_binding_types.is_empty() {
                    self.agent_for_loop_bindings
                        .insert(agent_declaration.name.clone(), for_loop_binding_types);
                }
            }
        }

        self.agent_names.push(agent_declaration.name.clone());
        NamedSpan::push_indexed(
            &mut self.agent_locations,
            &mut self.agent_location_index,
            agent_declaration.name.clone(),
            agent_declaration.span,
        );
    }

    pub fn from_text_fallback(source_text: &str) -> Self {
        let tooling_snapshot = SemanticToolingSnapshot::from_source_tolerant(source_text);
        let mut semantic_index = Self::from_tooling_snapshot(&tooling_snapshot);

        semantic_index.has_input_declaration = semantic_index.has_input_declaration
            || Self::source_has_named_block_declaration(source_text, DeclarationKeyword::Input.as_str());
        semantic_index.has_secrets_declaration = semantic_index.has_secrets_declaration
            || Self::source_has_named_block_declaration(source_text, DeclarationKeyword::Secrets.as_str());
        semantic_index.has_output_declaration = semantic_index.has_output_declaration
            || Self::source_has_named_block_declaration(source_text, DeclarationKeyword::Output.as_str());
        semantic_index.mcp_server_names = Self::mcp_server_names_from_text(source_text);

        semantic_index
    }

    #[allow(clippy::too_many_lines)]
    fn from_tooling_snapshot(tooling_snapshot: &SemanticToolingSnapshot) -> Self {
        let providers = tooling_snapshot
            .declaration_index()
            .symbols_by_category(ToolingSymbolCategory::Provider)
            .map(|named_symbol_span| (named_symbol_span.name.clone(), ProviderSummary { driver: None }))
            .collect::<HashMap<_, _>>();
        let provider_locations = tooling_snapshot
            .declaration_index()
            .symbols_by_category(ToolingSymbolCategory::Provider)
            .map(|named_symbol_span| NamedSpan {
                name: named_symbol_span.name.clone(),
                span: named_symbol_span.span,
            })
            .collect::<Vec<_>>();
        let schemas = tooling_snapshot
            .schemas()
            .iter()
            .map(|(schema_name, schema_fields)| {
                (
                    schema_name.clone(),
                    SchemaSummary {
                        fields: schema_fields.clone(),
                        field_metadata: field_metadata_from_type_map(schema_fields),
                    },
                )
            })
            .collect::<HashMap<_, _>>();

        let mut schema_names = tooling_snapshot.schemas().keys().cloned().collect::<Vec<_>>();
        schema_names.sort();
        schema_names.dedup();

        let schema_locations = tooling_snapshot
            .declaration_index()
            .symbols_by_category(ToolingSymbolCategory::Schema)
            .map(|named_symbol_span| NamedSpan {
                name: named_symbol_span.name.clone(),
                span: named_symbol_span.span,
            })
            .collect::<Vec<_>>();
        let (tools, tool_names, tool_locations) = Self::tool_index_from_snapshot(tooling_snapshot);
        let agents = tooling_snapshot
            .agents()
            .iter()
            .map(|(agent_name, agent_output_type)| {
                (
                    agent_name.clone(),
                    AgentSummary {
                        output_type: agent_output_type.clone(),
                    },
                )
            })
            .collect::<HashMap<_, _>>();

        let mut agent_names = tooling_snapshot.agents().keys().cloned().collect::<Vec<_>>();
        agent_names.sort();
        agent_names.dedup();

        let agent_locations = tooling_snapshot
            .declaration_index()
            .symbols_by_category(ToolingSymbolCategory::Agent)
            .map(|named_symbol_span| NamedSpan {
                name: named_symbol_span.name.clone(),
                span: named_symbol_span.span,
            })
            .collect::<Vec<_>>();
        let provider_location_index = NamedSpan::first_span_map(&provider_locations);
        let schema_location_index = NamedSpan::first_span_map(&schema_locations);
        let tool_location_index = NamedSpan::first_span_map(&tool_locations);
        let agent_location_index = NamedSpan::first_span_map(&agent_locations);

        Self {
            providers,
            provider_locations,
            provider_location_index,
            models: HashMap::new(),
            model_locations: Vec::new(),
            model_location_index: HashMap::new(),
            schemas,
            schema_names,
            schema_locations,
            schema_location_index,
            schema_field_locations: HashMap::new(),
            tools,
            tool_names,
            tool_locations,
            tool_location_index,
            resource_names: Vec::new(),
            resource_locations: Vec::new(),
            resource_location_index: HashMap::new(),
            prompt_names: Vec::new(),
            prompt_locations: Vec::new(),
            prompt_location_index: HashMap::new(),
            mcp_server_names: Vec::new(),
            mcp_server_locations: Vec::new(),
            input_fields: tooling_snapshot.input_fields().clone(),
            input_field_metadata: field_metadata_from_type_map(tooling_snapshot.input_fields()),
            input_field_locations: HashMap::new(),
            secrets_fields: tooling_snapshot.secrets_fields().clone(),
            secrets_field_metadata: field_metadata_from_type_map(tooling_snapshot.secrets_fields()),
            secrets_field_locations: HashMap::new(),
            dynamic_fields: BTreeMap::new(),
            dynamic_field_metadata: BTreeMap::new(),
            dynamic_field_locations: HashMap::new(),
            agents,
            agent_dynamic_fields: HashMap::new(),
            agent_dynamic_field_metadata: HashMap::new(),
            agent_dynamic_field_locations: HashMap::new(),
            agent_output_field_locations: HashMap::new(),
            agent_for_loop_bindings: HashMap::new(),
            agent_for_loop_iterable_item_types: HashMap::new(),
            agent_names,
            model_usage_locations: Vec::new(),
            inference_setting_locations: Vec::new(),
            output_locations: Vec::new(),
            typed_declaration_locations: Vec::new(),
            agent_output_locations: Vec::new(),
            agent_locations,
            agent_location_index,
            has_input_declaration: !tooling_snapshot.input_fields().is_empty(),
            has_secrets_declaration: !tooling_snapshot.secrets_fields().is_empty(),
            has_output_declaration: false,
            tooling_snapshot: tooling_snapshot.clone(),
            mcp_lock: None,
            mcp_server_tool_lookups: HashMap::new(),
            workflow_semantics: None,
        }
    }

    fn mcp_server_tool_lookups(mcp_lock: Option<&McpLock>) -> HashMap<String, superwire_core::mcp::McpServerToolLookup> {
        let Some(mcp_lock) = mcp_lock else {
            return HashMap::new();
        };

        mcp_lock
            .servers
            .iter()
            .map(|(server_name, server_lock)| (server_name.clone(), server_lock.tool_lookup()))
            .collect()
    }

    fn tool_index_from_snapshot(tooling_snapshot: &SemanticToolingSnapshot) -> (HashMap<String, ToolSummary>, Vec<String>, Vec<NamedSpan>) {
        let tools = tooling_snapshot
            .tools()
            .iter()
            .map(|(tool_name, tool_schema_summary)| {
                let (mcp_server_name, mcp_tool_name) = match &tool_schema_summary.source {
                    Some(ToolSource::Mcp(mcp_source)) => (mcp_source.server_name.clone(), Some(mcp_source.tool_name.clone())),
                    None => (None, None),
                };

                (
                    tool_name.clone(),
                    ToolSummary {
                        description: tool_schema_summary.description.clone(),
                        bounded_fields: tool_schema_summary.bounded_fields.clone(),
                        bounded_field_metadata: field_metadata_from_type_map(&tool_schema_summary.bounded_fields),
                        output_type_expression: None,
                        mcp_server_name,
                        mcp_tool_name,
                    },
                )
            })
            .collect::<HashMap<_, _>>();

        let mut tool_names = tooling_snapshot.tools().keys().cloned().collect::<Vec<_>>();
        tool_names.sort();
        tool_names.dedup();

        let tool_locations = tooling_snapshot
            .declaration_index()
            .symbols_by_category(ToolingSymbolCategory::Tool)
            .map(|named_symbol_span| NamedSpan {
                name: named_symbol_span.name.clone(),
                span: named_symbol_span.span,
            })
            .collect::<Vec<_>>();

        (tools, tool_names, tool_locations)
    }

    fn mcp_server_names_from_text(source_text: &str) -> Vec<String> {
        let mut server_names = source_text.lines().filter_map(Self::mcp_server_name_from_line).collect::<Vec<_>>();

        server_names.sort();
        server_names.dedup();

        server_names
    }

    fn mcp_server_name_from_line(source_line: &str) -> Option<String> {
        let trimmed_source_line = source_line.trim_start();
        let declaration_keyword = DeclarationKeyword::Mcp.as_str();
        let after_declaration_keyword = trimmed_source_line.strip_prefix(declaration_keyword)?;

        if !after_declaration_keyword.starts_with(char::is_whitespace) {
            return None;
        }

        let server_name = after_declaration_keyword
            .trim_start()
            .chars()
            .take_while(|character| character.is_ascii_alphanumeric() || *character == '_')
            .collect::<String>();

        if server_name.is_empty() {
            return None;
        }

        Some(server_name)
    }

    fn source_has_named_block_declaration(source_text: &str, declaration_keyword: &str) -> bool {
        for source_line in source_text.lines() {
            let trimmed_line = source_line.trim_start();
            let Some(line_after_keyword) = trimmed_line.strip_prefix(declaration_keyword) else {
                continue;
            };

            if !line_after_keyword.starts_with(char::is_whitespace) {
                continue;
            }

            if line_after_keyword.trim_start().starts_with('{') {
                return true;
            }
        }

        false
    }

    fn insert_schema_field_locations(&mut self, schema_name: &str, typed_fields: &[TypedField]) {
        Self::insert_field_locations(
            &mut self.schema_field_locations,
            Self::schema_field_location_prefix(schema_name),
            typed_fields,
        );
    }

    fn insert_singleton_field_locations(&mut self, singleton_kind: SingletonDeclarationKind, typed_fields: &[TypedField]) {
        match singleton_kind {
            SingletonDeclarationKind::Input => {
                Self::insert_field_locations(&mut self.input_field_locations, Vec::new(), typed_fields);
            }
            SingletonDeclarationKind::Secrets => {
                Self::insert_field_locations(&mut self.secrets_field_locations, Vec::new(), typed_fields);
            }
            SingletonDeclarationKind::Output => {}
        }
    }

    fn insert_agent_output_field_locations(&mut self, agent_name: &str, output_type_expression: &TypeExpression) {
        let TypeExpression::Object(typed_fields) = output_type_expression else {
            return;
        };

        Self::insert_field_locations(
            &mut self.agent_output_field_locations,
            Self::agent_field_location_prefix(agent_name),
            typed_fields,
        );
    }

    fn insert_workflow_dynamic_block(&mut self, dynamic_block: &superwire_core::dsl::DynamicBlock) {
        let mut dynamic_fields = self.dynamic_fields.clone();
        let mut dynamic_field_metadata = self.dynamic_field_metadata.clone();
        let mut dynamic_field_locations = self.dynamic_field_locations.clone();

        self.insert_dynamic_block_fields(
            dynamic_block,
            &mut dynamic_fields,
            &mut dynamic_field_metadata,
            &mut dynamic_field_locations,
        );

        self.dynamic_fields = dynamic_fields;
        self.dynamic_field_metadata = dynamic_field_metadata;
        self.dynamic_field_locations = dynamic_field_locations;
    }

    fn insert_dynamic_block_fields(
        &self,
        dynamic_block: &superwire_core::dsl::DynamicBlock,
        dynamic_fields: &mut BTreeMap<String, TypeExpression>,
        dynamic_field_metadata: &mut BTreeMap<String, FieldMetadata>,
        dynamic_field_locations: &mut HashMap<String, SourceSpan>,
    ) {
        let mut pending_dynamic_fields = dynamic_block.fields.iter().collect::<Vec<_>>();

        while !pending_dynamic_fields.is_empty() {
            let pending_count_before_pass = pending_dynamic_fields.len();

            pending_dynamic_fields.retain(|dynamic_field| {
                let Some(dynamic_field_type) = self.expression_type_with_dynamic_scope(&dynamic_field.value, dynamic_fields) else {
                    return true;
                };

                dynamic_fields.insert(dynamic_field.name.clone(), dynamic_field_type.clone());
                dynamic_field_metadata.insert(
                    dynamic_field.name.clone(),
                    FieldMetadata {
                        field_type: dynamic_field_type,
                        description: None,
                    },
                );
                dynamic_field_locations.insert(dynamic_field.name.clone(), dynamic_field.span);

                false
            });

            if pending_dynamic_fields.len() == pending_count_before_pass {
                break;
            }
        }
    }

    fn insert_field_locations(
        field_locations: &mut HashMap<String, SourceSpan>,
        field_prefix_segments: Vec<String>,
        typed_fields: &[TypedField],
    ) {
        for typed_field in typed_fields {
            let mut field_path_segments = field_prefix_segments.clone();
            field_path_segments.push(typed_field.name.clone());

            let field_location_key = Self::field_location_key(field_path_segments.as_slice());
            field_locations.insert(field_location_key, typed_field.span);

            if let TypeExpression::Object(nested_typed_fields) = &typed_field.field_type {
                Self::insert_field_locations(field_locations, field_path_segments, nested_typed_fields);
            }
        }
    }

    pub(in crate::document::semantic_index) fn field_location_key(field_path_segments: &[String]) -> String {
        field_path_segments.join(".")
    }

    pub(in crate::document::semantic_index) fn schema_field_location_prefix(schema_name: &str) -> Vec<String> {
        vec![schema_name.to_string()]
    }

    fn agent_field_location_prefix(agent_name: &str) -> Vec<String> {
        vec![agent_name.to_string()]
    }

    fn insert_provider(&mut self, provider_declaration: &ProviderDeclaration) {
        let provider_driver = ProviderDriver::parse(&provider_declaration.driver_name);

        self.providers
            .insert(provider_declaration.name.clone(), ProviderSummary { driver: provider_driver });

        NamedSpan::push_indexed(
            &mut self.provider_locations,
            &mut self.provider_location_index,
            provider_declaration.name.clone(),
            provider_declaration.span,
        );
    }

    fn insert_model(&mut self, model_declaration: &ModelDeclaration) {
        self.models.insert(
            model_declaration.name.clone(),
            ModelSummary {
                provider_name: model_declaration.provider_name.clone(),
                model_identifier: model_declaration.id_literal().map(str::to_string),
            },
        );

        NamedSpan::push_indexed(
            &mut self.model_locations,
            &mut self.model_location_index,
            model_declaration.name.clone(),
            model_declaration.span,
        );
        self.insert_inference_setting_locations(&model_declaration.properties, ModelDeclarationPropertyName::Inference.as_str());
    }

    fn insert_inference_setting_locations(&mut self, properties: &[ObjectField], inference_property_name: &str) {
        let Some(inference_property) = properties.iter().find(|property| property.name == inference_property_name) else {
            return;
        };

        self.inference_setting_locations.push(NamedSpan {
            name: inference_property.name.clone(),
            span: inference_property.span,
        });
    }

    fn for_loop_binding_types(
        &self,
        agent_for_loop: &superwire_core::dsl::AgentForLoop,
        iterable_item_type: TypeExpression,
    ) -> BTreeMap<String, Vec<TypeExpression>> {
        let mut binding_types = BTreeMap::new();

        match &agent_for_loop.pattern {
            AgentForLoopPattern::Identifier(identifier) => {
                binding_types.insert(identifier.clone(), vec![iterable_item_type]);
            }
            AgentForLoopPattern::ObjectDestructuring(field_names) => {
                for field_name in field_names {
                    let resolved_field_types = self
                        .tooling_snapshot
                        .resolve_access_path_types(vec![iterable_item_type.clone()], std::slice::from_ref(field_name));

                    if resolved_field_types.is_empty() {
                        continue;
                    }

                    binding_types.insert(field_name.clone(), resolved_field_types);
                }
            }
        }

        binding_types
    }

    fn iterable_item_type(&self, iterable_expression: &Expression) -> Option<TypeExpression> {
        let iterable_type = self.expression_type(iterable_expression)?;

        match iterable_type {
            TypeExpression::Array {
                item_type,
                fixed_length: _,
            } => Some(*item_type),
            TypeExpression::Tuple(tuple_member_types) => Some(TypeExpression::Union(tuple_member_types)),
            TypeExpression::String
            | TypeExpression::Number
            | TypeExpression::Float
            | TypeExpression::Boolean
            | TypeExpression::Null
            | TypeExpression::AnyObject
            | TypeExpression::Object(_)
            | TypeExpression::SchemaReference(_)
            | TypeExpression::StringEnum(_)
            | TypeExpression::StringEnumReference(_)
            | TypeExpression::Variant {
                discriminator: _,
                cases: _,
            }
            | TypeExpression::Union(_) => None,
        }
    }

    fn expression_type(&self, expression: &Expression) -> Option<TypeExpression> {
        self.expression_type_with_dynamic_scope(expression, &self.dynamic_fields)
    }

    fn expression_type_with_dynamic_scope(
        &self,
        expression: &Expression,
        dynamic_fields: &BTreeMap<String, TypeExpression>,
    ) -> Option<TypeExpression> {
        match expression {
            Expression::StringLiteral(_) | Expression::StringTemplate(_) => Some(TypeExpression::String),
            Expression::NumberLiteral(number_literal) => {
                if number_literal.contains('.') {
                    return Some(TypeExpression::Float);
                }

                Some(TypeExpression::Number)
            }
            Expression::BooleanLiteral(_) => Some(TypeExpression::Boolean),
            Expression::NullLiteral => Some(TypeExpression::Null),
            Expression::Reference(reference) => self.reference_expression_type(reference, dynamic_fields),
            Expression::FunctionCall(_) => None,
            Expression::Asset(_) => Some(TypeExpression::AnyObject),
            Expression::McpCall(_) => Some(TypeExpression::String),
            Expression::NullFallback(null_fallback) => self.expression_type_with_dynamic_scope(&null_fallback.value, dynamic_fields),
            Expression::VariantProjection(_) | Expression::Match(_) => None,
            Expression::ToolCall(tool_call) => {
                let tool_name = tool_call.callee.first_access_field()?;
                let tool_summary = self.tools.get(tool_name)?;

                tool_summary.output_type_expression.clone()
            }
            Expression::ArrayLiteral(array_items) => {
                let mut array_item_types = array_items
                    .iter()
                    .filter_map(|array_item| self.expression_type_with_dynamic_scope(array_item, dynamic_fields))
                    .collect::<Vec<_>>();

                if array_item_types.is_empty() {
                    return None;
                }

                if array_item_types.len() == 1 {
                    return Some(TypeExpression::Array {
                        item_type: Box::new(array_item_types.remove(0)),
                        fixed_length: None,
                    });
                }

                Some(TypeExpression::Array {
                    item_type: Box::new(TypeExpression::Union(array_item_types)),
                    fixed_length: None,
                })
            }
            Expression::ObjectLiteral(object_fields) => {
                let typed_fields = object_fields
                    .iter()
                    .filter_map(|object_field| {
                        let field_type = self.expression_type_with_dynamic_scope(&object_field.value, dynamic_fields)?;

                        Some(TypedField::from_type(object_field.name.clone(), field_type, object_field.span))
                    })
                    .collect::<Vec<_>>();

                Some(TypeExpression::Object(typed_fields))
            }
        }
    }

    fn reference_expression_type(
        &self,
        reference: &superwire_core::dsl::Reference,
        dynamic_fields: &BTreeMap<String, TypeExpression>,
    ) -> Option<TypeExpression> {
        let reference_keyword = reference.root_keyword()?;
        let reference_accesses = reference.accesses.clone();
        let reference_access_fields = reference
            .accesses
            .iter()
            .map(|reference_access| reference_access.field.clone())
            .collect::<Vec<_>>();

        match reference_keyword {
            ReferenceKeyword::Dynamic => self.resolve_singleton_reference_type(dynamic_fields, &reference_accesses),
            ReferenceKeyword::Input => self.resolve_singleton_reference_type(&self.input_fields, &reference_accesses),
            ReferenceKeyword::Secrets => self.resolve_singleton_reference_type(&self.secrets_fields, &reference_accesses),
            ReferenceKeyword::Agent => {
                let agent_name = reference_access_fields.first()?;
                let agent_output_type = self.agents.get(agent_name)?.output_type.clone()?;

                if reference_accesses.len() == 1 {
                    return Some(agent_output_type);
                }

                let candidate_types = self
                    .tooling_snapshot
                    .resolve_reference_access_path_types(vec![agent_output_type], &reference_accesses[1..]);

                candidate_types.first().cloned()
            }
            ReferenceKeyword::Model | ReferenceKeyword::Tool | ReferenceKeyword::Resource | ReferenceKeyword::Prompt => None,
        }
    }
}

fn typed_fields_to_metadata_map(typed_fields: &[TypedField]) -> BTreeMap<String, FieldMetadata> {
    typed_fields
        .iter()
        .map(|typed_field| {
            (
                typed_field.name.clone(),
                FieldMetadata {
                    field_type: typed_field.field_type.clone(),
                    description: typed_field.description.clone(),
                },
            )
        })
        .collect()
}

fn field_metadata_from_type_map(type_map: &BTreeMap<String, TypeExpression>) -> BTreeMap<String, FieldMetadata> {
    type_map
        .iter()
        .map(|(field_name, field_type)| {
            (
                field_name.clone(),
                FieldMetadata {
                    field_type: field_type.clone(),
                    description: None,
                },
            )
        })
        .collect()
}
