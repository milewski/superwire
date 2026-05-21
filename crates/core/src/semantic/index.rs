use crate::dsl::{
    AgentDeclaration, Declaration, McpPromptImportDeclaration, McpResourceImportDeclaration, ModelDeclaration, ObjectField,
    ProviderDeclaration, SchemaDeclaration, SingletonDeclarationKind, SourceSpan, ToolDeclaration, TypeExpression, TypedField,
    ValidationIssue, ValidationReport, Workflow,
};
use crate::semantic::support::types::{workflow_type_from_dsl, WorkflowType};
use crate::semantic::ProviderDriver;
use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;

#[derive(Debug, Clone, Default)]
pub struct WorkflowSemanticIndex {
    provider_names: HashSet<String>,
    model_names: HashSet<String>,
    agent_names: HashSet<String>,
    tool_names: HashSet<String>,
    resource_names: HashSet<String>,
    prompt_names: HashSet<String>,
    schema_names: HashSet<String>,
    schema_field_types: HashMap<String, HashMap<String, TypeExpression>>,
    schema_types: HashMap<String, TypeExpression>,
    input_field_types: Option<HashMap<String, TypeExpression>>,
    secrets_field_types: Option<HashMap<String, TypeExpression>>,
    agent_output_types: HashMap<String, Option<TypeExpression>>,
    tool_input_types: HashMap<String, WorkflowType>,
    tool_binding_types: HashMap<String, WorkflowType>,
    tool_fixed_binding_names: HashMap<String, HashSet<String>>,
    tool_fixed_binding_fields: HashMap<String, Vec<ObjectField>>,
    tool_output_types: HashMap<String, WorkflowType>,
}

impl WorkflowSemanticIndex {
    #[must_use]
    pub fn from_workflow(workflow: &Workflow) -> Self {
        let mut validation_report = ValidationReport::default();

        Self::build_for_validation(workflow, &mut validation_report)
    }

    #[must_use]
    pub fn has_provider(&self, provider_name: &str) -> bool {
        self.provider_names.contains(provider_name)
    }

    #[must_use]
    pub fn has_model(&self, model_name: &str) -> bool {
        self.model_names.contains(model_name)
    }

    #[must_use]
    pub fn has_agent(&self, agent_name: &str) -> bool {
        self.agent_names.contains(agent_name)
    }

    #[must_use]
    pub fn has_tool(&self, tool_name: &str) -> bool {
        self.tool_names.contains(tool_name)
    }

    #[must_use]
    pub fn has_resource(&self, resource_name: &str) -> bool {
        self.resource_names.contains(resource_name)
    }

    #[must_use]
    pub fn has_prompt(&self, prompt_name: &str) -> bool {
        self.prompt_names.contains(prompt_name)
    }

    #[must_use]
    pub fn has_schema(&self, schema_name: &str) -> bool {
        self.schema_names.contains(schema_name)
    }

    pub fn provider_names(&self) -> impl Iterator<Item = &str> {
        self.provider_names.iter().map(String::as_str)
    }

    pub fn model_names(&self) -> impl Iterator<Item = &str> {
        self.model_names.iter().map(String::as_str)
    }

    pub fn agent_names(&self) -> impl Iterator<Item = &str> {
        self.agent_names.iter().map(String::as_str)
    }

    pub fn tool_names(&self) -> impl Iterator<Item = &str> {
        self.tool_names.iter().map(String::as_str)
    }

    pub fn resource_names(&self) -> impl Iterator<Item = &str> {
        self.resource_names.iter().map(String::as_str)
    }

    pub fn prompt_names(&self) -> impl Iterator<Item = &str> {
        self.prompt_names.iter().map(String::as_str)
    }

    pub fn schema_names(&self) -> impl Iterator<Item = &str> {
        self.schema_names.iter().map(String::as_str)
    }

    #[must_use]
    pub fn input_field_types(&self) -> Option<&HashMap<String, TypeExpression>> {
        self.input_field_types.as_ref()
    }

    #[must_use]
    pub fn secrets_field_types(&self) -> Option<&HashMap<String, TypeExpression>> {
        self.secrets_field_types.as_ref()
    }

    #[must_use]
    pub fn agent_output_type(&self, agent_name: &str) -> Option<&Option<TypeExpression>> {
        self.agent_output_types.get(agent_name)
    }

