use super::{
    AgentDeclaration, Expression, McpBatchImportDeclaration, McpPromptBatchImportDeclaration, McpPromptImportDeclaration,
    McpResourceBatchImportDeclaration, McpResourceImportDeclaration, McpToolBatchImportDeclaration, ModelAssetKind,
    ModelDeclarationPropertyName, ModelWireApi, ObjectField, SourceSpan, ToolDeclaration, TypeExpression, TypedField, Workflow,
};

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

    #[must_use]
    pub fn wire_api(&self) -> ModelWireApi {
        let Some(Expression::StringLiteral(wire_api)) =
            self.property(ModelDeclarationPropertyName::WireApi).map(|property| &property.value)
        else {
            return ModelWireApi::default_value();
        };

        ModelWireApi::from_identifier(wire_api).unwrap_or_else(ModelWireApi::default_value)
    }

    pub fn supported_asset_kinds(&self) -> Result<Vec<ModelAssetKind>, ModelAssetKindSupportError> {
        let Some(property) = self.property(ModelDeclarationPropertyName::Assets) else {
            return Ok(Vec::new());
        };
        let Expression::ArrayLiteral(asset_kind_expressions) = &property.value else {
            return Err(ModelAssetKindSupportError {
                message: format!(
                    "model `{}` property `{}` must be an array of string asset kinds",
                    self.name,
                    ModelDeclarationPropertyName::Assets.as_str()
                ),
            });
        };
        let mut asset_kinds = Vec::new();

        for asset_kind_expression in asset_kind_expressions {
            let Expression::StringLiteral(asset_kind_name) = asset_kind_expression else {
                return Err(ModelAssetKindSupportError {
                    message: format!(
                        "model `{}` property `{}` entries must be string literals",
                        self.name,
                        ModelDeclarationPropertyName::Assets.as_str()
                    ),
                });
            };
            let Some(asset_kind) = ModelAssetKind::from_identifier(asset_kind_name) else {
                return Err(ModelAssetKindSupportError {
                    message: format!(
                        "model `{}` property `{}` uses unsupported asset kind `{asset_kind_name}`",
                        self.name,
                        ModelDeclarationPropertyName::Assets.as_str()
                    ),
                });
            };

            asset_kinds.push(asset_kind);
        }

        Ok(asset_kinds)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelAssetKindSupportError {
    pub message: String,
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

    #[must_use]
    pub fn sample_json_value(&self, workflow: &Workflow) -> serde_json::Value {
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
