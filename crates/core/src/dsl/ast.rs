use super::structure::{self, DslProperty, PropertyDefinition as DslPropertyDefinition};
use serde_json::{Map, Value};
use std::collections::{HashMap, HashSet};
use std::hash::BuildHasher;
use std::ops::Range;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SourcePosition {
    pub line: usize,
    pub column: usize,
}

impl SourcePosition {
    #[must_use]
    pub fn to_byte_offset(self, source_text: &str) -> Option<usize> {
        if self.line == 0 || self.column == 0 {
            return None;
        }

        let mut current_line_number = 1_usize;
        let mut current_column_number = 1_usize;

        for (byte_offset, character) in source_text.char_indices() {
            if current_line_number == self.line && current_column_number == self.column {
                return Some(byte_offset);
            }

            if character == '\n' {
                current_line_number += 1;
                current_column_number = 1;

                continue;
            }

            current_column_number += 1;
        }

        if current_line_number == self.line && current_column_number == self.column {
            return Some(source_text.len());
        }

        None
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SourceSpan {
    pub start: SourcePosition,
    pub end: SourcePosition,
}

impl SourceSpan {
    #[must_use]
    pub fn to_byte_range(self, source_text: &str) -> Option<Range<usize>> {
        let start_byte_offset = self.start.to_byte_offset(source_text)?;
        let mut end_byte_offset = self.end.to_byte_offset(source_text)?;

        if end_byte_offset < start_byte_offset {
            return None;
        }

        if end_byte_offset == start_byte_offset {
            if let Some(character_at_start) = source_text[start_byte_offset..].chars().next() {
                end_byte_offset = start_byte_offset + character_at_start.len_utf8();
            }
        }

        Some(start_byte_offset..end_byte_offset)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Workflow {
    pub declarations: Vec<Declaration>,
    pub source_text: Option<String>,
}

impl Workflow {
    #[must_use]
    pub fn declarations(&self) -> &[Declaration] {
        &self.declarations
    }

    #[must_use]
    pub fn source_text(&self) -> Option<&str> {
        self.source_text.as_deref()
    }

    #[must_use]
    pub fn with_source_text(mut self, source_text: impl Into<String>) -> Self {
        self.source_text = Some(source_text.into());

        self
    }

    #[must_use]
    pub fn find_provider(&self, provider_name: &str) -> Option<&ProviderDeclaration> {
        self.declarations.iter().find_map(|declaration| match declaration {
            Declaration::Provider(provider_declaration) if provider_declaration.name == provider_name => Some(provider_declaration),
            _ => None,
        })
    }

    #[must_use]
    pub fn find_model(&self, model_name: &str) -> Option<&ModelDeclaration> {
        self.declarations.iter().find_map(|declaration| match declaration {
            Declaration::Model(model_declaration) if model_declaration.name == model_name => Some(model_declaration),
            _ => None,
        })
    }

    #[must_use]
    pub fn find_mcp_server(&self, server_name: &str) -> Option<&McpServerDeclaration> {
        self.declarations.iter().find_map(|declaration| match declaration {
            Declaration::McpServer(mcp_server_declaration) if mcp_server_declaration.name == server_name => Some(mcp_server_declaration),
            _ => None,
        })
    }

    #[must_use]
    pub fn find_secrets(&self) -> Option<&SecretsDeclaration> {
        self.declarations.iter().find_map(|declaration| match declaration {
            Declaration::Secrets(secrets_declaration) => Some(secrets_declaration),
            _ => None,
        })
    }

    #[must_use]
    pub fn find_input(&self) -> Option<&InputDeclaration> {
        self.declarations.iter().find_map(|declaration| match declaration {
            Declaration::Input(input_declaration) => Some(input_declaration),
            _ => None,
        })
    }

    #[must_use]
    pub fn find_schema(&self, schema_name: &str) -> Option<&SchemaDeclaration> {
        self.declarations.iter().find_map(|declaration| match declaration {
            Declaration::Schema(schema_declaration) if schema_declaration.name == schema_name => Some(schema_declaration),
            _ => None,
        })
    }

    #[must_use]
    pub fn find_tool(&self, tool_name: &str) -> Option<&ToolDeclaration> {
        self.tool_declarations().find(|tool_declaration| tool_declaration.name == tool_name)
    }

    pub fn tool_declarations(&self) -> impl Iterator<Item = &ToolDeclaration> {
        self.declarations.iter().flat_map(Declaration::tool_declarations)
    }

    #[must_use]
    pub fn find_resource_import(&self, resource_name: &str) -> Option<&McpResourceImportDeclaration> {
        self.declarations.iter().find_map(|declaration| match declaration {
            Declaration::McpResource(resource_import_declaration) if resource_import_declaration.name == resource_name => {
                Some(resource_import_declaration)
            }
            Declaration::McpBatch(batch_import_declaration) => batch_import_declaration
                .resources
                .iter()
                .find(|resource_import_declaration| resource_import_declaration.name == resource_name),
            Declaration::McpResourceBatch(resource_batch_import_declaration) => resource_batch_import_declaration
                .resources
                .iter()
                .find(|resource_import_declaration| resource_import_declaration.name == resource_name),
            _ => None,
        })
    }

    #[must_use]
    pub fn find_prompt_import(&self, prompt_name: &str) -> Option<&McpPromptImportDeclaration> {
        self.declarations.iter().find_map(|declaration| match declaration {
            Declaration::McpPrompt(prompt_import_declaration) if prompt_import_declaration.name == prompt_name => {
                Some(prompt_import_declaration)
            }
            Declaration::McpBatch(batch_import_declaration) => batch_import_declaration
                .prompts
                .iter()
                .find(|prompt_import_declaration| prompt_import_declaration.name == prompt_name),
            Declaration::McpPromptBatch(prompt_batch_import_declaration) => prompt_batch_import_declaration
                .prompts
                .iter()
                .find(|prompt_import_declaration| prompt_import_declaration.name == prompt_name),
            _ => None,
        })
    }

    pub fn resource_imports(&self) -> impl Iterator<Item = &McpResourceImportDeclaration> {
        self.declarations.iter().flat_map(|declaration| match declaration {
            Declaration::McpResource(resource_import_declaration) => std::slice::from_ref(resource_import_declaration).iter(),
            Declaration::McpBatch(batch_import_declaration) => batch_import_declaration.resources.iter(),
            Declaration::McpResourceBatch(resource_batch_import_declaration) => resource_batch_import_declaration.resources.iter(),
            _ => [].iter(),
        })
    }

    pub fn prompt_imports(&self) -> impl Iterator<Item = &McpPromptImportDeclaration> {
        self.declarations.iter().flat_map(|declaration| match declaration {
            Declaration::McpPrompt(prompt_import_declaration) => std::slice::from_ref(prompt_import_declaration).iter(),
            Declaration::McpBatch(batch_import_declaration) => batch_import_declaration.prompts.iter(),
            Declaration::McpPromptBatch(prompt_batch_import_declaration) => prompt_batch_import_declaration.prompts.iter(),
            _ => [].iter(),
        })
    }

    #[must_use]
    pub fn find_agent(&self, agent_name: &str) -> Option<&AgentDeclaration> {
        self.declarations.iter().find_map(|declaration| match declaration {
            Declaration::Agent(agent_declaration) if agent_declaration.name == agent_name => Some(agent_declaration),
            _ => None,
        })
    }

    #[must_use]
    pub fn find_output(&self) -> Option<&OutputDeclaration> {
        self.declarations.iter().find_map(|declaration| match declaration {
            Declaration::Output(output_declaration) => Some(output_declaration),
            _ => None,
        })
    }

    pub fn dynamic_blocks(&self) -> impl Iterator<Item = &DynamicBlock> {
        self.declarations.iter().filter_map(|declaration| match declaration {
            Declaration::Dynamic(dynamic_block) => Some(dynamic_block),
            Declaration::Provider(_)
            | Declaration::Model(_)
            | Declaration::McpServer(_)
            | Declaration::Secrets(_)
            | Declaration::Input(_)
            | Declaration::Schema(_)
            | Declaration::Tool(_)
            | Declaration::McpBatch(_)
            | Declaration::McpToolBatch(_)
            | Declaration::McpResourceBatch(_)
            | Declaration::McpPromptBatch(_)
            | Declaration::McpResource(_)
            | Declaration::McpPrompt(_)
            | Declaration::Agent(_)
            | Declaration::Output(_) => None,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Declaration {
    Provider(ProviderDeclaration),
    Model(ModelDeclaration),
    McpServer(McpServerDeclaration),
    Secrets(SecretsDeclaration),
    Input(InputDeclaration),
    Schema(SchemaDeclaration),
    Tool(ToolDeclaration),
    McpBatch(McpBatchImportDeclaration),
    McpToolBatch(McpToolBatchImportDeclaration),
    McpResourceBatch(McpResourceBatchImportDeclaration),
    McpPromptBatch(McpPromptBatchImportDeclaration),
    McpResource(McpResourceImportDeclaration),
    McpPrompt(McpPromptImportDeclaration),
    Dynamic(DynamicBlock),
    Agent(AgentDeclaration),
    Output(OutputDeclaration),
}

impl Declaration {
    #[must_use]
    pub fn tool_declarations(&self) -> ToolDeclarationIter<'_> {
        match self {
            Self::Tool(tool_declaration) => ToolDeclarationIter::Single(Some(tool_declaration)),
            Self::McpBatch(batch_import_declaration) => ToolDeclarationIter::Batch(batch_import_declaration.tools.iter()),
            Self::McpToolBatch(tool_batch_import_declaration) => ToolDeclarationIter::Batch(tool_batch_import_declaration.tools.iter()),
            Self::Provider(_)
            | Self::Model(_)
            | Self::McpServer(_)
            | Self::Secrets(_)
            | Self::Input(_)
            | Self::Schema(_)
            | Self::McpResourceBatch(_)
            | Self::McpPromptBatch(_)
            | Self::McpResource(_)
            | Self::McpPrompt(_)
            | Self::Dynamic(_)
            | Self::Agent(_)
            | Self::Output(_) => ToolDeclarationIter::Empty,
        }
    }
}

pub enum ToolDeclarationIter<'declaration> {
    Empty,
    Single(Option<&'declaration ToolDeclaration>),
    Batch(std::slice::Iter<'declaration, ToolDeclaration>),
}

impl<'declaration> Iterator for ToolDeclarationIter<'declaration> {
    type Item = &'declaration ToolDeclaration;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Empty => None,
            Self::Single(tool_declaration) => tool_declaration.take(),
            Self::Batch(tool_declarations) => tool_declarations.next(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DeclarationKeyword {
    Provider,
    Model,
    Mcp,
    Secrets,
    Input,
    Schema,
    Tool,
    Resource,
    Prompt,
    Dynamic,
    Agent,
    Output,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ForClauseKeyword {
    For,
    In,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ImportKeyword {
    From,
    As,
}

impl ForClauseKeyword {
    #[must_use]
    pub fn from_identifier(identifier: &str) -> Option<Self> {
        match identifier {
            "for" => Some(Self::For),
            "in" => Some(Self::In),
            _ => None,
        }
    }

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::For => "for",
            Self::In => "in",
        }
    }
}

impl ImportKeyword {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::From => "from",
            Self::As => "as",
        }
    }
}

impl DeclarationKeyword {
    #[must_use]
    pub fn from_identifier(identifier: &str) -> Option<Self> {
        match identifier {
            "provider" => Some(Self::Provider),
            "model" => Some(Self::Model),
            "mcp" => Some(Self::Mcp),
            "secrets" => Some(Self::Secrets),
            "input" => Some(Self::Input),
            "schema" => Some(Self::Schema),
            "tool" => Some(Self::Tool),
            "resource" => Some(Self::Resource),
            "prompt" => Some(Self::Prompt),
            "dynamic" => Some(Self::Dynamic),
            "agent" => Some(Self::Agent),
            "output" => Some(Self::Output),
            _ => None,
        }
    }

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Provider => "provider",
            Self::Model => "model",
            Self::Mcp => "mcp",
            Self::Secrets => "secrets",
            Self::Input => "input",
            Self::Schema => "schema",
            Self::Tool => "tool",
            Self::Resource => "resource",
            Self::Prompt => "prompt",
            Self::Dynamic => "dynamic",
            Self::Agent => "agent",
            Self::Output => "output",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderDeclaration {
    pub name: String,
    pub driver_name: String,
    pub properties: Vec<ObjectField>,
    pub span: SourceSpan,
}

impl ProviderDeclaration {
    #[must_use]
    pub fn property(&self, property_name: ModelDeclarationPropertyName) -> Option<&ObjectField> {
        self.properties
            .iter()
            .find(|property| ModelDeclarationPropertyName::from_identifier(property.name.as_str()) == Some(property_name))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelDeclaration {
    pub name: String,
    pub provider_name: String,
    pub properties: Vec<ObjectField>,
    pub span: SourceSpan,
}

impl ModelDeclaration {
    #[must_use]
    pub fn property(&self, property_name: ModelDeclarationPropertyName) -> Option<&ObjectField> {
        self.properties
            .iter()
            .find(|property| ModelDeclarationPropertyName::from_identifier(property.name.as_str()) == Some(property_name))
    }

    #[must_use]
    pub fn id_expression(&self) -> Option<&Expression> {
        self.property(ModelDeclarationPropertyName::Id).map(|property| &property.value)
    }

    #[must_use]
    pub fn id_literal(&self) -> Option<&str> {
        let Expression::StringLiteral(model_identifier) = self.id_expression()? else {
            return None;
        };

        Some(model_identifier)
    }

    #[must_use]
    pub fn inference_fields(&self) -> Option<&[ObjectField]> {
        let property = self.property(ModelDeclarationPropertyName::Inference)?;
        let Expression::ObjectLiteral(fields) = &property.value else {
            return None;
        };

        Some(fields.as_slice())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ModelDeclarationPropertyName {
    Id,
    Inference,
}

impl ModelDeclarationPropertyName {
    #[must_use]
    pub fn all() -> [Self; 2] {
        [Self::Id, Self::Inference]
    }

    #[must_use]
    pub fn from_identifier(identifier: &str) -> Option<Self> {
        Self::all().into_iter().find(|property_name| property_name.as_str() == identifier)
    }

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Id => "id",
            Self::Inference => "inference",
        }
    }

    #[must_use]
    pub fn definition(self) -> DslPropertyDefinition {
        match self {
            Self::Id => structure::Model::new().id.definition(),
            Self::Inference => structure::Model::new()
                .inference
                .expect("model structure should include inference")
                .definition(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ModelUsagePropertyName {
    Inference,
}

impl ModelUsagePropertyName {
    #[must_use]
    pub fn all() -> [Self; 1] {
        [Self::Inference]
    }

    #[must_use]
    pub fn from_identifier(identifier: &str) -> Option<Self> {
        Self::all().into_iter().find(|property_name| property_name.as_str() == identifier)
    }

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Inference => "inference",
        }
    }

    #[must_use]
    pub fn definition(self) -> DslPropertyDefinition {
        match self {
            Self::Inference => structure::ModelUsage::new()
                .inference
                .expect("model usage structure should include inference")
                .definition(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpServerDeclaration {
    pub name: String,
    pub properties: Vec<ObjectField>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum McpServerPropertyName {
    Endpoint,
    Headers,
}

impl McpServerPropertyName {
    #[must_use]
    pub fn all() -> [Self; 2] {
        [Self::Endpoint, Self::Headers]
    }

    #[must_use]
    pub fn from_identifier(identifier: &str) -> Option<Self> {
        Self::all()
            .into_iter()
            .find(|mcp_server_property_name| mcp_server_property_name.as_str() == identifier)
    }

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Endpoint => "endpoint",
            Self::Headers => "headers",
        }
    }

    #[must_use]
    pub fn definition(self) -> DslPropertyDefinition {
        match self {
            Self::Endpoint => structure::McpServer::new().endpoint.definition(),
            Self::Headers => structure::McpServer::new()
                .headers
                .expect("mcp server structure should include headers")
                .definition(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecretsDeclaration {
    pub fields: Vec<TypedField>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputDeclaration {
    pub fields: Vec<TypedField>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaDeclaration {
    pub name: String,
    pub fields: Vec<TypedField>,
    pub root_variant: Option<TypeExpression>,
    pub span: SourceSpan,
}

impl SchemaDeclaration {
    #[must_use]
    pub fn type_expression(&self) -> TypeExpression {
        self.root_variant
            .clone()
            .unwrap_or_else(|| TypeExpression::Object(self.fields.clone()))
    }

    fn sample_json_value(&self, workflow: &Workflow) -> Value {
        self.type_expression().sample_json_value(workflow)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolDeclaration {
    pub name: String,
    pub description: Option<String>,
    pub max_calls: Option<u64>,
    pub source: Option<ToolSource>,
    pub imported: bool,
    pub input_fields: Vec<TypedField>,
    pub binding_fields: Vec<TypedField>,
    pub fixed_binding_fields: Vec<ObjectField>,
    pub output_fields: Vec<TypedField>,
    pub span: SourceSpan,
}

impl ToolDeclaration {
    #[must_use]
    pub fn has_untyped_mcp_output(&self) -> bool {
        self.output_fields.is_empty() && matches!(self.source, Some(ToolSource::Mcp(_)))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpBatchImportDeclaration {
    pub server_name: String,
    pub fixed_binding_fields: Vec<ObjectField>,
    pub input_fields: Vec<TypedField>,
    pub max_calls: Option<u64>,
    pub output_fields: Vec<TypedField>,
    pub tool_items: Vec<McpToolBatchImportItem>,
    pub resource_items: Vec<McpResourceBatchImportItem>,
    pub prompt_items: Vec<McpPromptBatchImportItem>,
    pub tools: Vec<ToolDeclaration>,
    pub resources: Vec<McpResourceImportDeclaration>,
    pub prompts: Vec<McpPromptImportDeclaration>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpToolBatchImportDeclaration {
    pub server_name: String,
    pub fixed_binding_fields: Vec<ObjectField>,
    pub input_fields: Vec<TypedField>,
    pub max_calls: Option<u64>,
    pub output_fields: Vec<TypedField>,
    pub items: Vec<McpToolBatchImportItem>,
    pub tools: Vec<ToolDeclaration>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpResourceBatchImportDeclaration {
    pub server_name: String,
    pub parameters: Vec<ObjectField>,
    pub items: Vec<McpResourceBatchImportItem>,
    pub resources: Vec<McpResourceImportDeclaration>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpResourceBatchImportItem {
    pub source_name: String,
    pub local_name: String,
    pub alias: Option<String>,
    pub parameters: Vec<ObjectField>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpPromptBatchImportDeclaration {
    pub server_name: String,
    pub parameters: Vec<ObjectField>,
    pub items: Vec<McpPromptBatchImportItem>,
    pub prompts: Vec<McpPromptImportDeclaration>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpPromptBatchImportItem {
    pub source_name: String,
    pub local_name: String,
    pub alias: Option<String>,
    pub parameters: Vec<ObjectField>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpToolBatchImportItem {
    pub source_name: String,
    pub local_name: String,
    pub alias: Option<String>,
    pub input_fields: Vec<TypedField>,
    pub max_calls: Option<u64>,
    pub fixed_binding_fields: Vec<ObjectField>,
    pub output_fields: Vec<TypedField>,
    pub span: SourceSpan,
}

impl McpToolBatchImportItem {
    #[must_use]
    pub fn new(
        source_name: String,
        local_name: Option<String>,
        input_fields: Vec<TypedField>,
        max_calls: Option<u64>,
        fixed_binding_fields: Vec<ObjectField>,
        output_fields: Vec<TypedField>,
        span: SourceSpan,
    ) -> Self {
        let alias = local_name;
        let local_name = alias.clone().unwrap_or_else(|| source_name.replace('-', "_"));

        Self {
            source_name,
            local_name,
            alias,
            input_fields,
            max_calls,
            fixed_binding_fields,
            output_fields,
            span,
        }
    }

    #[must_use]
    pub fn to_tool_declaration(
        &self,
        server_name: &str,
        input_fields: &[TypedField],
        fixed_binding_fields: &[ObjectField],
        max_calls: Option<u64>,
        output_fields: &[TypedField],
    ) -> ToolDeclaration {
        let fixed_binding_fields = ObjectField::merged_with_overrides(fixed_binding_fields, &self.fixed_binding_fields);
        let input_fields = if self.input_fields.is_empty() {
            input_fields.to_vec()
        } else {
            self.input_fields.clone()
        };
        let output_fields = if self.output_fields.is_empty() {
            output_fields.to_vec()
        } else {
            self.output_fields.clone()
        };

        ToolDeclaration {
            name: self.local_name.clone(),
            description: None,
            max_calls: self.max_calls.or(max_calls),
            source: Some(ToolSource::Mcp(McpToolSource {
                server_name: Some(server_name.to_string()),
                tool_name: self.source_name.clone(),
                span: self.span,
            })),
            imported: true,
            input_fields,
            binding_fields: Vec::new(),
            fixed_binding_fields,
            output_fields,
            span: self.span,
        }
    }
}

impl McpResourceBatchImportItem {
    #[must_use]
    pub fn new(source_name: String, local_name: Option<String>, parameters: Vec<ObjectField>, span: SourceSpan) -> Self {
        let alias = local_name;
        let local_name = alias.clone().unwrap_or_else(|| source_name.replace('-', "_"));

        Self {
            source_name,
            local_name,
            alias,
            parameters,
            span,
        }
    }

    #[must_use]
    pub fn to_resource_import_declaration(&self, server_name: &str, shared_parameters: &[ObjectField]) -> McpResourceImportDeclaration {
        let parameters = ObjectField::merged_with_overrides(shared_parameters, &self.parameters);

        McpResourceImportDeclaration {
            name: self.local_name.clone(),
            source: McpImportSource {
                server_name: server_name.to_string(),
                kind: McpImportKind::Resource,
                item_name: self.source_name.clone(),
                span: self.span,
            },
            parameters,
            span: self.span,
        }
    }
}

impl McpPromptBatchImportItem {
    #[must_use]
    pub fn new(source_name: String, local_name: Option<String>, parameters: Vec<ObjectField>, span: SourceSpan) -> Self {
        let alias = local_name;
        let local_name = alias.clone().unwrap_or_else(|| source_name.replace('-', "_"));

        Self {
            source_name,
            local_name,
            alias,
            parameters,
            span,
        }
    }

    #[must_use]
    pub fn to_prompt_import_declaration(&self, server_name: &str, shared_parameters: &[ObjectField]) -> McpPromptImportDeclaration {
        let parameters = ObjectField::merged_with_overrides(shared_parameters, &self.parameters);

        McpPromptImportDeclaration {
            name: self.local_name.clone(),
            source: McpImportSource {
                server_name: server_name.to_string(),
                kind: McpImportKind::Prompt,
                item_name: self.source_name.clone(),
                span: self.span,
            },
            parameters,
            span: self.span,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolSource {
    Mcp(McpToolSource),
}

impl ToolSource {
    #[must_use]
    pub fn mcp_tool_name(&self) -> Option<&str> {
        match self {
            Self::Mcp(mcp_tool_source) => Some(mcp_tool_source.tool_name.as_str()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpToolSource {
    pub server_name: Option<String>,
    pub tool_name: String,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpResourceImportDeclaration {
    pub name: String,
    pub source: McpImportSource,
    pub parameters: Vec<ObjectField>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpPromptImportDeclaration {
    pub name: String,
    pub source: McpImportSource,
    pub parameters: Vec<ObjectField>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpImportSource {
    pub server_name: String,
    pub kind: McpImportKind,
    pub item_name: String,
    pub span: SourceSpan,
}

impl McpImportSource {
    #[must_use]
    pub fn inferred_local_name(&self) -> String {
        self.item_name.replace('-', "_")
    }

    #[must_use]
    pub fn wire_item_name(&self) -> String {
        match self.kind {
            McpImportKind::Tool => self.item_name.replace('-', "_"),
            McpImportKind::Resource | McpImportKind::Prompt => self.item_name.clone(),
        }
    }

    #[must_use]
    pub fn render_path(&self) -> String {
        format!("mcp.{}.{}.{}", self.server_name, self.kind.as_str(), self.wire_item_name())
    }

    #[must_use]
    pub fn as_tool_source(&self) -> McpToolSource {
        McpToolSource {
            server_name: Some(self.server_name.clone()),
            tool_name: self.item_name.clone(),
            span: self.span,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum McpImportKind {
    Tool,
    Resource,
    Prompt,
}

impl McpImportKind {
    #[must_use]
    pub fn from_identifier(identifier: &str) -> Option<Self> {
        match identifier {
            "tool" => Some(Self::Tool),
            "resource" => Some(Self::Resource),
            "prompt" => Some(Self::Prompt),
            _ => None,
        }
    }

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Tool => "tool",
            Self::Resource => "resource",
            Self::Prompt => "prompt",
        }
    }

    #[must_use]
    pub fn wire_tool_name_is_snake_case(tool_name: &str) -> bool {
        let mut characters = tool_name.chars();

        let Some(first_character) = characters.next() else {
            return false;
        };

        if !(first_character.is_ascii_lowercase() || first_character == '_') {
            return false;
        }

        characters.all(|character| character.is_ascii_lowercase() || character.is_ascii_digit() || character == '_')
    }

    #[must_use]
    pub fn wire_item_name_is_snake_case(self, item_name: &str) -> bool {
        let _ = self;

        Self::wire_tool_name_is_snake_case(item_name)
    }

    #[must_use]
    pub fn normalize_tool_name_from_wire(self, wire_name: &str) -> String {
        wire_name.to_string()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ToolPropertyName {
    Description,
    MaxCalls,
    Input,
    Bindings,
    Output,
}

impl ToolPropertyName {
    #[must_use]
    pub fn all() -> [Self; 5] {
        [Self::Description, Self::MaxCalls, Self::Input, Self::Bindings, Self::Output]
    }

    #[must_use]
    pub fn from_identifier(identifier: &str) -> Option<Self> {
        Self::all()
            .into_iter()
            .find(|tool_property_name| tool_property_name.as_str() == identifier)
    }

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Description => "description",
            Self::MaxCalls => "max_calls",
            Self::Input => "input",
            Self::Bindings => "bindings",
            Self::Output => "output",
        }
    }

    #[must_use]
    pub fn definition(self) -> DslPropertyDefinition {
        match self {
            Self::Description => structure::Tool::new()
                .description
                .expect("tool structure should include description")
                .definition(),
            Self::MaxCalls => structure::Tool::new()
                .max_calls
                .expect("tool structure should include max_calls")
                .definition(),
            Self::Input => structure::Tool::new()
                .input
                .expect("tool structure should include input")
                .definition(),
            Self::Bindings => structure::Tool::new()
                .bindings
                .expect("tool structure should include bindings")
                .definition(),
            Self::Output => structure::Tool::new()
                .output
                .expect("tool structure should include output")
                .definition(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum McpImportPropertyName {
    Bindings,
}

impl McpImportPropertyName {
    #[must_use]
    pub fn all() -> [Self; 1] {
        [Self::Bindings]
    }

    #[must_use]
    pub fn from_identifier(identifier: &str) -> Option<Self> {
        Self::all().into_iter().find(|property_name| property_name.as_str() == identifier)
    }

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Bindings => "bindings",
        }
    }

    #[must_use]
    pub fn definition(self) -> DslPropertyDefinition {
        match self {
            Self::Bindings => structure::McpImport::new()
                .bindings
                .expect("mcp import structure should include bindings")
                .definition(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ToolCallPropertyName {
    Input,
    Bindings,
    MaxCalls,
}

impl ToolCallPropertyName {
    #[must_use]
    pub fn all() -> [Self; 3] {
        [Self::Input, Self::Bindings, Self::MaxCalls]
    }

    #[must_use]
    pub fn from_identifier(identifier: &str) -> Option<Self> {
        Self::all().into_iter().find(|property_name| property_name.as_str() == identifier)
    }

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Input => "input",
            Self::Bindings => "bindings",
            Self::MaxCalls => "max_calls",
        }
    }

    #[must_use]
    pub fn definition(self) -> DslPropertyDefinition {
        match self {
            Self::Input => structure::ToolCall::new()
                .input
                .expect("tool call structure should include input")
                .definition(),
            Self::Bindings => structure::ToolCall::new()
                .bindings
                .expect("tool call structure should include bindings")
                .definition(),
            Self::MaxCalls => structure::ToolCall::new()
                .max_calls
                .expect("tool call structure should include max_calls")
                .definition(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum McpToolBatchImportPropertyName {
    Bindings,
    MaxCalls,
}

impl McpToolBatchImportPropertyName {
    #[must_use]
    pub fn all() -> [Self; 2] {
        [Self::Bindings, Self::MaxCalls]
    }

    #[must_use]
    pub fn from_identifier(identifier: &str) -> Option<Self> {
        Self::all().into_iter().find(|property_name| property_name.as_str() == identifier)
    }

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Bindings => ToolPropertyName::Bindings.as_str(),
            Self::MaxCalls => ToolPropertyName::MaxCalls.as_str(),
        }
    }

    #[must_use]
    pub fn definition(self) -> DslPropertyDefinition {
        match self {
            Self::Bindings => structure::McpToolBatchImport::new()
                .bindings
                .expect("mcp tool batch import structure should include bindings")
                .definition(),
            Self::MaxCalls => structure::McpToolBatchImport::new()
                .max_calls
                .expect("mcp tool batch import structure should include max_calls")
                .definition(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DynamicBlock {
    pub fields: Vec<ObjectField>,
    pub span: SourceSpan,
}

impl DynamicBlock {
    #[must_use]
    pub fn field(&self, field_name: &str) -> Option<&ObjectField> {
        self.fields.iter().find(|field| field.name == field_name)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentDeclaration {
    pub name: String,
    pub for_loop: Option<AgentForLoop>,
    pub properties: Vec<AgentProperty>,
    pub span: SourceSpan,
}

impl AgentDeclaration {
    pub fn dynamic_blocks(&self) -> impl Iterator<Item = &DynamicBlock> {
        self.properties.iter().filter_map(|property| match property {
            AgentProperty::Dynamic(dynamic_block) => Some(dynamic_block),
            AgentProperty::Model(_)
            | AgentProperty::InvalidModel(_)
            | AgentProperty::Instruction(_)
            | AgentProperty::Output { fields: _, span: _ }
            | AgentProperty::Context(_)
            | AgentProperty::Uses(_)
            | AgentProperty::Unknown { name: _, span: _ } => None,
        })
    }

    #[must_use]
    pub fn expression_property(&self, property_name: AgentExpressionPropertyName) -> Option<&Expression> {
        for agent_property in &self.properties {
            match agent_property {
                AgentProperty::Instruction(expression) if property_name == AgentExpressionPropertyName::Instruction => {
                    return Some(expression)
                }
                AgentProperty::Context(expression) if property_name == AgentExpressionPropertyName::Context => return Some(expression),
                AgentProperty::Uses(expression) if property_name == AgentExpressionPropertyName::Uses => return Some(expression),
                AgentProperty::Dynamic(_) => {}
                AgentProperty::Model(_)
                | AgentProperty::InvalidModel(_)
                | AgentProperty::Instruction(_)
                | AgentProperty::Output { fields: _, span: _ }
                | AgentProperty::Context(_)
                | AgentProperty::Uses(_)
                | AgentProperty::Unknown { name: _, span: _ } => {}
            }
        }

        None
    }

    #[must_use]
    pub fn model_usage(&self) -> Option<&ModelUsage> {
        for agent_property in &self.properties {
            if let AgentProperty::Model(model_usage) = agent_property {
                return Some(model_usage);
            }
        }

        None
    }

    #[must_use]
    pub fn effective_inference_fields(
        &self,
        provider_declaration: Option<&ProviderDeclaration>,
        model_declaration: &ModelDeclaration,
    ) -> Vec<ObjectField> {
        let mut inference_fields = Vec::new();
        let _ = provider_declaration;

        if let Some(model_inference_fields) = model_declaration.inference_fields() {
            merge_inference_fields(&mut inference_fields, model_inference_fields);
        }

        if let Some(model_usage_inference_fields) = self.model_usage().and_then(ModelUsage::inference_fields) {
            merge_inference_fields(&mut inference_fields, model_usage_inference_fields);
        }

        inference_fields
    }

    pub fn required_expression_property(
        &self,
        property_name: AgentExpressionPropertyName,
    ) -> Result<&Expression, AgentExpressionPropertyName> {
        self.expression_property(property_name).ok_or(property_name)
    }

    #[must_use]
    pub fn output_type(&self) -> Option<TypeExpression> {
        for agent_property in &self.properties {
            if let Some(output_type_expression) = agent_property.output_type_expression() {
                return Some(output_type_expression);
            }
        }

        None
    }

    #[must_use]
    pub fn declared_final_output_type_expression(&self) -> Option<TypeExpression> {
        let output_type_expression = self.output_type()?;

        if self.for_loop.is_some() {
            return Some(TypeExpression::Array {
                item_type: Box::new(output_type_expression),
                fixed_length: None,
            });
        }

        Some(output_type_expression)
    }

    #[must_use]
    pub fn inferred_iteration_output_type_expression(&self) -> TypeExpression {
        self.output_type().unwrap_or(TypeExpression::String)
    }

    #[must_use]
    pub fn inferred_final_output_type_expression(&self) -> TypeExpression {
        let iteration_output_type_expression = self.inferred_iteration_output_type_expression();

        if self.for_loop.is_some() {
            return TypeExpression::Array {
                item_type: Box::new(iteration_output_type_expression),
                fixed_length: None,
            };
        }

        iteration_output_type_expression
    }
}

fn merge_inference_fields(inference_fields: &mut Vec<ObjectField>, override_fields: &[ObjectField]) {
    for override_field in override_fields {
        if let Some(existing_field) = inference_fields
            .iter_mut()
            .find(|inference_field| inference_field.name == override_field.name)
        {
            *existing_field = override_field.clone();

            continue;
        }

        inference_fields.push(override_field.clone());
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentForLoop {
    pub pattern: AgentForLoopPattern,
    pub iterable: Expression,
}

impl AgentForLoop {
    #[must_use]
    pub fn bound_identifier_names(&self) -> Vec<&str> {
        self.pattern.bound_identifier_names()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentForLoopPattern {
    Identifier(String),
    ObjectDestructuring(Vec<String>),
}

impl AgentForLoopPattern {
    #[must_use]
    pub fn bound_identifier_names(&self) -> Vec<&str> {
        match self {
            Self::Identifier(identifier) => vec![identifier.as_str()],
            Self::ObjectDestructuring(field_names) => field_names.iter().map(String::as_str).collect(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentProperty {
    Dynamic(DynamicBlock),
    Model(ModelUsage),
    InvalidModel(Expression),
    Instruction(Expression),
    Output { fields: Vec<TypedField>, span: SourceSpan },
    Context(Expression),
    Uses(Expression),
    Unknown { name: String, span: SourceSpan },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelUsage {
    pub reference: Reference,
    pub properties: Vec<ObjectField>,
    pub span: SourceSpan,
}

impl ModelUsage {
    #[must_use]
    pub fn model_name(&self) -> Option<&str> {
        if self.reference.root_keyword() != Some(ReferenceKeyword::Model) || self.reference.accesses.len() != 1 {
            return None;
        }

        let access = self.reference.first_access()?;

        if access.optional {
            return None;
        }

        Some(access.field.as_str())
    }

    #[must_use]
    pub fn inference_fields(&self) -> Option<&[ObjectField]> {
        for property in &self.properties {
            if ModelUsagePropertyName::from_identifier(property.name.as_str()) != Some(ModelUsagePropertyName::Inference) {
                continue;
            }

            let Expression::ObjectLiteral(inference_fields) = &property.value else {
                return None;
            };

            return Some(inference_fields.as_slice());
        }

        None
    }
}

impl AgentProperty {
    #[must_use]
    pub fn output_type_expression(&self) -> Option<TypeExpression> {
        match self {
            Self::Output { fields, span: _ } => Some(TypeExpression::Object(fields.clone())),
            Self::Dynamic(_)
            | Self::Model(_)
            | Self::InvalidModel(_)
            | Self::Instruction(_)
            | Self::Context(_)
            | Self::Uses(_)
            | Self::Unknown { name: _, span: _ } => None,
        }
    }

    #[must_use]
    pub fn definition(&self) -> Option<DslPropertyDefinition> {
        let agent = structure::Agent::new();

        let property_definition = match self {
            Self::Dynamic(_) => agent.dynamic[0].definition(),
            Self::Model(_) | Self::InvalidModel(_) => agent.model.definition(),
            Self::Instruction(_) => agent.instruction.definition(),
            Self::Output { fields: _, span: _ } => agent.output.expect("agent structure should include output").definition(),
            Self::Context(_) => agent.context.expect("agent structure should include context").definition(),
            Self::Uses(_) => agent.uses[0].definition(),
            Self::Unknown { name: _, span: _ } => return None,
        };

        Some(property_definition)
    }

    #[must_use]
    pub fn name(&self) -> Option<&'static str> {
        self.definition().map(|property_definition| property_definition.name)
    }

    #[must_use]
    pub fn repeatable(&self) -> bool {
        self.definition().is_some_and(|property_definition| property_definition.repeatable)
    }
}

impl AgentExpressionPropertyName {
    #[must_use]
    pub fn from_agent_property_identifier(identifier: &str) -> Option<Self> {
        let agent = structure::Agent::new();

        if agent.property_is_model(identifier) {
            return Some(Self::Model);
        }

        if agent.property_is_instruction(identifier) {
            return Some(Self::Instruction);
        }

        if agent.property_is_context(identifier) {
            return Some(Self::Context);
        }

        if agent.property_is_uses(identifier) {
            return Some(Self::Uses);
        }

        None
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AgentExpressionPropertyName {
    Model,
    Instruction,
    Context,
    Uses,
}

impl AgentExpressionPropertyName {
    #[must_use]
    pub fn all() -> [Self; 4] {
        [Self::Model, Self::Instruction, Self::Context, Self::Uses]
    }

    #[must_use]
    pub fn from_identifier(identifier: &str) -> Option<Self> {
        Self::from_agent_property_identifier(identifier)
    }

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Model => "model",
            Self::Instruction => "instruction",
            Self::Context => "context",
            Self::Uses => "uses",
        }
    }

    #[must_use]
    pub fn definition(self) -> DslPropertyDefinition {
        let agent = structure::Agent::new();

        match self {
            Self::Model => agent.model.definition(),
            Self::Instruction => agent.instruction.definition(),
            Self::Context => agent.context.expect("agent structure should include context").definition(),
            Self::Uses => agent.uses[0].definition(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputDeclaration {
    pub fields: Vec<ObjectField>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypedField {
    pub name: String,
    pub field_type: TypeExpression,
    pub description: Option<String>,
    pub span: SourceSpan,
}

impl TypedField {
    fn insert_sample_json_value(&self, workflow: &Workflow, object_values: &mut Map<String, Value>) {
        object_values.insert(self.name.clone(), self.field_type.sample_json_value(workflow));
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeExpression {
    String,
    Number,
    Float,
    Boolean,
    Null,
    AnyObject,
    SchemaReference(String),
    StringEnum(String),
    StringEnumReference(Reference),
    Array {
        item_type: Box<TypeExpression>,
        fixed_length: Option<u64>,
    },
    Tuple(Vec<TypeExpression>),
    Object(Vec<TypedField>),
    Variant {
        discriminator: String,
        cases: Vec<VariantCase>,
    },
    Union(Vec<TypeExpression>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VariantCase {
    pub name: String,
    pub fields: Vec<TypedField>,
    pub span: SourceSpan,
}

impl TypeExpression {
    #[must_use]
    pub fn sample_json_value(&self, workflow: &Workflow) -> Value {
        match self {
            Self::String | Self::StringEnum(_) | Self::StringEnumReference(_) => Value::String(String::new()),
            Self::Number => Value::Number(0.into()),
            Self::Float => Value::Number(serde_json::Number::from(0)),
            Self::Boolean => Value::Bool(false),
            Self::Null => Value::Null,
            Self::AnyObject => Value::Object(Map::new()),
            Self::SchemaReference(schema_name) => workflow.find_schema(schema_name).map_or_else(
                || Value::Object(Map::new()),
                |schema_declaration| schema_declaration.sample_json_value(workflow),
            ),
            Self::Array {
                item_type: _,
                fixed_length: _,
            }
            | Self::Tuple(_) => Value::Array(Vec::new()),
            Self::Object(typed_fields) => {
                let mut object_values = Map::new();

                for typed_field in typed_fields {
                    typed_field.insert_sample_json_value(workflow, &mut object_values);
                }

                Value::Object(object_values)
            }
            Self::Variant { discriminator, cases } => {
                let Some(first_case) = cases.first() else {
                    return Value::Object(Map::new());
                };

                let mut object_values = Map::new();
                object_values.insert(discriminator.clone(), Value::String(first_case.name.clone()));

                for typed_field in &first_case.fields {
                    typed_field.insert_sample_json_value(workflow, &mut object_values);
                }

                Value::Object(object_values)
            }
            Self::Union(type_expressions) => {
                if let Some(non_null_type_expression) = type_expressions
                    .iter()
                    .find(|candidate_type_expression| !matches!(candidate_type_expression, Self::Null))
                {
                    return non_null_type_expression.sample_json_value(workflow);
                }

                Value::Null
            }
        }
    }

    #[must_use]
    pub fn can_be_null(&self) -> bool {
        match self {
            Self::Null => true,
            Self::Union(type_expressions) => type_expressions.iter().any(Self::can_be_null),
            Self::String
            | Self::Number
            | Self::Float
            | Self::Boolean
            | Self::AnyObject
            | Self::SchemaReference(_)
            | Self::StringEnum(_)
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
            } => false,
        }
    }

    #[must_use]
    pub fn nullable(inner_type: Self) -> Self {
        match inner_type {
            Self::Union(mut type_expressions) => {
                type_expressions.push(Self::Null);

                Self::Union(type_expressions)
            }
            _ => Self::Union(vec![inner_type, Self::Null]),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expression {
    StringLiteral(String),
    StringTemplate(StringTemplate),
    NumberLiteral(String),
    BooleanLiteral(bool),
    NullLiteral,
    Reference(Reference),
    FunctionCall(FunctionCall),
    ToolCall(ToolCall),
    McpCall(McpCall),
    NullFallback(NullFallbackExpression),
    VariantProjection(VariantProjectionExpression),
    Match(MatchExpression),
    ArrayLiteral(Vec<Expression>),
    ObjectLiteral(Vec<ObjectField>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NullFallbackExpression {
    pub value: Box<Expression>,
    pub fallback: Box<Expression>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VariantProjectionExpression {
    pub value: Reference,
    pub case_name: String,
    pub field_path: Vec<String>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchExpression {
    pub value: Box<Expression>,
    pub branches: Vec<MatchBranch>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MatchBranch {
    Variant {
        case_name: String,
        field_path: Vec<String>,
        span: SourceSpan,
    },
    Fallback {
        value: Expression,
        span: SourceSpan,
    },
}

impl Expression {
    #[must_use]
    pub(crate) fn referenced_names_for_keyword(&self, reference_keyword: ReferenceKeyword) -> Vec<String> {
        let Self::ArrayLiteral(expressions) = self else {
            return Vec::new();
        };

        expressions
            .iter()
            .filter_map(|expression| expression.direct_name_for_keyword(reference_keyword))
            .collect()
    }

    #[must_use]
    pub(crate) fn direct_name_for_keyword(&self, reference_keyword: ReferenceKeyword) -> Option<String> {
        match self {
            Self::Reference(reference) => reference.direct_name_for_keyword(reference_keyword),
            Self::FunctionCall(function_call) => function_call.direct_name_for_keyword(reference_keyword),
            Self::ToolCall(tool_call) => tool_call.callee.direct_name_for_keyword(reference_keyword),
            Self::McpCall(_) => None,
            Self::NullFallback(_)
            | Self::VariantProjection(_)
            | Self::Match(_)
            | Self::StringLiteral(_)
            | Self::StringTemplate(_)
            | Self::NumberLiteral(_)
            | Self::BooleanLiteral(_)
            | Self::NullLiteral
            | Self::ArrayLiteral(_)
            | Self::ObjectLiteral(_) => None,
        }
    }

    #[must_use]
    pub(crate) fn agent_tool_binding_fields(&self) -> &[ObjectField] {
        match self {
            Self::ToolCall(tool_call) => tool_call.agent_tool_binding_fields(),
            Self::Reference(_)
            | Self::FunctionCall(_)
            | Self::McpCall(_)
            | Self::NullFallback(_)
            | Self::VariantProjection(_)
            | Self::Match(_)
            | Self::StringLiteral(_)
            | Self::StringTemplate(_)
            | Self::NumberLiteral(_)
            | Self::BooleanLiteral(_)
            | Self::NullLiteral
            | Self::ArrayLiteral(_)
            | Self::ObjectLiteral(_) => &[],
        }
    }

    #[must_use]
    pub fn tool_calls(&self) -> Vec<&ToolCall> {
        let mut tool_calls = Vec::new();
        self.collect_tool_calls(&mut tool_calls);

        tool_calls
    }

    fn collect_tool_calls<'expression>(&'expression self, tool_calls: &mut Vec<&'expression ToolCall>) {
        match self {
            Self::ToolCall(tool_call) => {
                tool_calls.push(tool_call);

                for input_field in &tool_call.input_fields {
                    input_field.value.collect_tool_calls(tool_calls);
                }

                for binding_field in &tool_call.binding_fields {
                    binding_field.value.collect_tool_calls(tool_calls);
                }
            }
            Self::StringTemplate(string_template) => {
                for string_template_part in &string_template.parts {
                    if let StringTemplatePart::Interpolation(interpolation_expression) = string_template_part {
                        interpolation_expression.collect_tool_calls(tool_calls);
                    }
                }
            }
            Self::FunctionCall(function_call) => {
                for call_argument in &function_call.arguments {
                    call_argument.expression().collect_tool_calls(tool_calls);
                }
            }
            Self::McpCall(mcp_call) => {
                for parameter_field in &mcp_call.parameter_fields {
                    parameter_field.value.collect_tool_calls(tool_calls);
                }
            }
            Self::NullFallback(null_fallback) => {
                null_fallback.value.collect_tool_calls(tool_calls);
                null_fallback.fallback.collect_tool_calls(tool_calls);
            }
            Self::Match(match_expression) => {
                match_expression.value.collect_tool_calls(tool_calls);

                for match_branch in &match_expression.branches {
                    if let MatchBranch::Fallback { value, .. } = match_branch {
                        value.collect_tool_calls(tool_calls);
                    }
                }
            }
            Self::ArrayLiteral(item_expressions) => {
                for item_expression in item_expressions {
                    item_expression.collect_tool_calls(tool_calls);
                }
            }
            Self::ObjectLiteral(object_fields) => {
                for object_field in object_fields {
                    object_field.value.collect_tool_calls(tool_calls);
                }
            }
            Self::NumberLiteral(_)
            | Self::BooleanLiteral(_)
            | Self::NullLiteral
            | Self::StringLiteral(_)
            | Self::Reference(_)
            | Self::VariantProjection(_) => {}
        }
    }

    #[must_use]
    pub fn to_type_expression(&self) -> Option<TypeExpression> {
        match self {
            Self::Reference(reference) => reference.to_type_expression(),
            Self::StringLiteral(string_value) => Some(TypeExpression::StringEnum(string_value.clone())),
            Self::ArrayLiteral(item_expressions) => {
                let [item_expression] = item_expressions.as_slice() else {
                    return None;
                };

                Some(TypeExpression::Array {
                    item_type: Box::new(item_expression.to_type_expression()?),
                    fixed_length: None,
                })
            }
            Self::ObjectLiteral(object_fields) => {
                let mut typed_fields = Vec::new();

                for object_field in object_fields {
                    typed_fields.push(TypedField {
                        name: object_field.name.clone(),
                        field_type: object_field.value.to_type_expression()?,
                        description: None,
                        span: object_field.span,
                    });
                }

                Some(TypeExpression::Object(typed_fields))
            }
            Self::StringTemplate(_)
            | Self::NumberLiteral(_)
            | Self::BooleanLiteral(_)
            | Self::NullLiteral
            | Self::FunctionCall(_)
            | Self::ToolCall(_)
            | Self::McpCall(_)
            | Self::NullFallback(_)
            | Self::VariantProjection(_)
            | Self::Match(_) => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolCall {
    pub callee: Reference,
    pub input_fields: Vec<ObjectField>,
    pub binding_fields: Vec<ObjectField>,
    pub max_calls: Option<u64>,
    pub span: SourceSpan,
}

impl ToolCall {
    #[must_use]
    pub(crate) fn agent_tool_binding_fields(&self) -> &[ObjectField] {
        self.binding_fields.as_slice()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpCall {
    pub operation: McpCallOperation,
    pub callee: Reference,
    pub parameter_fields: Vec<ObjectField>,
    pub span: SourceSpan,
}

impl McpCall {
    #[must_use]
    pub fn target_name(&self) -> Option<&str> {
        self.callee.first_access_field()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum McpCallOperation {
    Read,
    Render,
}

impl McpCallOperation {
    #[must_use]
    pub fn from_identifier(identifier: &str) -> Option<Self> {
        match identifier {
            "read" => Some(Self::Read),
            "render" => Some(Self::Render),
            _ => None,
        }
    }

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Render => "render",
        }
    }

    #[must_use]
    pub fn expected_root(self) -> ReferenceKeyword {
        match self {
            Self::Read => ReferenceKeyword::Resource,
            Self::Render => ReferenceKeyword::Prompt,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StringTemplate {
    pub parts: Vec<StringTemplatePart>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StringTemplatePart {
    Text(String),
    Interpolation(Expression),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectField {
    pub name: String,
    pub value: Expression,
    pub span: SourceSpan,
}

impl ObjectField {
    #[must_use]
    pub fn merged_with_overrides(shared_fields: &[Self], local_fields: &[Self]) -> Vec<Self> {
        let mut merged_fields = shared_fields.to_vec();

        for local_field in local_fields {
            if let Some(existing_field_index) = merged_fields
                .iter()
                .position(|existing_field| existing_field.name == local_field.name)
            {
                merged_fields[existing_field_index] = local_field.clone();

                continue;
            }

            merged_fields.push(local_field.clone());
        }

        merged_fields
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionCall {
    pub callee: Reference,
    pub arguments: Vec<CallArgument>,
}

impl FunctionCall {
    #[must_use]
    pub(crate) fn direct_name_for_keyword(&self, reference_keyword: ReferenceKeyword) -> Option<String> {
        self.callee.direct_name_for_keyword(reference_keyword)
    }

    #[must_use]
    pub fn identifier_name(&self) -> Option<&str> {
        self.callee.root.as_identifier()
    }

    #[must_use]
    pub fn builtin_function_name(&self) -> Option<BuiltinFunctionName> {
        self.identifier_name().and_then(BuiltinFunctionName::from_identifier)
    }

    #[must_use]
    pub fn argument_expression(&self, index: usize) -> Option<&Expression> {
        self.arguments.get(index).map(CallArgument::expression)
    }

    #[must_use]
    pub fn first_argument_expression(&self) -> Option<&Expression> {
        self.argument_expression(0)
    }

    #[must_use]
    pub fn named_argument_expression(&self, argument_name: &str) -> Option<&Expression> {
        for call_argument in &self.arguments {
            if call_argument.named_argument_name() == Some(argument_name) {
                return Some(call_argument.expression());
            }
        }

        None
    }

    #[must_use]
    pub fn builtin_named_argument_expression(&self, argument_name: BuiltinFunctionArgumentName) -> Option<&Expression> {
        self.named_argument_expression(argument_name.as_str())
    }

    #[must_use]
    pub fn model_named_argument_expression(&self, argument_name: ModelCallArgumentName) -> Option<&Expression> {
        self.named_argument_expression(argument_name.as_str())
    }

    #[must_use]
    pub fn model_argument_expressions(&self) -> Vec<&Expression> {
        let mut model_argument_expressions = Vec::new();

        for call_argument in &self.arguments {
            if call_argument.named_argument_name().is_none() {
                model_argument_expressions.push(call_argument.expression());

                continue;
            }

            if call_argument.named_argument_name() == Some(ModelCallArgumentName::Model.as_str()) {
                model_argument_expressions.push(call_argument.expression());
            }
        }

        model_argument_expressions
    }

    #[must_use]
    pub fn agent_argument_expression(&self) -> Option<&Expression> {
        for call_argument in &self.arguments {
            if call_argument.named_argument_name().is_none() {
                return Some(call_argument.expression());
            }

            if call_argument.named_argument_name() == Some(BuiltinFunctionArgumentName::Agent.as_str()) {
                return Some(call_argument.expression());
            }
        }

        None
    }
}

impl Expression {
    pub(crate) fn collect_dynamic_dependencies(&self, referenced_dynamic_fields: &mut std::collections::HashSet<String>) {
        match self {
            Self::Reference(reference) => {
                reference.collect_dynamic_dependency(referenced_dynamic_fields);
            }
            Self::FunctionCall(function_call) => {
                function_call.callee.collect_dynamic_dependency(referenced_dynamic_fields);

                for call_argument in &function_call.arguments {
                    call_argument.expression().collect_dynamic_dependencies(referenced_dynamic_fields);
                }
            }
            Self::ToolCall(tool_call) => {
                tool_call.callee.collect_dynamic_dependency(referenced_dynamic_fields);

                for object_field in &tool_call.input_fields {
                    object_field.value.collect_dynamic_dependencies(referenced_dynamic_fields);
                }

                for object_field in &tool_call.binding_fields {
                    object_field.value.collect_dynamic_dependencies(referenced_dynamic_fields);
                }
            }
            Self::McpCall(mcp_call) => {
                mcp_call.callee.collect_dynamic_dependency(referenced_dynamic_fields);

                for object_field in &mcp_call.parameter_fields {
                    object_field.value.collect_dynamic_dependencies(referenced_dynamic_fields);
                }
            }
            Self::NullFallback(null_fallback) => {
                null_fallback.value.collect_dynamic_dependencies(referenced_dynamic_fields);
                null_fallback.fallback.collect_dynamic_dependencies(referenced_dynamic_fields);
            }
            Self::VariantProjection(variant_projection) => {
                variant_projection.value.collect_dynamic_dependency(referenced_dynamic_fields);
            }
            Self::Match(match_expression) => {
                match_expression.value.collect_dynamic_dependencies(referenced_dynamic_fields);

                for branch in &match_expression.branches {
                    if let MatchBranch::Fallback { value, span: _ } = branch {
                        value.collect_dynamic_dependencies(referenced_dynamic_fields);
                    }
                }
            }
            Self::ArrayLiteral(array_values) => {
                for array_value in array_values {
                    array_value.collect_dynamic_dependencies(referenced_dynamic_fields);
                }
            }
            Self::ObjectLiteral(object_fields) => {
                for object_field in object_fields {
                    object_field.value.collect_dynamic_dependencies(referenced_dynamic_fields);
                }
            }
            Self::StringTemplate(string_template) => {
                for string_template_part in &string_template.parts {
                    if let StringTemplatePart::Interpolation(interpolation_expression) = string_template_part {
                        interpolation_expression.collect_dynamic_dependencies(referenced_dynamic_fields);
                    }
                }
            }
            Self::StringLiteral(_) | Self::NumberLiteral(_) | Self::BooleanLiteral(_) | Self::NullLiteral => {}
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CallArgument {
    Positional(Expression),
    Named(NamedArgument),
}

impl CallArgument {
    #[must_use]
    pub fn expression(&self) -> &Expression {
        match self {
            Self::Positional(expression) => expression,
            Self::Named(named_argument) => &named_argument.value,
        }
    }

    #[must_use]
    pub fn named_argument_name(&self) -> Option<&str> {
        match self {
            Self::Positional(_) => None,
            Self::Named(named_argument) => Some(named_argument.name.as_str()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NamedArgument {
    pub name: String,
    pub value: Expression,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reference {
    pub root: ReferenceRoot,
    pub accesses: Vec<ReferenceAccess>,
    pub span: SourceSpan,
}

impl Reference {
    #[must_use]
    pub fn to_type_expression(&self) -> Option<TypeExpression> {
        if self.accesses.is_empty() {
            let identifier = self.root.as_identifier()?;

            return match identifier {
                "string" => Some(TypeExpression::String),
                "number" => Some(TypeExpression::Number),
                "float" => Some(TypeExpression::Float),
                "boolean" => Some(TypeExpression::Boolean),
                "object" => Some(TypeExpression::AnyObject),
                "null" => Some(TypeExpression::Null),
                _ => Some(TypeExpression::StringEnumReference(self.clone())),
            };
        }

        if let Some((schema_name, field_path)) = self.schema_name_and_field_path() {
            if field_path.is_empty() {
                return Some(TypeExpression::SchemaReference(schema_name.to_string()));
            }
        }

        Some(TypeExpression::StringEnumReference(self.clone()))
    }

    #[must_use]
    pub fn root_keyword(&self) -> Option<ReferenceKeyword> {
        self.root.keyword()
    }

    #[must_use]
    pub fn is_keyword_root(&self, reference_keyword: ReferenceKeyword) -> bool {
        self.root_keyword() == Some(reference_keyword)
    }

    #[must_use]
    pub fn is_agent_root(&self) -> bool {
        self.is_keyword_root(ReferenceKeyword::Agent)
    }

    #[must_use]
    pub fn schema_name_and_field_path(&self) -> Option<(&str, Vec<&str>)> {
        if self.root.as_identifier() != Some(DeclarationKeyword::Schema.as_str()) {
            return None;
        }

        let schema_name = self.first_access_field()?;
        let field_path = self
            .accesses
            .iter()
            .skip(1)
            .map(|reference_access| reference_access.field.as_str())
            .collect::<Vec<_>>();

        Some((schema_name, field_path))
    }

    #[must_use]
    pub fn first_access(&self) -> Option<&ReferenceAccess> {
        self.accesses.first()
    }

    #[must_use]
    pub fn first_access_field(&self) -> Option<&str> {
        self.first_access().map(|reference_access| reference_access.field.as_str())
    }

    #[must_use]
    pub(crate) fn direct_name_for_keyword(&self, reference_keyword: ReferenceKeyword) -> Option<String> {
        if self.root_keyword() != Some(reference_keyword) || self.accesses.len() != 1 || self.accesses[0].optional {
            return None;
        }

        Some(self.accesses[0].field.clone())
    }

    #[must_use]
    pub fn tool_name(&self) -> Option<&str> {
        if self.root_keyword() != Some(ReferenceKeyword::Tool) {
            return None;
        }

        self.first_access_field()
    }

    #[must_use]
    pub fn import_name(&self, reference_keyword: ReferenceKeyword) -> Option<&str> {
        if self.root_keyword() != Some(reference_keyword) {
            return None;
        }

        self.first_access_field()
    }

    #[must_use]
    pub fn render_path(&self) -> String {
        let mut rendered_reference = if let Some(reference_root_keyword) = self.root_keyword() {
            reference_root_keyword.as_str().to_owned()
        } else {
            self.root
                .as_identifier()
                .expect("non-keyword reference root should be identifier")
                .to_owned()
        };

        for reference_access in &self.accesses {
            if reference_access.optional {
                rendered_reference.push_str("?.");
                rendered_reference.push_str(reference_access.field.as_str());

                continue;
            }

            rendered_reference.push('.');
            rendered_reference.push_str(reference_access.field.as_str());
        }

        rendered_reference
    }

    pub(crate) fn collect_dynamic_dependency(&self, referenced_dynamic_fields: &mut std::collections::HashSet<String>) {
        if self.root_keyword() != Some(ReferenceKeyword::Dynamic) {
            return;
        }

        let Some(dynamic_field_name) = self.first_access_field() else {
            return;
        };

        referenced_dynamic_fields.insert(dynamic_field_name.to_string());
    }

    pub(crate) fn collect_agent_dependency<HashBuilder: BuildHasher>(&self, referenced_agents: &mut HashSet<String, HashBuilder>) {
        if !self.is_agent_root() {
            return;
        }

        let Some(agent_name) = self.first_access_field() else {
            return;
        };

        referenced_agents.insert(agent_name.to_string());
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ReferenceRoot {
    Keyword(ReferenceKeyword),
    Identifier(String),
}

impl ReferenceRoot {
    #[must_use]
    pub fn from_identifier(identifier: String) -> Self {
        if let Some(keyword) = ReferenceKeyword::from_identifier(identifier.as_str()) {
            Self::Keyword(keyword)
        } else {
            Self::Identifier(identifier)
        }
    }

    #[must_use]
    pub fn as_identifier(&self) -> Option<&str> {
        match self {
            Self::Identifier(identifier) => Some(identifier),
            Self::Keyword(_) => None,
        }
    }

    #[must_use]
    pub fn keyword(&self) -> Option<ReferenceKeyword> {
        match self {
            Self::Keyword(keyword) => Some(*keyword),
            Self::Identifier(_) => None,
        }
    }
}

impl TypeExpression {
    #[must_use]
    pub fn field_type_at_path<'expression>(&'expression self, field_path: &[&str]) -> Option<&'expression TypeExpression> {
        let Some((field_name, remaining_field_path)) = field_path.split_first() else {
            return Some(self);
        };

        match self {
            Self::Object(typed_fields) => {
                let typed_field = typed_fields.iter().find(|typed_field| typed_field.name == *field_name)?;

                typed_field.field_type.field_type_at_path(remaining_field_path)
            }
            Self::Union(type_expressions) => {
                for type_expression in type_expressions {
                    if let Some(field_type) = type_expression.field_type_at_path(field_path) {
                        return Some(field_type);
                    }
                }

                None
            }
            Self::String
            | Self::Number
            | Self::Float
            | Self::Boolean
            | Self::Null
            | Self::AnyObject
            | Self::SchemaReference(_)
            | Self::StringEnum(_)
            | Self::StringEnumReference(_)
            | Self::Array {
                item_type: _,
                fixed_length: _,
            }
            | Self::Tuple(_)
            | Self::Variant {
                discriminator: _,
                cases: _,
            } => None,
        }
    }

    #[must_use]
    pub fn resolved_field_type_at_path<HashBuilder: BuildHasher>(
        &self,
        field_path: &[&str],
        named_schemas: &HashMap<String, TypeExpression, HashBuilder>,
    ) -> Option<TypeExpression> {
        let Some((field_name, remaining_field_path)) = field_path.split_first() else {
            return Some(self.clone());
        };

        match self {
            Self::Object(typed_fields) => {
                let typed_field = typed_fields.iter().find(|typed_field| typed_field.name == *field_name)?;

                typed_field
                    .field_type
                    .resolved_field_type_at_path(remaining_field_path, named_schemas)
            }
            Self::SchemaReference(schema_name) => named_schemas
                .get(schema_name)?
                .resolved_field_type_at_path(field_path, named_schemas),
            Self::Union(type_expressions) => {
                for type_expression in type_expressions {
                    if let Some(field_type) = type_expression.resolved_field_type_at_path(field_path, named_schemas) {
                        return Some(field_type);
                    }
                }

                None
            }
            Self::String
            | Self::Number
            | Self::Float
            | Self::Boolean
            | Self::Null
            | Self::AnyObject
            | Self::StringEnum(_)
            | Self::StringEnumReference(_)
            | Self::Array {
                item_type: _,
                fixed_length: _,
            }
            | Self::Tuple(_)
            | Self::Variant {
                discriminator: _,
                cases: _,
            } => None,
        }
    }

    #[must_use]
    pub fn is_string_enum_expression(&self) -> bool {
        match self {
            Self::StringEnum(_) => true,
            Self::Union(type_expressions) => type_expressions.iter().all(Self::is_string_enum_expression),
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
            } => false,
        }
    }

    #[must_use]
    pub fn is_resolved_string_enum_expression<HashBuilder: BuildHasher>(
        &self,
        named_schemas: &HashMap<String, TypeExpression, HashBuilder>,
    ) -> bool {
        match self {
            Self::StringEnum(_) => true,
            Self::StringEnumReference(reference) => {
                let Some((schema_name, field_path)) = reference.schema_name_and_field_path() else {
                    return false;
                };

                if field_path.is_empty() {
                    return false;
                }

                let Some(schema_type_expression) = named_schemas.get(schema_name) else {
                    return false;
                };

                schema_type_expression
                    .resolved_field_type_at_path(&field_path, named_schemas)
                    .is_some_and(|field_type| field_type.is_resolved_string_enum_expression(named_schemas))
            }
            Self::Union(type_expressions) => type_expressions
                .iter()
                .all(|type_expression| type_expression.is_resolved_string_enum_expression(named_schemas)),
            Self::String
            | Self::Number
            | Self::Float
            | Self::Boolean
            | Self::Null
            | Self::AnyObject
            | Self::SchemaReference(_)
            | Self::Array {
                item_type: _,
                fixed_length: _,
            }
            | Self::Tuple(_)
            | Self::Object(_)
            | Self::Variant {
                discriminator: _,
                cases: _,
            } => false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ReferenceKeyword {
    Agent,
    Dynamic,
    Input,
    Model,
    Secrets,
    Tool,
    Resource,
    Prompt,
}

impl ReferenceKeyword {
    #[must_use]
    pub fn from_identifier(identifier: &str) -> Option<Self> {
        match identifier {
            "agent" => Some(Self::Agent),
            "dynamic" => Some(Self::Dynamic),
            "input" => Some(Self::Input),
            "model" => Some(Self::Model),
            "secrets" => Some(Self::Secrets),
            "tool" => Some(Self::Tool),
            "resource" => Some(Self::Resource),
            "prompt" => Some(Self::Prompt),
            _ => None,
        }
    }

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Agent => "agent",
            Self::Dynamic => "dynamic",
            Self::Input => "input",
            Self::Model => "model",
            Self::Secrets => "secrets",
            Self::Tool => "tool",
            Self::Resource => "resource",
            Self::Prompt => "prompt",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BuiltinFunctionName {
    Context,
    Template,
    Compact,
}

impl BuiltinFunctionName {
    #[must_use]
    pub fn from_identifier(identifier: &str) -> Option<Self> {
        match identifier {
            "context" => Some(Self::Context),
            "template" => Some(Self::Template),
            "compact" => Some(Self::Compact),
            _ => None,
        }
    }

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Context => "context",
            Self::Template => "template",
            Self::Compact => "compact",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ToolCallKeyword {
    Call,
}

impl ToolCallKeyword {
    #[must_use]
    pub fn from_identifier(identifier: &str) -> Option<Self> {
        match identifier {
            "call" => Some(Self::Call),
            _ => None,
        }
    }

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Call => "call",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BuiltinFunctionArgumentName {
    Agent,
}

impl BuiltinFunctionArgumentName {
    #[must_use]
    pub fn from_identifier(identifier: &str) -> Option<Self> {
        match identifier {
            "agent" => Some(Self::Agent),
            _ => None,
        }
    }

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Agent => "agent",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ModelCallArgumentName {
    Model,
}

impl ModelCallArgumentName {
    #[must_use]
    pub fn from_identifier(identifier: &str) -> Option<Self> {
        match identifier {
            "model" => Some(Self::Model),
            _ => None,
        }
    }

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Model => "model",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReferenceAccess {
    pub field: String,
    pub optional: bool,
}

#[cfg(test)]
mod tests {
    use super::{
        Declaration, ForClauseKeyword, SchemaDeclaration, SourcePosition, SourceSpan, TypeExpression, TypedField, VariantCase, Workflow,
    };
    use crate::dsl::structure::Agent;
    use serde_json::json;

    #[test]
    fn parses_for_clause_keywords_from_identifier() {
        assert_eq!(ForClauseKeyword::from_identifier("for"), Some(ForClauseKeyword::For));
        assert_eq!(ForClauseKeyword::from_identifier("in"), Some(ForClauseKeyword::In));
        assert_eq!(ForClauseKeyword::from_identifier("agent"), None);
    }

    #[test]
    fn renders_for_clause_keywords_as_str() {
        assert_eq!(ForClauseKeyword::For.as_str(), "for");
        assert_eq!(ForClauseKeyword::In.as_str(), "in");
    }

    #[test]
    fn maps_source_position_to_byte_offset() {
        let source_text = "alpha\nbeta\n";

        assert_eq!(SourcePosition { line: 1, column: 1 }.to_byte_offset(source_text), Some(0));
        assert_eq!(SourcePosition { line: 2, column: 1 }.to_byte_offset(source_text), Some(6));
        assert_eq!(SourcePosition { line: 2, column: 5 }.to_byte_offset(source_text), Some(10));
        assert_eq!(SourcePosition { line: 3, column: 1 }.to_byte_offset(source_text), Some(11));
    }

    #[test]
    fn maps_source_span_to_byte_range() {
        let source_text = "agent greeting";
        let source_span = SourceSpan {
            start: SourcePosition { line: 1, column: 7 },
            end: SourcePosition { line: 1, column: 15 },
        };

        assert_eq!(source_span.to_byte_range(source_text), Some(6..14));
    }

    #[test]
    fn suggests_closest_agent_property_name_for_typos() {
        let agent = Agent::new();

        assert_eq!(
            agent
                .suggested_property_definition("instrction")
                .map(|property_definition| property_definition.name),
            Some("instruction")
        );

        assert_eq!(
            agent
                .suggested_property_definition("modle")
                .map(|property_definition| property_definition.name),
            Some("model")
        );
    }

    #[test]
    fn does_not_suggest_agent_property_name_for_distant_identifier() {
        assert_eq!(Agent::new().suggested_property_definition("retries"), None);
    }

    #[test]
    fn samples_json_values_from_type_expressions() {
        let workflow = Workflow {
            declarations: Vec::new(),
            source_text: None,
        };
        let type_expression = TypeExpression::Object(vec![
            typed_field("title", TypeExpression::String),
            typed_field("count", TypeExpression::Number),
            typed_field("enabled", TypeExpression::Boolean),
            typed_field(
                "metadata",
                TypeExpression::Union(vec![
                    TypeExpression::Null,
                    TypeExpression::Object(vec![typed_field("owner", TypeExpression::String)]),
                ]),
            ),
            typed_field(
                "items",
                TypeExpression::Array {
                    item_type: Box::new(TypeExpression::String),
                    fixed_length: None,
                },
            ),
        ]);

        assert_eq!(
            type_expression.sample_json_value(&workflow),
            json!({
                "title": "",
                "count": 0,
                "enabled": false,
                "metadata": {
                    "owner": ""
                },
                "items": []
            })
        );
    }

    #[test]
    fn samples_json_values_from_schema_references_and_variants() {
        let workflow = Workflow {
            declarations: vec![Declaration::Schema(SchemaDeclaration {
                name: "Payload".to_string(),
                fields: Vec::new(),
                root_variant: Some(TypeExpression::Variant {
                    discriminator: "kind".to_string(),
                    cases: vec![VariantCase {
                        name: "email".to_string(),
                        fields: vec![typed_field("subject", TypeExpression::String)],
                        span: test_source_span(),
                    }],
                }),
                span: test_source_span(),
            })],
            source_text: None,
        };
        let type_expression = TypeExpression::SchemaReference("Payload".to_string());

        assert_eq!(
            type_expression.sample_json_value(&workflow),
            json!({
                "kind": "email",
                "subject": ""
            })
        );
    }

    fn typed_field(field_name: &str, field_type: TypeExpression) -> TypedField {
        TypedField {
            name: field_name.to_string(),
            field_type,
            description: None,
            span: test_source_span(),
        }
    }

    fn test_source_span() -> SourceSpan {
        SourceSpan {
            start: SourcePosition { line: 1, column: 1 },
            end: SourcePosition { line: 1, column: 1 },
        }
    }
}