    #[must_use]
    pub fn tool_input_type(&self, tool_name: &str) -> Option<&WorkflowType> {
        self.tool_input_types.get(tool_name)
    }

    #[must_use]
    pub fn tool_binding_type(&self, tool_name: &str) -> Option<&WorkflowType> {
        self.tool_binding_types.get(tool_name)
    }

    #[must_use]
    pub fn tool_fixed_binding_names(&self, tool_name: &str) -> Option<&HashSet<String>> {
        self.tool_fixed_binding_names.get(tool_name)
    }

    #[must_use]
    pub fn tool_fixed_binding_fields(&self, tool_name: &str) -> Option<&[ObjectField]> {
        self.tool_fixed_binding_fields.get(tool_name).map(Vec::as_slice)
    }

    #[must_use]
    pub fn tool_output_type(&self, tool_name: &str) -> Option<&WorkflowType> {
        self.tool_output_types.get(tool_name)
    }

    #[must_use]
    pub fn schema_type_expression(&self, schema_name: &str, span: SourceSpan) -> Option<TypeExpression> {
        if let Some(schema_type) = self.schema_types.get(schema_name) {
            return Some(schema_type.clone());
        }

        let schema_field_types = self.schema_field_types.get(schema_name)?;
        let typed_fields = schema_field_types
            .iter()
            .map(|(field_name, field_type)| TypedField {
                name: field_name.clone(),
                field_type: field_type.clone(),
                description: None,
                span,
            })
            .collect::<Vec<_>>();

        Some(TypeExpression::Object(typed_fields))
    }

    #[must_use]
    pub fn named_schema_types(&self, span: SourceSpan) -> HashMap<String, TypeExpression> {
        self.schema_names
            .iter()
            .filter_map(|schema_name| {
                self.schema_type_expression(schema_name, span)
                    .map(|schema_type| (schema_name.clone(), schema_type))
            })
            .collect()
    }

    #[must_use]
    pub fn stable_summary(&self) -> String {
        let mut summary_text = String::new();

        self.push_name_summary_section("providers", self.provider_names(), &mut summary_text);
        self.push_name_summary_section("models", self.model_names(), &mut summary_text);
        self.push_name_summary_section("schemas", self.schema_names(), &mut summary_text);
        self.push_name_summary_section("tools", self.tool_names(), &mut summary_text);
        self.push_name_summary_section("resources", self.resource_names(), &mut summary_text);
        self.push_name_summary_section("prompts", self.prompt_names(), &mut summary_text);
        self.push_name_summary_section("agents", self.agent_names(), &mut summary_text);
        self.push_schema_type_summary_section(&mut summary_text);
        Self::push_optional_type_expression_summary_section("input fields", self.input_field_types.as_ref(), &mut summary_text);
        Self::push_optional_type_expression_summary_section("secrets fields", self.secrets_field_types.as_ref(), &mut summary_text);
        self.push_agent_output_summary_section(&mut summary_text);
        Self::push_workflow_type_summary_section("tool input types", &self.tool_input_types, &mut summary_text);
        Self::push_workflow_type_summary_section("tool binding types", &self.tool_binding_types, &mut summary_text);
        Self::push_workflow_type_summary_section("tool output types", &self.tool_output_types, &mut summary_text);
        self.push_tool_fixed_binding_summary_section(&mut summary_text);

        summary_text
    }

