use crate::dsl::{Reference, ReferenceAccess, ReferenceKeyword, ReferenceRoot, SourceSpan};
use crate::semantic::support::types::WorkflowType;
use crate::semantic::{SemanticMcpImport, SemanticModel, SemanticToolSchema, WorkflowSemanticIndex};
use std::collections::HashMap;

#[derive(Debug, Clone, Default)]
pub struct ReferenceResolutionScope {
    dynamic_field_types: HashMap<String, WorkflowType>,
    local_binding_types: HashMap<String, WorkflowType>,
    dynamic_field_spans: HashMap<String, SourceSpan>,
    local_binding_spans: HashMap<String, SourceSpan>,
}

impl ReferenceResolutionScope {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn with_dynamic_field_type(mut self, field_name: impl Into<String>, field_type: WorkflowType) -> Self {
        self.dynamic_field_types.insert(field_name.into(), field_type);

        self
    }

    #[must_use]
    pub fn with_dynamic_field_span(mut self, field_name: impl Into<String>, field_span: SourceSpan) -> Self {
        self.dynamic_field_spans.insert(field_name.into(), field_span);

        self
    }

    #[must_use]
    pub fn with_local_binding_type(mut self, binding_name: impl Into<String>, binding_type: WorkflowType) -> Self {
        self.local_binding_types.insert(binding_name.into(), binding_type);

        self
    }

    #[must_use]
    pub fn with_local_binding_span(mut self, binding_name: impl Into<String>, binding_span: SourceSpan) -> Self {
        self.local_binding_spans.insert(binding_name.into(), binding_span);

        self
    }

    #[must_use]
    pub fn dynamic_field_type(&self, field_name: &str) -> Option<&WorkflowType> {
        self.dynamic_field_types.get(field_name)
    }

    #[must_use]
    pub fn dynamic_field_span(&self, field_name: &str) -> Option<SourceSpan> {
        self.dynamic_field_spans.get(field_name).copied()
    }

    #[must_use]
    pub fn local_binding_type(&self, binding_name: &str) -> Option<&WorkflowType> {
        self.local_binding_types.get(binding_name)
    }

    #[must_use]
    pub fn local_binding_span(&self, binding_name: &str) -> Option<SourceSpan> {
        self.local_binding_spans.get(binding_name).copied()
    }

    #[must_use]
    pub fn has_dynamic_fields(&self) -> bool {
        !self.dynamic_field_types.is_empty()
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ReferenceResolver<'semantic> {
    semantic_index: &'semantic WorkflowSemanticIndex,
}

impl<'semantic> ReferenceResolver<'semantic> {
    #[must_use]
    pub fn new(semantic_index: &'semantic WorkflowSemanticIndex) -> Self {
        Self { semantic_index }
    }

    #[must_use]
    pub fn semantic_index(self) -> &'semantic WorkflowSemanticIndex {
        self.semantic_index
    }

    pub fn resolve(
        self,
        reference: &Reference,
        resolution_scope: &ReferenceResolutionScope,
    ) -> Result<ReferenceResolution<'semantic>, ReferenceResolutionError> {
        reference.resolve_with(self, resolution_scope)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedValueReference {
    pub root: ReferenceResolutionRoot,
    pub field_name: String,
    pub projection: Vec<ReferenceAccess>,
    pub resolved_type: WorkflowType,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedNamedValueReference {
    pub root: ReferenceResolutionRoot,
    pub name: String,
    pub projection: Vec<ReferenceAccess>,
    pub resolved_type: WorkflowType,
}

#[derive(Debug, Clone)]
pub struct ResolvedToolReference<'semantic> {
    pub tool_schema: &'semantic SemanticToolSchema,
    pub mcp_import: Option<&'semantic SemanticMcpImport>,
    pub projection: Vec<ReferenceAccess>,
    pub resolved_type: Option<WorkflowType>,
}

#[derive(Debug, Clone)]
pub struct ResolvedModelReference<'semantic> {
    pub model: &'semantic SemanticModel,
}

#[derive(Debug, Clone)]
pub struct ResolvedMcpImportReference<'semantic> {
    pub import: &'semantic SemanticMcpImport,
}

