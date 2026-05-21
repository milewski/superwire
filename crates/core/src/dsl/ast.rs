use serde_json::Value;

mod agent;
mod expression;
mod keywords;
mod mcp;
mod reference;
mod span;
mod tool;
mod types;

pub use agent::{AgentDeclaration, AgentForLoop, AgentForLoopPattern, AgentProperty, ModelUsage};
pub use expression::{
    CallArgument, Expression, FunctionCall, MatchBranch, MatchExpression, McpCall, McpCallOperation, NamedArgument, NullFallbackExpression,
    ObjectField, StringTemplate, StringTemplatePart, ToolCall, VariantProjectionExpression,
};
pub use keywords::{
    AgentExpressionPropertyName, BuiltinFunctionArgumentName, BuiltinFunctionName, DeclarationKeyword, ForClauseKeyword, ImportKeyword,
    McpImportPropertyName, McpServerPropertyName, McpToolBatchImportPropertyName, ModelCallArgumentName, ModelDeclarationPropertyName,
    ModelUsagePropertyName, ReferenceKeyword, ToolCallKeyword, ToolCallPropertyName, ToolPropertyName,
};
pub use mcp::{
    McpBatchImportDeclaration, McpImportBindings, McpImportKind, McpImportSource, McpPromptBatchImportDeclaration,
    McpPromptBatchImportItem, McpPromptImportDeclaration, McpResourceBatchImportDeclaration, McpResourceBatchImportItem,
    McpResourceImportDeclaration, McpToolBatchImportDeclaration, McpToolBatchImportItem, McpToolSource,
};
pub use reference::{Reference, ReferenceAccess, ReferenceRoot};
pub use span::{SourcePosition, SourceSpan};
pub use tool::{ToolDeclaration, ToolSource};
pub use types::{TypeExpression, TypedField, VariantCase};

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpServerDeclaration {
    pub name: String,
    pub properties: Vec<ObjectField>,
    pub span: SourceSpan,
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
pub struct OutputDeclaration {
    pub fields: Vec<ObjectField>,
    pub span: SourceSpan,
}

#[cfg(test)]
mod tests {
    use super::{ForClauseKeyword, Reference, ReferenceAccess, ReferenceKeyword, ReferenceRoot, SourcePosition, SourceSpan};
    use crate::dsl::structure::Agent;

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
    fn reference_access_helpers_expose_owned_path_segments() {
        let reference = reference_with_accesses(ReferenceKeyword::Input, [("profile", false), ("address", true), ("city", false)]);

        assert_eq!(reference.first_access_field(), Some("profile"));
        assert_eq!(
            reference.first_projection_access().map(|access| access.field.as_str()),
            Some("address")
        );
        assert_eq!(reference.last_access().map(|access| access.field.as_str()), Some("city"));

        let projection_fields = reference
            .projection_accesses()
            .iter()
            .map(|access| access.field.as_str())
            .collect::<Vec<_>>();

        assert_eq!(projection_fields, vec!["address", "city"]);
    }

    #[test]
    fn reference_keyword_predicates_require_direct_required_access() {
        let model_reference = reference_with_accesses(ReferenceKeyword::Model, [("fast", false)]);
        let optional_model_reference = reference_with_accesses(ReferenceKeyword::Model, [("fast", true)]);
        let nested_model_reference = reference_with_accesses(ReferenceKeyword::Model, [("fast", false), ("name", false)]);
        let secret_reference = reference_with_accesses(ReferenceKeyword::Secrets, [("api_key", false)]);

        assert_eq!(
            model_reference.direct_required_name_for_keyword(ReferenceKeyword::Model),
            Some("fast")
        );
        assert!(model_reference.is_direct_required_reference_to_keyword(ReferenceKeyword::Model));
        assert!(!optional_model_reference.is_direct_required_reference_to_keyword(ReferenceKeyword::Model));
        assert!(!nested_model_reference.is_direct_required_reference_to_keyword(ReferenceKeyword::Model));
        assert!(secret_reference.is_secret_reference());
    }

    fn reference_with_accesses<const ACCESS_COUNT: usize>(
        reference_keyword: ReferenceKeyword,
        accesses: [(&str, bool); ACCESS_COUNT],
    ) -> Reference {
        Reference {
            root: ReferenceRoot::Keyword(reference_keyword),
            accesses: accesses
                .into_iter()
                .map(|(field_name, optional)| ReferenceAccess {
                    field: field_name.to_string(),
                    optional,
                })
                .collect(),
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