    fn push_name_summary_section<'index>(&self, section_name: &str, names: impl Iterator<Item = &'index str>, summary_text: &mut String) {
        let mut sorted_names = names.map(str::to_string).collect::<Vec<_>>();
        sorted_names.sort();

        Self::push_summary_section_header(section_name, summary_text);

        if sorted_names.is_empty() {
            summary_text.push_str("  - none\n");

            return;
        }

        for name in sorted_names {
            let _ = writeln!(summary_text, "  - {name}");
        }
    }

    fn push_schema_type_summary_section(&self, summary_text: &mut String) {
        Self::push_summary_section_header("schema types", summary_text);

        if self.schema_names.is_empty() {
            summary_text.push_str("  - none\n");

            return;
        }

        let mut schema_names = self.schema_names.iter().cloned().collect::<Vec<_>>();
        schema_names.sort();

        for schema_name in schema_names {
            let schema_type_summary = self
                .schema_types
                .get(&schema_name)
                .map(TypeExpression::summary_text)
                .or_else(|| {
                    self.schema_field_types
                        .get(&schema_name)
                        .map(Self::type_expression_field_map_summary)
                })
                .unwrap_or_else(|| "unknown".to_string());

            let _ = writeln!(summary_text, "  - {schema_name}: {schema_type_summary}");
        }
    }

    fn push_optional_type_expression_summary_section(
        section_name: &str,
        field_types: Option<&HashMap<String, TypeExpression>>,
        summary_text: &mut String,
    ) {
        Self::push_summary_section_header(section_name, summary_text);

        let Some(field_types) = field_types else {
            summary_text.push_str("  - none\n");

            return;
        };

        if field_types.is_empty() {
            summary_text.push_str("  - none\n");

            return;
        }

        for field_summary in Self::sorted_type_expression_field_summaries(field_types) {
            let _ = writeln!(summary_text, "  - {field_summary}");
        }
    }

    fn push_agent_output_summary_section(&self, summary_text: &mut String) {
        Self::push_summary_section_header("agent output types", summary_text);

        if self.agent_output_types.is_empty() {
            summary_text.push_str("  - none\n");

            return;
        }

        let mut agent_names = self.agent_output_types.keys().cloned().collect::<Vec<_>>();
        agent_names.sort();

        for agent_name in agent_names {
            let output_type_summary = self
                .agent_output_types
                .get(&agent_name)
                .and_then(Option::as_ref)
                .map_or_else(|| "none".to_string(), TypeExpression::summary_text);

            let _ = writeln!(summary_text, "  - {agent_name}: {output_type_summary}");
        }
    }

    fn push_workflow_type_summary_section(section_name: &str, workflow_types: &HashMap<String, WorkflowType>, summary_text: &mut String) {
        Self::push_summary_section_header(section_name, summary_text);

        if workflow_types.is_empty() {
            summary_text.push_str("  - none\n");

            return;
        }

        let mut names = workflow_types.keys().cloned().collect::<Vec<_>>();
        names.sort();

        for name in names {
            let workflow_type = workflow_types
                .get(&name)
                .expect("sorted workflow type names should come from workflow type map");

            let _ = writeln!(summary_text, "  - {name}: {workflow_type}");
        }
    }

    fn push_tool_fixed_binding_summary_section(&self, summary_text: &mut String) {
        Self::push_summary_section_header("tool fixed bindings", summary_text);

        if self.tool_fixed_binding_names.is_empty() {
            summary_text.push_str("  - none\n");

            return;
        }

        let mut tool_names = self.tool_fixed_binding_names.keys().cloned().collect::<Vec<_>>();
        tool_names.sort();

        for tool_name in tool_names {
            let mut fixed_binding_names = self
                .tool_fixed_binding_names
                .get(&tool_name)
                .expect("sorted tool names should come from tool fixed binding names")
                .iter()
                .cloned()
                .collect::<Vec<_>>();
            fixed_binding_names.sort();

            let fixed_binding_summary = if fixed_binding_names.is_empty() {
                "none".to_string()
            } else {
                fixed_binding_names.join(", ")
            };

            let _ = writeln!(summary_text, "  - {tool_name}: {fixed_binding_summary}");
        }
    }

    fn type_expression_field_map_summary(field_types: &HashMap<String, TypeExpression>) -> String {
        format!("{{ {} }}", Self::sorted_type_expression_field_summaries(field_types).join(", "))
    }

    fn sorted_type_expression_field_summaries(field_types: &HashMap<String, TypeExpression>) -> Vec<String> {
        let mut field_names = field_types.keys().cloned().collect::<Vec<_>>();
        field_names.sort();

        field_names
            .into_iter()
            .map(|field_name| {
                let field_type = field_types
                    .get(&field_name)
                    .expect("sorted field names should come from field type map");

                format!("{field_name}: {}", field_type.summary_text())
            })
            .collect()
    }

    fn push_summary_section_header(section_name: &str, summary_text: &mut String) {
        if !summary_text.is_empty() {
            summary_text.push('\n');
        }

        let _ = writeln!(summary_text, "{section_name}:");
    }
}