#[derive(Debug, Clone)]
pub enum ReferenceResolution<'semantic> {
    Input(ResolvedValueReference),
    Secrets(ResolvedValueReference),
    Dynamic(ResolvedValueReference),
    AgentOutput(ResolvedNamedValueReference),
    Tool(ResolvedToolReference<'semantic>),
    ToolImport(ResolvedMcpImportReference<'semantic>),
    ResourceImport(ResolvedMcpImportReference<'semantic>),
    PromptImport(ResolvedMcpImportReference<'semantic>),
    Model(ResolvedModelReference<'semantic>),
    LocalBinding(ResolvedNamedValueReference),
}

impl ReferenceResolution<'_> {
    #[must_use]
    pub fn root(&self) -> ReferenceResolutionRoot {
        match self {
            Self::Input(resolved_value) | Self::Secrets(resolved_value) | Self::Dynamic(resolved_value) => resolved_value.root,
            Self::AgentOutput(resolved_value) | Self::LocalBinding(resolved_value) => resolved_value.root,
            Self::Tool(_) | Self::ToolImport(_) => ReferenceResolutionRoot::Tool,
            Self::ResourceImport(_) => ReferenceResolutionRoot::Resource,
            Self::PromptImport(_) => ReferenceResolutionRoot::Prompt,
            Self::Model(_) => ReferenceResolutionRoot::Model,
        }
    }

    #[must_use]
    pub fn name(&self) -> Option<&str> {
        match self {
            Self::Input(resolved_value) | Self::Secrets(resolved_value) | Self::Dynamic(resolved_value) => Some(&resolved_value.field_name),
            Self::AgentOutput(resolved_value) | Self::LocalBinding(resolved_value) => Some(&resolved_value.name),
            Self::Tool(resolved_tool) => Some(&resolved_tool.tool_schema.name),
            Self::ToolImport(resolved_import) | Self::ResourceImport(resolved_import) | Self::PromptImport(resolved_import) => {
                Some(&resolved_import.import.name)
            }
            Self::Model(resolved_model) => Some(&resolved_model.model.name),
        }
    }

    #[must_use]
    pub fn resolved_type(&self) -> Option<&WorkflowType> {
        match self {
            Self::Input(resolved_value) | Self::Secrets(resolved_value) | Self::Dynamic(resolved_value) => {
                Some(&resolved_value.resolved_type)
            }
            Self::AgentOutput(resolved_value) | Self::LocalBinding(resolved_value) => Some(&resolved_value.resolved_type),
            Self::Tool(resolved_tool) => resolved_tool.resolved_type.as_ref(),
            Self::ToolImport(_) | Self::ResourceImport(_) | Self::PromptImport(_) | Self::Model(_) => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ReferenceResolutionRoot {
    Input,
    Secrets,
    Dynamic,
    Agent,
    Tool,
    Resource,
    Prompt,
    Model,
    LocalBinding,
}

impl ReferenceResolutionRoot {
    #[must_use]
    pub fn from_keyword(reference_keyword: ReferenceKeyword) -> Self {
        match reference_keyword {
            ReferenceKeyword::Input => Self::Input,
            ReferenceKeyword::Secrets => Self::Secrets,
            ReferenceKeyword::Dynamic => Self::Dynamic,
            ReferenceKeyword::Agent => Self::Agent,
            ReferenceKeyword::Tool => Self::Tool,
            ReferenceKeyword::Resource => Self::Resource,
            ReferenceKeyword::Prompt => Self::Prompt,
            ReferenceKeyword::Model => Self::Model,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReferenceResolutionError {
    MissingAccess { root: ReferenceResolutionRoot },
    MissingInputDeclaration,
    MissingSecretsDeclaration,
    MissingDynamicScope,
    UnknownInputField { field_name: String },
    UnknownSecretsField { field_name: String },
    UnknownDynamicField { field_name: String },
    UnknownAgent { agent_name: String },
    MissingAgentOutputType { agent_name: String },
    UnknownTool { tool_name: String },
    UnknownResourceImport { import_name: String },
    UnknownPromptImport { import_name: String },
    UnknownModel { model_name: String },
    UnknownLocalBinding { binding_name: String },
    InvalidPath { root: ReferenceResolutionRoot, field_name: String },
    MissingOptionalAccess { root: ReferenceResolutionRoot, field_name: String },
    UnsupportedProjection { root: ReferenceResolutionRoot, name: String },
}

impl WorkflowSemanticIndex {
    #[must_use]
    pub fn reference_resolver(&self) -> ReferenceResolver<'_> {
        ReferenceResolver::new(self)
    }
}

impl Reference {
    pub fn resolve_with<'semantic>(
        &self,
        reference_resolver: ReferenceResolver<'semantic>,
        resolution_scope: &ReferenceResolutionScope,
    ) -> Result<ReferenceResolution<'semantic>, ReferenceResolutionError> {
        match &self.root {
            ReferenceRoot::Keyword(reference_keyword) => {
                self.resolve_keyword_reference(*reference_keyword, reference_resolver, resolution_scope)
            }
            ReferenceRoot::Identifier(identifier) => self.resolve_local_binding_reference(identifier, resolution_scope),
        }
    }

    fn resolve_keyword_reference<'semantic>(
        &self,
        reference_keyword: ReferenceKeyword,
        reference_resolver: ReferenceResolver<'semantic>,
        resolution_scope: &ReferenceResolutionScope,
    ) -> Result<ReferenceResolution<'semantic>, ReferenceResolutionError> {
        match reference_keyword {
            ReferenceKeyword::Input => self.resolve_input_reference(reference_resolver),
            ReferenceKeyword::Secrets => self.resolve_secrets_reference(reference_resolver),
            ReferenceKeyword::Dynamic => self.resolve_dynamic_reference(resolution_scope),
            ReferenceKeyword::Agent => self.resolve_agent_reference(reference_resolver),
            ReferenceKeyword::Tool => self.resolve_tool_reference(reference_resolver),
            ReferenceKeyword::Resource => self.resolve_resource_import_reference(reference_resolver),
            ReferenceKeyword::Prompt => self.resolve_prompt_import_reference(reference_resolver),
            ReferenceKeyword::Model => self.resolve_model_reference(reference_resolver),
        }
    }

    fn resolve_input_reference<'semantic>(
        &self,
        reference_resolver: ReferenceResolver<'semantic>,
    ) -> Result<ReferenceResolution<'semantic>, ReferenceResolutionError> {
        let input_type = reference_resolver
            .semantic_index()
            .input_type()
            .ok_or(ReferenceResolutionError::MissingInputDeclaration)?;
        let field_name = self.required_first_access_field(ReferenceResolutionRoot::Input)?;

        if !self.root_field_exists(input_type, field_name) {
            return Err(ReferenceResolutionError::UnknownInputField {
                field_name: field_name.to_string(),
            });
        }

        let resolved_type = self.resolve_workflow_type_path(input_type, 0, ReferenceResolutionRoot::Input)?;

        Ok(ReferenceResolution::Input(ResolvedValueReference {
            root: ReferenceResolutionRoot::Input,
            field_name: field_name.to_string(),
            projection: self.projection_accesses().to_vec(),
            resolved_type,
        }))
    }

    fn resolve_secrets_reference<'semantic>(
        &self,
        reference_resolver: ReferenceResolver<'semantic>,
    ) -> Result<ReferenceResolution<'semantic>, ReferenceResolutionError> {
        let secrets_type = reference_resolver
            .semantic_index()
            .secrets_type()
            .ok_or(ReferenceResolutionError::MissingSecretsDeclaration)?;
        let field_name = self.required_first_access_field(ReferenceResolutionRoot::Secrets)?;

        if !self.root_field_exists(secrets_type, field_name) {
            return Err(ReferenceResolutionError::UnknownSecretsField {
                field_name: field_name.to_string(),
            });
        }

        let resolved_type = self.resolve_workflow_type_path(secrets_type, 0, ReferenceResolutionRoot::Secrets)?;

        Ok(ReferenceResolution::Secrets(ResolvedValueReference {
            root: ReferenceResolutionRoot::Secrets,
            field_name: field_name.to_string(),
            projection: self.projection_accesses().to_vec(),
            resolved_type,
        }))
    }

    fn resolve_dynamic_reference<'semantic>(
        &self,
        resolution_scope: &ReferenceResolutionScope,
    ) -> Result<ReferenceResolution<'semantic>, ReferenceResolutionError> {
        let field_name = self.required_first_access_field(ReferenceResolutionRoot::Dynamic)?;
        let Some(dynamic_field_type) = resolution_scope.dynamic_field_type(field_name) else {
            if resolution_scope.has_dynamic_fields() {
                return Err(ReferenceResolutionError::UnknownDynamicField {
                    field_name: field_name.to_string(),
                });
            }

            return Err(ReferenceResolutionError::MissingDynamicScope);
        };
        let resolved_type = self.resolve_workflow_type_path(dynamic_field_type, 1, ReferenceResolutionRoot::Dynamic)?;

        Ok(ReferenceResolution::Dynamic(ResolvedValueReference {
            root: ReferenceResolutionRoot::Dynamic,
            field_name: field_name.to_string(),
            projection: self.projection_accesses().to_vec(),
            resolved_type,
        }))
    }

    fn resolve_agent_reference<'semantic>(
        &self,
        reference_resolver: ReferenceResolver<'semantic>,
    ) -> Result<ReferenceResolution<'semantic>, ReferenceResolutionError> {
        let agent_name = self.required_first_access_field(ReferenceResolutionRoot::Agent)?;
        let Some(agent_output_type) = reference_resolver.semantic_index().agent_output_workflow_type(agent_name) else {
            if reference_resolver.semantic_index().has_agent(agent_name) {
                return Err(ReferenceResolutionError::MissingAgentOutputType {
                    agent_name: agent_name.to_string(),
                });
            }

            return Err(ReferenceResolutionError::UnknownAgent {
                agent_name: agent_name.to_string(),
            });
        };
        let resolved_type = self.resolve_workflow_type_path(agent_output_type, 1, ReferenceResolutionRoot::Agent)?;

        Ok(ReferenceResolution::AgentOutput(ResolvedNamedValueReference {
            root: ReferenceResolutionRoot::Agent,
            name: agent_name.to_string(),
            projection: self.projection_accesses().to_vec(),
            resolved_type,
        }))
    }

    fn resolve_tool_reference<'semantic>(
        &self,
        reference_resolver: ReferenceResolver<'semantic>,
    ) -> Result<ReferenceResolution<'semantic>, ReferenceResolutionError> {
        let tool_name = self.required_first_access_field(ReferenceResolutionRoot::Tool)?;
        let Some(tool_schema) = reference_resolver.semantic_index().tool_schema(tool_name) else {
            return Err(ReferenceResolutionError::UnknownTool {
                tool_name: tool_name.to_string(),
            });
        };
        let resolved_type = reference_resolver
            .semantic_index()
            .tool_output_type(tool_name)
            .map(|tool_output_type| self.resolve_workflow_type_path(tool_output_type, 1, ReferenceResolutionRoot::Tool))
            .transpose()?;
        let mcp_import = reference_resolver.semantic_index().mcp_tool_import(tool_name);

        if self.has_single_access() {
            if let Some(mcp_import) = mcp_import {
                return Ok(ReferenceResolution::ToolImport(ResolvedMcpImportReference { import: mcp_import }));
            }
        }

        Ok(ReferenceResolution::Tool(ResolvedToolReference {
            tool_schema,
            mcp_import,
            projection: self.projection_accesses().to_vec(),
            resolved_type,
        }))
    }

    fn resolve_resource_import_reference<'semantic>(
        &self,
        reference_resolver: ReferenceResolver<'semantic>,
    ) -> Result<ReferenceResolution<'semantic>, ReferenceResolutionError> {
        let import_name = self.required_first_access_field(ReferenceResolutionRoot::Resource)?;

        if !self.has_single_access() {
            return Err(ReferenceResolutionError::UnsupportedProjection {
                root: ReferenceResolutionRoot::Resource,
                name: import_name.to_string(),
            });
        }

        let Some(import) = reference_resolver.semantic_index().resource_import(import_name) else {
            return Err(ReferenceResolutionError::UnknownResourceImport {
                import_name: import_name.to_string(),
            });
        };

        Ok(ReferenceResolution::ResourceImport(ResolvedMcpImportReference { import }))
    }

    fn resolve_prompt_import_reference<'semantic>(
        &self,
        reference_resolver: ReferenceResolver<'semantic>,
    ) -> Result<ReferenceResolution<'semantic>, ReferenceResolutionError> {
        let import_name = self.required_first_access_field(ReferenceResolutionRoot::Prompt)?;

        if !self.has_single_access() {
            return Err(ReferenceResolutionError::UnsupportedProjection {
                root: ReferenceResolutionRoot::Prompt,
                name: import_name.to_string(),
            });
        }

        let Some(import) = reference_resolver.semantic_index().prompt_import(import_name) else {
            return Err(ReferenceResolutionError::UnknownPromptImport {
                import_name: import_name.to_string(),
            });
        };

        Ok(ReferenceResolution::PromptImport(ResolvedMcpImportReference { import }))
    }

    fn resolve_model_reference<'semantic>(
        &self,
        reference_resolver: ReferenceResolver<'semantic>,
    ) -> Result<ReferenceResolution<'semantic>, ReferenceResolutionError> {
        let model_name = self.required_first_access_field(ReferenceResolutionRoot::Model)?;

        if !self.has_single_access() {
            return Err(ReferenceResolutionError::UnsupportedProjection {
                root: ReferenceResolutionRoot::Model,
                name: model_name.to_string(),
            });
        }

        let Some(model) = reference_resolver.semantic_index().model(model_name) else {
            return Err(ReferenceResolutionError::UnknownModel {
                model_name: model_name.to_string(),
            });
        };

        Ok(ReferenceResolution::Model(ResolvedModelReference { model }))
    }

    fn resolve_local_binding_reference<'semantic>(
        &self,
        binding_name: &str,
        resolution_scope: &ReferenceResolutionScope,
    ) -> Result<ReferenceResolution<'semantic>, ReferenceResolutionError> {
        let Some(binding_type) = resolution_scope.local_binding_type(binding_name) else {
            return Err(ReferenceResolutionError::UnknownLocalBinding {
                binding_name: binding_name.to_string(),
            });
        };
        let resolved_type = self.resolve_workflow_type_path(binding_type, 0, ReferenceResolutionRoot::LocalBinding)?;

        Ok(ReferenceResolution::LocalBinding(ResolvedNamedValueReference {
            root: ReferenceResolutionRoot::LocalBinding,
            name: binding_name.to_string(),
            projection: self.accesses.clone(),
            resolved_type,
        }))
    }

    fn required_first_access_field(&self, root: ReferenceResolutionRoot) -> Result<&str, ReferenceResolutionError> {
        self.first_access_field().ok_or(ReferenceResolutionError::MissingAccess { root })
    }

    fn root_field_exists(&self, root_type: &WorkflowType, field_name: &str) -> bool {
        root_type.field_type(field_name).is_some()
    }

    fn resolve_workflow_type_path(
        &self,
        root_type: &WorkflowType,
        access_start_index: usize,
        root: ReferenceResolutionRoot,
    ) -> Result<WorkflowType, ReferenceResolutionError> {
        let mut candidate_types = vec![root_type.clone()];

        for reference_access in self.accesses_from(access_start_index) {
            if candidate_types.iter().any(WorkflowType::can_be_null) && !reference_access.optional {
                return Err(ReferenceResolutionError::MissingOptionalAccess {
                    root,
                    field_name: reference_access.field.clone(),
                });
            }

            let mut next_candidate_types = Vec::new();

            for candidate_type in &candidate_types {
                if let Some(field_type) = candidate_type.without_null().field_type(&reference_access.field) {
                    next_candidate_types.push(field_type);
                }
            }

            if reference_access.optional {
                next_candidate_types.push(WorkflowType::Null);
            }

            if next_candidate_types.is_empty() {
                return Err(ReferenceResolutionError::InvalidPath {
                    root,
                    field_name: reference_access.field.clone(),
                });
            }

            candidate_types = next_candidate_types;
        }

        Ok(merge_workflow_types(candidate_types))
    }
}