impl WorkflowSemanticIndex {
    pub(crate) fn register_provider_name(
        &mut self,
        provider_declaration: &ProviderDeclaration,
        validation_report: &mut ValidationReport,
    ) -> bool {
        provider_declaration.validate_name(validation_report);

        let inserted_provider = self.provider_names.insert(provider_declaration.name.clone());

        if !inserted_provider {
            validation_report.push_issue_with_span(
                ValidationIssue::DuplicateProvider {
                    provider_name: provider_declaration.name.clone(),
                },
                Some(provider_declaration.span),
            );
        }

        inserted_provider
    }

    pub(crate) fn register_model_name(&mut self, model_declaration: &ModelDeclaration, validation_report: &mut ValidationReport) -> bool {
        model_declaration.validate_name(validation_report);

        let inserted_model = self.model_names.insert(model_declaration.name.clone());

        if !inserted_model {
            validation_report.push_issue_with_span(
                ValidationIssue::DuplicateModel {
                    model_name: model_declaration.name.clone(),
                },
                Some(model_declaration.span),
            );
        }

        inserted_model
    }

    pub(crate) fn register_schema_name(
        &mut self,
        schema_declaration: &SchemaDeclaration,
        validation_report: &mut ValidationReport,
    ) -> bool {
        schema_declaration.validate_name(validation_report);

        let inserted_schema = self.schema_names.insert(schema_declaration.name.clone());

        if !inserted_schema {
            validation_report.push_issue_with_span(
                ValidationIssue::DuplicateSchema {
                    schema_name: schema_declaration.name.clone(),
                },
                Some(schema_declaration.span),
            );
        }

        inserted_schema
    }

    pub(crate) fn register_tool_name(&mut self, tool_declaration: &ToolDeclaration, validation_report: &mut ValidationReport) -> bool {
        let inserted_tool = self.tool_names.insert(tool_declaration.name.clone());

        if !inserted_tool {
            validation_report.push_issue_with_span(
                ValidationIssue::DuplicateTool {
                    tool_name: tool_declaration.name.clone(),
                },
                Some(tool_declaration.span),
            );
        }

        inserted_tool
    }

    pub(crate) fn register_resource_name(
        &mut self,
        resource_import_declaration: &McpResourceImportDeclaration,
        validation_report: &mut ValidationReport,
    ) -> bool {
        let inserted_resource = self.resource_names.insert(resource_import_declaration.name.clone());

        if !inserted_resource {
            validation_report.push_issue_with_span(
                ValidationIssue::DuplicateResource {
                    resource_name: resource_import_declaration.name.clone(),
                },
                Some(resource_import_declaration.span),
            );
        }

        inserted_resource
    }

    pub(crate) fn register_prompt_name(
        &mut self,
        prompt_import_declaration: &McpPromptImportDeclaration,
        validation_report: &mut ValidationReport,
    ) -> bool {
        let inserted_prompt = self.prompt_names.insert(prompt_import_declaration.name.clone());

        if !inserted_prompt {
            validation_report.push_issue_with_span(
                ValidationIssue::DuplicatePrompt {
                    prompt_name: prompt_import_declaration.name.clone(),
                },
                Some(prompt_import_declaration.span),
            );
        }

        inserted_prompt
    }

    pub(crate) fn register_agent_name(&mut self, agent_declaration: &AgentDeclaration, validation_report: &mut ValidationReport) -> bool {
        let inserted_agent = self.agent_names.insert(agent_declaration.name.clone());

        if !inserted_agent {
            validation_report.push_issue_with_span(
                ValidationIssue::DuplicateAgent {
                    agent_name: agent_declaration.name.clone(),
                },
                Some(agent_declaration.span),
            );
        }

        inserted_agent
    }