fn merge_workflow_types(workflow_types: Vec<WorkflowType>) -> WorkflowType {
    if workflow_types.len() == 1 {
        return workflow_types[0].clone().normalize();
    }

    WorkflowType::Union(workflow_types).normalize()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsl::{ReferenceAccess, ReferenceRoot};

    #[test]
    fn resolves_value_references_from_semantic_index_and_scope() {
        let semantic_index = resolver_test_index();
        let reference_resolver = semantic_index.reference_resolver();
        let resolution_scope = resolver_test_scope();

        assert_value_resolution(
            reference_resolver.resolve(
                &reference(
                    ReferenceRoot::Keyword(ReferenceKeyword::Input),
                    [("profile", false), ("title", false)],
                ),
                &resolution_scope,
            ),
            ReferenceResolutionRoot::Input,
            WorkflowType::String,
        );
        assert_value_resolution(
            reference_resolver.resolve(
                &reference(ReferenceRoot::Keyword(ReferenceKeyword::Secrets), [("api_key", false)]),
                &resolution_scope,
            ),
            ReferenceResolutionRoot::Secrets,
            WorkflowType::String,
        );
        assert_value_resolution(
            reference_resolver.resolve(
                &reference(ReferenceRoot::Keyword(ReferenceKeyword::Dynamic), [("topic", false)]),
                &resolution_scope,
            ),
            ReferenceResolutionRoot::Dynamic,
            WorkflowType::String,
        );
        assert_named_value_resolution(
            reference_resolver.resolve(
                &reference(
                    ReferenceRoot::Keyword(ReferenceKeyword::Agent),
                    [("worker", false), ("summary", false)],
                ),
                &resolution_scope,
            ),
            ReferenceResolutionRoot::Agent,
            "worker",
            WorkflowType::String,
        );
    }

    #[test]
    fn resolves_tool_references_from_semantic_index() {
        let semantic_index = resolver_test_index();
        let reference_resolver = semantic_index.reference_resolver();
        let resolution_scope = resolver_test_scope();

        assert_tool_resolution(
            reference_resolver.resolve(
                &reference(
                    ReferenceRoot::Keyword(ReferenceKeyword::Tool),
                    [("local_lookup", false), ("result", false)],
                ),
                &resolution_scope,
            ),
            "local_lookup",
            false,
            WorkflowType::String,
        );

        let remote_tool_resolution = reference_resolver
            .resolve(
                &reference(ReferenceRoot::Keyword(ReferenceKeyword::Tool), [("remote_lookup", false)]),
                &resolution_scope,
            )
            .expect("imported tool reference should resolve");

        assert!(matches!(remote_tool_resolution, ReferenceResolution::ToolImport(_)));
    }

    #[test]
    fn resolves_import_and_model_references_from_semantic_index() {
        let semantic_index = resolver_test_index();
        let reference_resolver = semantic_index.reference_resolver();
        let resolution_scope = resolver_test_scope();

        let resource_resolution = reference_resolver
            .resolve(
                &reference(ReferenceRoot::Keyword(ReferenceKeyword::Resource), [("project_readme", false)]),
                &resolution_scope,
            )
            .expect("resource reference should resolve");

        assert!(matches!(resource_resolution, ReferenceResolution::ResourceImport(_)));

        let prompt_resolution = reference_resolver
            .resolve(
                &reference(ReferenceRoot::Keyword(ReferenceKeyword::Prompt), [("system_prompt", false)]),
                &resolution_scope,
            )
            .expect("prompt reference should resolve");

        assert!(matches!(prompt_resolution, ReferenceResolution::PromptImport(_)));

        let model_resolution = reference_resolver
            .resolve(
                &reference(ReferenceRoot::Keyword(ReferenceKeyword::Model), [("fast", false)]),
                &resolution_scope,
            )
            .expect("model reference should resolve");

        assert!(matches!(model_resolution, ReferenceResolution::Model(_)));
    }

    #[test]
    fn reports_typed_resolution_errors() {
        let workflow = crate::parse_inline_workflow! {
            input {
                topic: string
            }

            agent worker {
                instruction: input.topic
            }
        };
        let semantic_index = WorkflowSemanticIndex::from_workflow(&workflow);
        let reference_resolver = semantic_index.reference_resolver();
        let resolution_scope = ReferenceResolutionScope::new();

        let unknown_input_result = reference_resolver.resolve(
            &reference(ReferenceRoot::Keyword(ReferenceKeyword::Input), [("missing", false)]),
            &resolution_scope,
        );

        assert_eq!(
            unknown_input_result.expect_err("missing input field should fail"),
            ReferenceResolutionError::UnknownInputField {
                field_name: "missing".to_string()
            }
        );

        let missing_output_result = reference_resolver.resolve(
            &reference(
                ReferenceRoot::Keyword(ReferenceKeyword::Agent),
                [("worker", false), ("value", false)],
            ),
            &resolution_scope,
        );

        assert_eq!(
            missing_output_result.expect_err("missing agent output should fail"),
            ReferenceResolutionError::MissingAgentOutputType {
                agent_name: "worker".to_string()
            }
        );
    }

    fn assert_value_resolution(
        resolution_result: Result<ReferenceResolution<'_>, ReferenceResolutionError>,
        expected_root: ReferenceResolutionRoot,
        expected_type: WorkflowType,
    ) {
        let resolution = resolution_result.expect("value reference should resolve");
        let resolved_value = match resolution {
            ReferenceResolution::Input(resolved_value)
            | ReferenceResolution::Secrets(resolved_value)
            | ReferenceResolution::Dynamic(resolved_value) => resolved_value,
            other_resolution => panic!("expected value reference resolution, got {other_resolution:?}"),
        };

        assert_eq!(resolved_value.root, expected_root);
        assert_eq!(resolved_value.resolved_type, expected_type);
    }

    fn assert_named_value_resolution(
        resolution_result: Result<ReferenceResolution<'_>, ReferenceResolutionError>,
        expected_root: ReferenceResolutionRoot,
        expected_name: &str,
        expected_type: WorkflowType,
    ) {
        let resolution = resolution_result.expect("named value reference should resolve");
        let resolved_value = match resolution {
            ReferenceResolution::AgentOutput(resolved_value) | ReferenceResolution::LocalBinding(resolved_value) => resolved_value,
            other_resolution => panic!("expected named value reference resolution, got {other_resolution:?}"),
        };

        assert_eq!(resolved_value.root, expected_root);
        assert_eq!(resolved_value.name, expected_name);
        assert_eq!(resolved_value.resolved_type, expected_type);
    }

    fn assert_tool_resolution(
        resolution_result: Result<ReferenceResolution<'_>, ReferenceResolutionError>,
        expected_tool_name: &str,
        expected_imported: bool,
        expected_type: WorkflowType,
    ) {
        let resolution = resolution_result.expect("tool reference should resolve");
        let ReferenceResolution::Tool(resolved_tool) = resolution else {
            panic!("expected tool reference resolution");
        };

        assert_eq!(resolved_tool.tool_schema.name, expected_tool_name);
        assert_eq!(resolved_tool.mcp_import.is_some(), expected_imported);
        assert_eq!(resolved_tool.resolved_type, Some(expected_type));
    }

    fn resolver_test_index() -> WorkflowSemanticIndex {
        let workflow = crate::parse_inline_workflow! {
            provider openai from openai {}

            model fast from openai {
                id: "gpt-4.1-mini"
            }

            input {
                profile: {
                    title: string
                }
            }

            secrets {
                api_key: string
            }

            mcp local {
                endpoint: "http://localhost:3000/mcp"
            }

            resource project_readme from mcp.local.resource.project_readme
            prompt system_prompt from mcp.local.prompt.system_prompt

            tool local_lookup {
                input {}

                output {
                    result: string
                }
            }

            tool remote_lookup from mcp.local.tool.remote_lookup {
                output {
                    result: string
                }
            }

            agent worker {
                instruction: input.profile.title

                output {
                    summary: string
                }
            }
        };

        WorkflowSemanticIndex::from_workflow(&workflow)
    }

    fn resolver_test_scope() -> ReferenceResolutionScope {
        ReferenceResolutionScope::new().with_dynamic_field_type("topic", WorkflowType::String)
    }

    fn reference<const ACCESS_COUNT: usize>(root: ReferenceRoot, accesses: [(&str, bool); ACCESS_COUNT]) -> Reference {
        Reference {
            root,
            accesses: accesses
                .into_iter()
                .map(|(field_name, optional)| ReferenceAccess {
                    field: field_name.to_string(),
                    optional,
                })
                .collect(),
            span: crate::dsl::SourceSpan::generated(),
        }
    }
}