    #[allow(clippy::too_many_lines)]
    pub(crate) fn build_for_validation(workflow: &Workflow, validation_report: &mut ValidationReport) -> Self {
        let mut validation_index = Self::default();

        let mut has_input_declaration = false;
        let mut has_secrets_declaration = false;
        let mut has_output_declaration = false;

        for declaration in workflow.declarations() {
            match declaration {
                Declaration::Provider(provider_declaration) => {
                    let provider_name = provider_declaration.name.clone();

                    if !validation_index.register_provider_name(provider_declaration, validation_report) {
                        continue;
                    }

                    let provider_driver = ProviderDriver::parse(&provider_declaration.driver_name);

                    if provider_driver.is_none() {
                        validation_report.push_issue_with_span(
                            ValidationIssue::UnknownProviderDriver {
                                provider_name: provider_name.clone(),
                                driver_name: provider_declaration.driver_name.clone(),
                            },
                            Some(provider_declaration.span),
                        );
                    }
                }
                Declaration::Model(model_declaration) => {
                    let model_name = model_declaration.name.clone();

                    if !validation_index.register_model_name(model_declaration, validation_report) {
                        continue;
                    }

                    if !validation_index.provider_names.contains(&model_declaration.provider_name) {
                        validation_report.push_issue_with_span(
                            ValidationIssue::UnknownProviderInModelDeclaration {
                                model_name: model_name.clone(),
                                provider_name: model_declaration.provider_name.clone(),
                            },
                            Some(model_declaration.span),
                        );
                    }

                    if model_declaration.id_expression().is_none() {
                        validation_report.push_issue_with_span(
                            ValidationIssue::MissingModelId {
                                model_name: model_name.clone(),
                            },
                            Some(model_declaration.span),
                        );
                    }
                }
                Declaration::McpServer(_) => {}
                Declaration::Schema(schema_declaration) => {
                    if !validation_index.register_schema_name(schema_declaration, validation_report) {
                        continue;
                    }

                    let schema_field_types = Self::collect_field_types(schema_declaration.fields.as_slice());
                    validation_index
                        .schema_field_types
                        .insert(schema_declaration.name.clone(), schema_field_types);
                    validation_index
                        .schema_types
                        .insert(schema_declaration.name.clone(), schema_declaration.type_expression());
                }
                Declaration::Tool(_) | Declaration::McpToolBatch(_) => {
                    for tool_declaration in declaration.tool_declarations() {
                        let named_schema_types = validation_index.named_schema_types(tool_declaration.span);

                        if let Ok(tool_input_type) =
                            workflow_type_from_dsl(&TypeExpression::Object(tool_declaration.input_fields.clone()), &named_schema_types)
                        {
                            validation_index
                                .tool_input_types
                                .insert(tool_declaration.name.clone(), tool_input_type);
                        }

                        if let Ok(tool_binding_type) = workflow_type_from_dsl(
                            &TypeExpression::Object(tool_declaration.binding_fields.clone()),
                            &named_schema_types,
                        ) {
                            validation_index
                                .tool_binding_types
                                .insert(tool_declaration.name.clone(), tool_binding_type);
                        }

                        let fixed_binding_names = tool_declaration
                            .fixed_binding_fields
                            .iter()
                            .map(|fixed_binding| fixed_binding.name.clone())
                            .collect::<HashSet<_>>();

                        if !fixed_binding_names.is_empty() {
                            validation_index
                                .tool_fixed_binding_names
                                .insert(tool_declaration.name.clone(), fixed_binding_names);
                        }

                        if !tool_declaration.fixed_binding_fields.is_empty() {
                            validation_index
                                .tool_fixed_binding_fields
                                .insert(tool_declaration.name.clone(), tool_declaration.fixed_binding_fields.clone());
                        }

                        if tool_declaration.has_untyped_mcp_output() {
                            validation_index
                                .tool_output_types
                                .insert(tool_declaration.name.clone(), crate::semantic::support::types::WorkflowType::Any);
                        } else if let Ok(tool_output_type) =
                            workflow_type_from_dsl(&TypeExpression::Object(tool_declaration.output_fields.clone()), &named_schema_types)
                        {
                            validation_index
                                .tool_output_types
                                .insert(tool_declaration.name.clone(), tool_output_type);
                        }

                        validation_index.register_tool_name(tool_declaration, validation_report);
                    }
                }
                Declaration::McpBatch(batch_import_declaration) => {
                    for tool_declaration in declaration.tool_declarations() {
                        let named_schema_types = validation_index.named_schema_types(tool_declaration.span);

                        if let Ok(tool_input_type) =
                            workflow_type_from_dsl(&TypeExpression::Object(tool_declaration.input_fields.clone()), &named_schema_types)
                        {
                            validation_index
                                .tool_input_types
                                .insert(tool_declaration.name.clone(), tool_input_type);
                        }

                        if let Ok(tool_binding_type) = workflow_type_from_dsl(
                            &TypeExpression::Object(tool_declaration.binding_fields.clone()),
                            &named_schema_types,
                        ) {
                            validation_index
                                .tool_binding_types
                                .insert(tool_declaration.name.clone(), tool_binding_type);
                        }

                        let fixed_binding_names = tool_declaration
                            .fixed_binding_fields
                            .iter()
                            .map(|fixed_binding| fixed_binding.name.clone())
                            .collect::<HashSet<_>>();

                        if !fixed_binding_names.is_empty() {
                            validation_index
                                .tool_fixed_binding_names
                                .insert(tool_declaration.name.clone(), fixed_binding_names);
                        }

                        if !tool_declaration.fixed_binding_fields.is_empty() {
                            validation_index
                                .tool_fixed_binding_fields
                                .insert(tool_declaration.name.clone(), tool_declaration.fixed_binding_fields.clone());
                        }

                        if tool_declaration.has_untyped_mcp_output() {
                            validation_index
                                .tool_output_types
                                .insert(tool_declaration.name.clone(), crate::semantic::support::types::WorkflowType::Any);
                        } else if let Ok(tool_output_type) =
                            workflow_type_from_dsl(&TypeExpression::Object(tool_declaration.output_fields.clone()), &named_schema_types)
                        {
                            validation_index
                                .tool_output_types
                                .insert(tool_declaration.name.clone(), tool_output_type);
                        }

                        validation_index.register_tool_name(tool_declaration, validation_report);
                    }

                    for resource_import_declaration in &batch_import_declaration.resources {
                        validation_index.register_resource_name(resource_import_declaration, validation_report);
                    }

                    for prompt_import_declaration in &batch_import_declaration.prompts {
                        validation_index.register_prompt_name(prompt_import_declaration, validation_report);
                    }
                }
                Declaration::McpResource(resource_import_declaration) => {
                    validation_index.register_resource_name(resource_import_declaration, validation_report);
                }
                Declaration::McpResourceBatch(resource_batch_import_declaration) => {
                    for resource_import_declaration in &resource_batch_import_declaration.resources {
                        validation_index.register_resource_name(resource_import_declaration, validation_report);
                    }
                }
                Declaration::McpPrompt(prompt_import_declaration) => {
                    validation_index.register_prompt_name(prompt_import_declaration, validation_report);
                }
                Declaration::McpPromptBatch(prompt_batch_import_declaration) => {
                    for prompt_import_declaration in &prompt_batch_import_declaration.prompts {
                        validation_index.register_prompt_name(prompt_import_declaration, validation_report);
                    }
                }
                Declaration::Dynamic(_) => {}
                Declaration::Agent(agent_declaration) => {
                    if !validation_index.register_agent_name(agent_declaration, validation_report) {
                        continue;
                    }

                    let agent_output_type = agent_declaration.declared_final_output_type_expression();
                    validation_index
                        .agent_output_types
                        .insert(agent_declaration.name.clone(), agent_output_type);
                }
                Declaration::Input(input_declaration) => {
                    if has_input_declaration {
                        validation_report.push_issue_with_span(
                            ValidationIssue::DuplicateSingletonDeclaration {
                                declaration_kind: SingletonDeclarationKind::Input,
                            },
                            Some(input_declaration.span),
                        );
                    }

                    has_input_declaration = true;

                    if validation_index.input_field_types.is_none() {
                        validation_index.input_field_types = Some(Self::collect_field_types(input_declaration.fields.as_slice()));
                    }
                }
                Declaration::Secrets(secrets_declaration) => {
                    if has_secrets_declaration {
                        validation_report.push_issue_with_span(
                            ValidationIssue::DuplicateSingletonDeclaration {
                                declaration_kind: SingletonDeclarationKind::Secrets,
                            },
                            Some(secrets_declaration.span),
                        );
                    }

                    has_secrets_declaration = true;

                    if validation_index.secrets_field_types.is_none() {
                        validation_index.secrets_field_types = Some(Self::collect_field_types(secrets_declaration.fields.as_slice()));
                    }
                }
                Declaration::Output(output_declaration) => {
                    if has_output_declaration {
                        validation_report.push_issue_with_span(
                            ValidationIssue::DuplicateSingletonDeclaration {
                                declaration_kind: SingletonDeclarationKind::Output,
                            },
                            Some(output_declaration.span),
                        );
                    }

                    has_output_declaration = true;
                }
            }
        }

        validation_index
    }

    fn collect_field_types(typed_fields: &[TypedField]) -> HashMap<String, TypeExpression> {
        typed_fields
            .iter()
            .map(|typed_field| (typed_field.name.clone(), typed_field.field_type.clone()))
            .collect()
    }
}
