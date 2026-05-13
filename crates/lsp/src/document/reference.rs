use std::collections::{BTreeMap, HashSet};

use superwire_core::dsl::{DeclarationKeyword, ReferenceKeyword, SourcePosition, SourceSpan, TypeExpression, TypedField};
use superwire_core::mcp::McpServerLock;
use superwire_core::semantic::ToolingReferencePath;

use crate::protocol::Position;

use super::position::source_span_contains_position;
use super::semantic_index::{FieldMetadata, SemanticIndex};
use super::text_utils::{for_clause_iterable_prefix, is_identifier, trailing_reference_token};
use super::{CompletionKind, CompletionSuggestion, RenderTypeExpression};

#[derive(Debug, Clone)]
pub struct ReferenceCompletionPath {
    root: String,
    pub complete_accesses: Vec<String>,
    complete_accesses_are_optional: Vec<bool>,
    pub pending_prefix: String,
    pub pending_access_is_optional: bool,
}

impl ReferenceCompletionPath {
    pub fn from_line_prefix(line_prefix: &str) -> Option<Self> {
        let reference_token = trailing_reference_token(line_prefix)?;

        Self::from_token(reference_token)
    }

    pub fn from_token(reference_token: &str) -> Option<Self> {
        if reference_token.is_empty() || reference_token.ends_with('?') {
            return None;
        }

        let parsed_token = ParsedReferenceToken::parse(reference_token)?;
        let root = parsed_token.root.clone();

        if !is_identifier(root.as_str()) {
            return None;
        }

        if parsed_token.accesses.is_empty() && !parsed_token.has_trailing_separator {
            return Some(Self {
                root,
                complete_accesses: Vec::new(),
                complete_accesses_are_optional: Vec::new(),
                pending_prefix: String::new(),
                pending_access_is_optional: false,
            });
        }

        let mut complete_accesses = Vec::<String>::new();
        let mut complete_accesses_are_optional = Vec::<bool>::new();

        if parsed_token.has_trailing_separator {
            for parsed_access in &parsed_token.accesses {
                if parsed_access.name.is_empty() || !is_identifier(&parsed_access.name) {
                    return None;
                }

                complete_accesses.push(parsed_access.name.clone());
                complete_accesses_are_optional.push(parsed_access.is_optional);
            }

            return Some(Self {
                root,
                complete_accesses,
                complete_accesses_are_optional,
                pending_prefix: String::new(),
                pending_access_is_optional: parsed_token.trailing_separator_is_optional,
            });
        }

        for parsed_access in parsed_token.accesses.iter().take(parsed_token.accesses.len().saturating_sub(1)) {
            if parsed_access.name.is_empty() || !is_identifier(&parsed_access.name) {
                return None;
            }

            complete_accesses.push(parsed_access.name.clone());
            complete_accesses_are_optional.push(parsed_access.is_optional);
        }

        let pending_access = parsed_token.accesses.last()?;
        let pending_prefix = pending_access.name.clone();

        if !pending_prefix.is_empty() && !is_identifier(&pending_prefix) {
            return None;
        }

        Some(Self {
            root,
            complete_accesses,
            complete_accesses_are_optional,
            pending_prefix,
            pending_access_is_optional: pending_access.is_optional,
        })
    }

    pub fn root_keyword(&self) -> Option<ReferenceKeyword> {
        ReferenceKeyword::from_identifier(&self.root)
    }

    pub fn root_identifier(&self) -> &str {
        &self.root
    }

    fn complete_access_is_optional(&self, access_index: usize) -> bool {
        self.complete_accesses_are_optional.get(access_index).copied().unwrap_or(false)
    }

    pub fn segment_index_at_cursor(reference_token: &str, cursor_character_offset: usize) -> Option<usize> {
        if reference_token.is_empty() {
            return None;
        }

        let token_characters = reference_token.chars().collect::<Vec<_>>();

        if cursor_character_offset >= token_characters.len() {
            return None;
        }

        if !is_identifier_character(token_characters[cursor_character_offset]) {
            return None;
        }

        let mut segment_index = 0_usize;
        let mut character_index = 0_usize;

        while character_index < token_characters.len() {
            if !is_identifier_character(token_characters[character_index]) {
                character_index += 1;
                continue;
            }

            let segment_start_index = character_index;

            while character_index < token_characters.len() && is_identifier_character(token_characters[character_index]) {
                character_index += 1;
            }

            if (segment_start_index..character_index).contains(&cursor_character_offset) {
                return Some(segment_index);
            }

            segment_index += 1;
        }

        None
    }

    pub fn resolved_accesses_through_segment(&self, segment_index: usize) -> Option<Vec<String>> {
        if segment_index == 0 {
            return Some(Vec::new());
        }

        let resolved_access_count = segment_index;
        let mut resolved_accesses = Vec::new();

        for complete_access in self.complete_accesses.iter().take(resolved_access_count) {
            resolved_accesses.push(complete_access.clone());
        }

        if resolved_accesses.len() < resolved_access_count
            && resolved_accesses.len() == self.complete_accesses.len()
            && !self.pending_prefix.is_empty()
        {
            resolved_accesses.push(self.pending_prefix.clone());
        }

        if resolved_accesses.len() == resolved_access_count {
            return Some(resolved_accesses);
        }

        None
    }

    fn root_declaration_keyword(&self) -> Option<DeclarationKeyword> {
        DeclarationKeyword::from_identifier(&self.root)
    }

    pub fn is_schema_root(&self) -> bool {
        self.root_declaration_keyword() == Some(DeclarationKeyword::Schema)
    }
}

#[derive(Debug, Clone)]
struct ParsedReferenceToken {
    root: String,
    accesses: Vec<ParsedReferenceAccess>,
    has_trailing_separator: bool,
    trailing_separator_is_optional: bool,
}

impl ParsedReferenceToken {
    fn parse(reference_token: &str) -> Option<Self> {
        let token_characters = reference_token.chars().collect::<Vec<_>>();
        let mut character_index = 0_usize;
        let root = Self::read_identifier(&token_characters, &mut character_index)?;
        let mut accesses = Vec::new();

        while character_index < token_characters.len() {
            let is_optional = token_characters.get(character_index) == Some(&'?');

            if is_optional {
                character_index += 1;
            }

            if token_characters.get(character_index) != Some(&'.') {
                return None;
            }

            character_index += 1;

            if character_index == token_characters.len() {
                return Some(Self {
                    root,
                    accesses,
                    has_trailing_separator: true,
                    trailing_separator_is_optional: is_optional,
                });
            }

            let access_name = Self::read_identifier(&token_characters, &mut character_index)?;
            accesses.push(ParsedReferenceAccess {
                name: access_name,
                is_optional,
            });
        }

        Some(Self {
            root,
            accesses,
            has_trailing_separator: false,
            trailing_separator_is_optional: false,
        })
    }

    fn read_identifier(token_characters: &[char], character_index: &mut usize) -> Option<String> {
        let start_index = *character_index;

        while *character_index < token_characters.len() && is_identifier_character(token_characters[*character_index]) {
            *character_index += 1;
        }

        if start_index == *character_index {
            return None;
        }

        Some(token_characters[start_index..*character_index].iter().collect())
    }
}

#[derive(Debug, Clone)]
struct ParsedReferenceAccess {
    name: String,
    is_optional: bool,
}

fn is_identifier_character(character: char) -> bool {
    character.is_ascii_alphanumeric() || character == '_'
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReferenceCompletionConstraint {
    None,
    ForLoopIterable,
    InferenceIntegerValue,
    InferenceNumericValue,
}

impl ReferenceCompletionConstraint {
    pub fn from_line_prefix(line_prefix: &str) -> Self {
        if is_for_loop_iterable_reference_context(line_prefix) {
            return Self::ForLoopIterable;
        }

        Self::None
    }
}

impl SemanticIndex {
    fn normalized_mcp_import_name(import_name: &str) -> String {
        McpServerLock::normalize_item_name(import_name)
    }

    fn mcp_tool_name_suggestions(&self, server_name: &str, pending_prefix: &str) -> Vec<CompletionSuggestion> {
        let Some(server_lock) = self.mcp_lock.as_ref().and_then(|mcp_lock| mcp_lock.servers.get(server_name)) else {
            return Vec::new();
        };

        let mut normalized_tool_names = server_lock
            .tools
            .keys()
            .map(|tool_name| Self::normalized_mcp_import_name(tool_name))
            .filter(|normalized_tool_name| normalized_tool_name.starts_with(pending_prefix))
            .collect::<Vec<_>>();

        normalized_tool_names.sort();
        normalized_tool_names.dedup();

        normalized_tool_names
            .into_iter()
            .map(|normalized_tool_name| CompletionSuggestion {
                label: normalized_tool_name.clone(),
                kind: CompletionKind::Value,
                detail: "MCP tool".to_string(),
                documentation: format!("MCP tool `{normalized_tool_name}` from server `{server_name}`."),
                insert_text: normalized_tool_name,
            })
            .collect()
    }

    fn mcp_resource_name_suggestions(&self, server_name: &str, pending_prefix: &str) -> Vec<CompletionSuggestion> {
        let Some(server_lock) = self.mcp_lock.as_ref().and_then(|mcp_lock| mcp_lock.servers.get(server_name)) else {
            return Vec::new();
        };

        let mut normalized_resource_names = server_lock
            .resources
            .iter()
            .map(|resource_name| Self::normalized_mcp_import_name(resource_name))
            .filter(|normalized_resource_name| normalized_resource_name.starts_with(pending_prefix))
            .collect::<Vec<_>>();

        normalized_resource_names.sort();
        normalized_resource_names.dedup();

        normalized_resource_names
            .into_iter()
            .map(|normalized_resource_name| CompletionSuggestion {
                label: normalized_resource_name.clone(),
                kind: CompletionKind::Value,
                detail: "MCP resource".to_string(),
                documentation: format!("MCP resource `{normalized_resource_name}` from server `{server_name}`."),
                insert_text: normalized_resource_name,
            })
            .collect()
    }

    fn mcp_prompt_name_suggestions(&self, server_name: &str, pending_prefix: &str) -> Vec<CompletionSuggestion> {
        let Some(server_lock) = self.mcp_lock.as_ref().and_then(|mcp_lock| mcp_lock.servers.get(server_name)) else {
            return Vec::new();
        };

        let mut normalized_prompt_names = server_lock
            .prompts
            .iter()
            .map(|prompt_name| Self::normalized_mcp_import_name(prompt_name))
            .filter(|normalized_prompt_name| normalized_prompt_name.starts_with(pending_prefix))
            .collect::<Vec<_>>();

        normalized_prompt_names.sort();
        normalized_prompt_names.dedup();

        normalized_prompt_names
            .into_iter()
            .map(|normalized_prompt_name| CompletionSuggestion {
                label: normalized_prompt_name.clone(),
                kind: CompletionKind::Value,
                detail: "MCP prompt".to_string(),
                documentation: format!("MCP prompt `{normalized_prompt_name}` from server `{server_name}`."),
                insert_text: normalized_prompt_name,
            })
            .collect()
    }

    pub fn reference_path_suggestions(
        &self,
        reference_completion_path: &ReferenceCompletionPath,
        reference_completion_constraint: ReferenceCompletionConstraint,
        position: Position,
        existing_tool_binding_block: bool,
    ) -> Vec<CompletionSuggestion> {
        let current_schema_name = self.schema_name_at_position(position);
        let current_agent_name = self.agent_name_at_position(position);

        if let Some(iterator_reference_suggestions) =
            self.for_loop_binding_reference_suggestions(reference_completion_path, reference_completion_constraint, position)
        {
            return iterator_reference_suggestions;
        }

        if reference_completion_path.is_schema_root() {
            if reference_completion_constraint == ReferenceCompletionConstraint::ForLoopIterable {
                return Vec::new();
            }

            return self.schema_reference_suggestions(reference_completion_path, current_schema_name);
        }

        if reference_completion_path.root_declaration_keyword() == Some(DeclarationKeyword::Mcp) {
            return self.mcp_namespace_reference_suggestions(reference_completion_path);
        }

        match reference_completion_path.root_keyword() {
            Some(ReferenceKeyword::Dynamic) => {
                self.dynamic_reference_suggestions(reference_completion_path, reference_completion_constraint, position)
            }
            Some(ReferenceKeyword::Input) => self.singleton_reference_suggestions(
                &self.input_fields,
                Some(&self.input_field_metadata),
                "Input field",
                reference_completion_constraint,
                reference_completion_path,
            ),
            Some(ReferenceKeyword::Secrets) => self.singleton_reference_suggestions(
                &self.secrets_fields,
                Some(&self.secrets_field_metadata),
                "Secrets field",
                reference_completion_constraint,
                reference_completion_path,
            ),
            Some(ReferenceKeyword::Agent) => {
                self.agent_reference_suggestions(reference_completion_path, reference_completion_constraint, current_agent_name)
            }
            Some(ReferenceKeyword::Tool) => {
                self.tool_namespace_reference_suggestions(reference_completion_path, existing_tool_binding_block)
            }
            Some(ReferenceKeyword::Resource) => {
                self.mcp_import_reference_suggestions(reference_completion_path, &self.resource_names, "Imported MCP resource")
            }
            Some(ReferenceKeyword::Prompt) => {
                self.mcp_import_reference_suggestions(reference_completion_path, &self.prompt_names, "Imported MCP prompt")
            }
            Some(ReferenceKeyword::Model) => Vec::new(),
            None => Vec::new(),
        }
    }

    fn mcp_import_reference_suggestions(
        &self,
        reference_completion_path: &ReferenceCompletionPath,
        import_names: &[String],
        detail: &str,
    ) -> Vec<CompletionSuggestion> {
        if !reference_completion_path.complete_accesses.is_empty() {
            return Vec::new();
        }

        import_names
            .iter()
            .filter(|import_name| import_name.starts_with(&reference_completion_path.pending_prefix))
            .map(|import_name| CompletionSuggestion {
                label: import_name.clone(),
                kind: CompletionKind::Value,
                detail: detail.to_string(),
                documentation: format!("Reference imported MCP item `{import_name}`."),
                insert_text: import_name.clone(),
            })
            .collect()
    }

    fn mcp_namespace_reference_suggestions(&self, reference_completion_path: &ReferenceCompletionPath) -> Vec<CompletionSuggestion> {
        let Some(mcp_lock) = &self.mcp_lock else {
            return Vec::new();
        };

        if reference_completion_path.complete_accesses.is_empty() {
            return mcp_lock
                .servers
                .keys()
                .filter(|server_name| server_name.starts_with(&reference_completion_path.pending_prefix))
                .map(|server_name| CompletionSuggestion {
                    label: server_name.clone(),
                    kind: CompletionKind::Module,
                    detail: "Declared MCP server".to_string(),
                    documentation: format!("MCP server `{server_name}` from lock file."),
                    insert_text: server_name.clone(),
                })
                .collect();
        }

        let server_name = &reference_completion_path.complete_accesses[0];
        let Some(_server_lock) = mcp_lock.servers.get(server_name) else {
            return Vec::new();
        };

        if reference_completion_path.complete_accesses.len() == 1 {
            return ["tool", "resource", "prompt"]
                .into_iter()
                .filter(|kind_name| kind_name.starts_with(&reference_completion_path.pending_prefix))
                .map(|kind_name| CompletionSuggestion {
                    label: kind_name.to_string(),
                    kind: CompletionKind::Module,
                    detail: "MCP import namespace".to_string(),
                    documentation: format!("Use `mcp.{server_name}.{kind_name}.<name>` for MCP imports."),
                    insert_text: kind_name.to_string(),
                })
                .collect();
        }

        if reference_completion_path.complete_accesses.len() > 2 {
            return Vec::new();
        }

        let import_kind = &reference_completion_path.complete_accesses[1];
        let pending_prefix = &reference_completion_path.pending_prefix;

        match import_kind.as_str() {
            "tool" => self.mcp_tool_name_suggestions(server_name, pending_prefix),
            "resource" => self.mcp_resource_name_suggestions(server_name, pending_prefix),
            "prompt" => self.mcp_prompt_name_suggestions(server_name, pending_prefix),
            _ => Vec::new(),
        }
    }

    fn tool_namespace_reference_suggestions(
        &self,
        reference_completion_path: &ReferenceCompletionPath,
        existing_tool_binding_block: bool,
    ) -> Vec<CompletionSuggestion> {
        if !reference_completion_path.complete_accesses.is_empty() {
            return Vec::new();
        }

        self.tool_reference_suggestions(&reference_completion_path.pending_prefix, existing_tool_binding_block)
    }

    fn dynamic_reference_suggestions(
        &self,
        reference_completion_path: &ReferenceCompletionPath,
        reference_completion_constraint: ReferenceCompletionConstraint,
        position: Position,
    ) -> Vec<CompletionSuggestion> {
        let (dynamic_fields, dynamic_field_metadata) = self.dynamic_scope_at_position(position);

        if reference_completion_path.complete_accesses.is_empty() {
            let dynamic_field_locations = self.dynamic_field_locations_at_position(position);
            let visible_dynamic_fields = dynamic_fields
                .iter()
                .filter(|(field_name, _)| {
                    dynamic_field_locations
                        .get(*field_name)
                        .is_none_or(|field_span| !source_span_contains_position(*field_span, position))
                })
                .map(|(field_name, field_type)| (field_name.clone(), field_type.clone()))
                .collect::<BTreeMap<_, _>>();
            let visible_dynamic_field_metadata = dynamic_field_metadata
                .iter()
                .filter(|(field_name, _)| visible_dynamic_fields.contains_key(*field_name))
                .map(|(field_name, field_metadata)| (field_name.clone(), field_metadata.clone()))
                .collect::<BTreeMap<_, _>>();

            return self.singleton_reference_suggestions(
                &visible_dynamic_fields,
                Some(&visible_dynamic_field_metadata),
                "Dynamic field",
                reference_completion_constraint,
                reference_completion_path,
            );
        }

        self.singleton_reference_suggestions(
            dynamic_fields,
            Some(dynamic_field_metadata),
            "Dynamic field",
            reference_completion_constraint,
            reference_completion_path,
        )
    }

    fn for_loop_binding_reference_suggestions(
        &self,
        reference_completion_path: &ReferenceCompletionPath,
        reference_completion_constraint: ReferenceCompletionConstraint,
        position: Position,
    ) -> Option<Vec<CompletionSuggestion>> {
        if reference_completion_path.root_keyword().is_some() {
            return None;
        }

        let for_loop_binding_types = self
            .for_loop_binding_types_at_position(position, reference_completion_path.root_identifier())?
            .to_vec();

        let candidate_types = if reference_completion_path.complete_accesses.is_empty() {
            for_loop_binding_types
        } else {
            self.tooling_snapshot
                .resolve_access_path_types(for_loop_binding_types, &reference_completion_path.complete_accesses)
        };

        if self.requires_optional_access_for_field_completion(candidate_types.as_slice(), reference_completion_path) {
            return Some(Vec::new());
        }

        Some(self.field_suggestions_from_types(
            candidate_types.as_slice(),
            &reference_completion_path.pending_prefix,
            reference_completion_constraint,
        ))
    }

    pub fn resolve_singleton_reference_type(
        &self,
        root_fields: &BTreeMap<String, TypeExpression>,
        resolved_accesses: &[String],
    ) -> Option<TypeExpression> {
        let first_field_name = resolved_accesses.first()?;
        let root_field_type = root_fields.get(first_field_name)?.clone();

        if resolved_accesses.len() == 1 {
            return Some(root_field_type);
        }

        let candidate_types = self
            .tooling_snapshot
            .resolve_access_path_types(vec![root_field_type], &resolved_accesses[1..]);

        candidate_types.first().cloned()
    }

    fn singleton_reference_suggestions(
        &self,
        root_fields: &BTreeMap<String, TypeExpression>,
        root_field_metadata: Option<&BTreeMap<String, FieldMetadata>>,
        detail_prefix: &str,
        reference_completion_constraint: ReferenceCompletionConstraint,
        reference_completion_path: &ReferenceCompletionPath,
    ) -> Vec<CompletionSuggestion> {
        let complete_accesses = reference_completion_path.complete_accesses.as_slice();
        let pending_prefix = reference_completion_path.pending_prefix.as_str();

        if complete_accesses.is_empty() {
            return root_fields
                .iter()
                .filter(|(field_name, _)| field_name.starts_with(pending_prefix))
                .filter(|(_, field_type)| self.type_matches_reference_constraint(field_type, reference_completion_constraint))
                .map(|(field_name, field_type)| CompletionSuggestion {
                    label: field_name.clone(),
                    kind: CompletionKind::Property,
                    detail: root_field_metadata
                        .and_then(|metadata_map| metadata_map.get(field_name))
                        .and_then(|field_metadata| field_metadata.description.clone())
                        .unwrap_or_else(|| format!("{detail_prefix}: {}", field_type.render_type())),
                    documentation: root_field_metadata
                        .and_then(|metadata_map| metadata_map.get(field_name))
                        .and_then(|field_metadata| field_metadata.description.clone())
                        .unwrap_or_else(|| "Field in singleton declaration.".to_string()),
                    insert_text: field_name.clone(),
                })
                .collect();
        }

        let first_field_name = &complete_accesses[0];
        let Some(root_field_type) = root_fields.get(first_field_name).cloned() else {
            return Vec::new();
        };

        let candidate_types = self
            .tooling_snapshot
            .resolve_access_path_types(vec![root_field_type], &complete_accesses[1..]);

        if self.requires_optional_access_for_field_completion(candidate_types.as_slice(), reference_completion_path) {
            return Vec::new();
        }

        self.field_suggestions_from_types(candidate_types.as_slice(), pending_prefix, reference_completion_constraint)
    }

    fn agent_reference_suggestions(
        &self,
        reference_completion_path: &ReferenceCompletionPath,
        reference_completion_constraint: ReferenceCompletionConstraint,
        current_agent_name: Option<&str>,
    ) -> Vec<CompletionSuggestion> {
        if reference_completion_path.complete_accesses.is_empty() {
            return self
                .agent_names
                .iter()
                .filter(|agent_name| agent_name.starts_with(&reference_completion_path.pending_prefix))
                .filter(|agent_name| current_agent_name.is_none_or(|current_name| *agent_name != current_name))
                .filter(|agent_name| {
                    let Some(agent_summary) = self.agents.get(*agent_name) else {
                        return false;
                    };

                    let Some(agent_output_type) = &agent_summary.output_type else {
                        return false;
                    };

                    self.type_matches_reference_constraint(agent_output_type, reference_completion_constraint)
                })
                .map(|agent_name| CompletionSuggestion {
                    label: agent_name.clone(),
                    kind: CompletionKind::Variable,
                    detail: "Declared agent".to_string(),
                    documentation: "Reference to a declared agent output.".to_string(),
                    insert_text: agent_name.clone(),
                })
                .collect();
        }

        let agent_name = &reference_completion_path.complete_accesses[0];

        if current_agent_name == Some(agent_name.as_str()) {
            return Vec::new();
        }

        let Some(agent_summary) = self.agents.get(agent_name) else {
            return Vec::new();
        };

        let Some(agent_output_type) = agent_summary.output_type.clone() else {
            return Vec::new();
        };

        let remaining_accesses = &reference_completion_path.complete_accesses[1..];

        if self.has_unsafe_nullable_access(agent_output_type.clone(), remaining_accesses, reference_completion_path, 1) {
            return Vec::new();
        }

        let candidate_types = self
            .tooling_snapshot
            .resolve_access_path_types(vec![agent_output_type], remaining_accesses);

        if self.requires_optional_access_for_field_completion(candidate_types.as_slice(), reference_completion_path) {
            return Vec::new();
        }

        self.field_suggestions_from_types(
            candidate_types.as_slice(),
            &reference_completion_path.pending_prefix,
            reference_completion_constraint,
        )
    }

    fn schema_reference_suggestions(
        &self,
        reference_completion_path: &ReferenceCompletionPath,
        current_schema_name: Option<&str>,
    ) -> Vec<CompletionSuggestion> {
        if reference_completion_path.complete_accesses.is_empty() {
            return self
                .schema_names
                .iter()
                .filter(|schema_name| current_schema_name.is_none_or(|current_name| *schema_name != current_name))
                .filter(|schema_name| schema_name.starts_with(&reference_completion_path.pending_prefix))
                .map(|schema_name| CompletionSuggestion {
                    label: schema_name.clone(),
                    kind: CompletionKind::Type,
                    detail: "Named schema".to_string(),
                    documentation: "Named schema type from this workflow.".to_string(),
                    insert_text: schema_name.clone(),
                })
                .collect();
        }

        let schema_name = &reference_completion_path.complete_accesses[0];

        if current_schema_name == Some(schema_name.as_str()) {
            return Vec::new();
        }

        let remaining_accesses = reference_completion_path.complete_accesses[1..].to_vec();
        let Some(schema_type) = self.schema_object_type(schema_name) else {
            return Vec::new();
        };

        if self.has_unsafe_nullable_access(schema_type, remaining_accesses.as_slice(), reference_completion_path, 1) {
            return Vec::new();
        }

        let candidate_types = self
            .tooling_snapshot
            .resolve_reference_path_types(&ToolingReferencePath::schema(schema_name.clone(), remaining_accesses));

        if self.requires_optional_access_for_field_completion(candidate_types.as_slice(), reference_completion_path) {
            return Vec::new();
        }

        self.field_suggestions_from_types(
            candidate_types.as_slice(),
            &reference_completion_path.pending_prefix,
            ReferenceCompletionConstraint::None,
        )
    }

    fn requires_optional_access_for_field_completion(
        &self,
        candidate_types: &[TypeExpression],
        reference_completion_path: &ReferenceCompletionPath,
    ) -> bool {
        if reference_completion_path.pending_access_is_optional {
            return false;
        }

        candidate_types.iter().any(TypeExpression::can_be_null)
    }

    fn has_unsafe_nullable_access(
        &self,
        root_type: TypeExpression,
        access_path_segments: &[String],
        reference_completion_path: &ReferenceCompletionPath,
        access_offset: usize,
    ) -> bool {
        let mut candidate_types = vec![root_type];

        for (access_path_index, access_path_segment) in access_path_segments.iter().enumerate() {
            let access_index = access_offset + access_path_index;

            if candidate_types.iter().any(TypeExpression::can_be_null)
                && !reference_completion_path.complete_access_is_optional(access_index)
            {
                return true;
            }

            candidate_types = self
                .tooling_snapshot
                .resolve_access_path_types(candidate_types, std::slice::from_ref(access_path_segment));

            if candidate_types.is_empty() {
                return false;
            }
        }

        false
    }

    fn schema_object_type(&self, schema_name: &str) -> Option<TypeExpression> {
        let schema_summary = self.schemas.get(schema_name)?;

        Some(TypeExpression::Object(
            schema_summary
                .field_metadata
                .iter()
                .map(|(field_name, field_metadata)| TypedField {
                    name: field_name.clone(),
                    field_type: field_metadata.field_type.clone(),
                    description: field_metadata.description.clone(),
                    span: SourceSpan {
                        start: SourcePosition { line: 1, column: 1 },
                        end: SourcePosition { line: 1, column: 1 },
                    },
                })
                .collect(),
        ))
    }

    fn field_suggestions_from_types(
        &self,
        candidate_types: &[TypeExpression],
        pending_prefix: &str,
        reference_completion_constraint: ReferenceCompletionConstraint,
    ) -> Vec<CompletionSuggestion> {
        let available_fields = self.available_fields_for_types(candidate_types);

        available_fields
            .into_iter()
            .filter(|(field_name, _)| field_name.starts_with(pending_prefix))
            .filter(|(_, field_metadata)| {
                self.type_matches_reference_constraint(&field_metadata.field_type, reference_completion_constraint)
            })
            .map(|(field_name, field_metadata)| CompletionSuggestion {
                label: field_name.clone(),
                kind: CompletionKind::Property,
                detail: field_metadata
                    .description
                    .clone()
                    .unwrap_or_else(|| format!("Field: {}", field_metadata.field_type.render_type())),
                documentation: field_metadata
                    .description
                    .unwrap_or_else(|| "Field available at this reference path.".to_string()),
                insert_text: field_name,
            })
            .collect()
    }

    fn available_fields_for_types(&self, candidate_types: &[TypeExpression]) -> BTreeMap<String, FieldMetadata> {
        let mut available_fields = BTreeMap::<String, FieldMetadata>::new();

        for candidate_type in candidate_types {
            self.collect_available_fields(candidate_type, &mut available_fields);
        }

        available_fields
    }

    fn collect_available_fields(&self, candidate_type: &TypeExpression, available_fields: &mut BTreeMap<String, FieldMetadata>) {
        match candidate_type {
            TypeExpression::Object(typed_fields) => {
                for typed_field in typed_fields {
                    available_fields.entry(typed_field.name.clone()).or_insert_with(|| FieldMetadata {
                        field_type: typed_field.field_type.clone(),
                        description: typed_field.description.clone(),
                    });
                }
            }
            TypeExpression::SchemaReference(schema_name) => {
                if let Some(schema_summary) = self.schemas.get(schema_name) {
                    for (field_name, field_metadata) in &schema_summary.field_metadata {
                        available_fields.entry(field_name.clone()).or_insert_with(|| field_metadata.clone());
                    }
                }
            }
            TypeExpression::Variant { discriminator, cases } => {
                available_fields.entry(discriminator.clone()).or_insert_with(|| FieldMetadata {
                    field_type: TypeExpression::Union(
                        cases
                            .iter()
                            .map(|variant_case| TypeExpression::StringEnum(variant_case.name.clone()))
                            .collect(),
                    ),
                    description: None,
                });
            }
            TypeExpression::Union(union_members) => {
                for union_member in union_members {
                    self.collect_available_fields(union_member, available_fields);
                }
            }
            TypeExpression::Array {
                item_type: _,
                fixed_length: _,
            }
            | TypeExpression::Tuple(_)
            | TypeExpression::String
            | TypeExpression::Number
            | TypeExpression::Float
            | TypeExpression::Boolean
            | TypeExpression::Null
            | TypeExpression::AnyObject
            | TypeExpression::StringEnum(_)
            | TypeExpression::StringEnumReference(_) => {}
        }
    }

    fn type_matches_reference_constraint(
        &self,
        field_type: &TypeExpression,
        reference_completion_constraint: ReferenceCompletionConstraint,
    ) -> bool {
        match reference_completion_constraint {
            ReferenceCompletionConstraint::None => true,
            ReferenceCompletionConstraint::ForLoopIterable => field_type.supports_for_loop_iterable(),
            ReferenceCompletionConstraint::InferenceIntegerValue => {
                self.type_supports_numeric_reference(field_type, NumericReferenceKind::Integer)
            }
            ReferenceCompletionConstraint::InferenceNumericValue => {
                self.type_supports_numeric_reference(field_type, NumericReferenceKind::Numeric)
            }
        }
    }

    fn type_supports_numeric_reference(&self, field_type: &TypeExpression, numeric_reference_kind: NumericReferenceKind) -> bool {
        self.type_supports_numeric_reference_with_visited(field_type, numeric_reference_kind, &mut HashSet::new())
    }

    fn type_supports_numeric_reference_with_visited(
        &self,
        field_type: &TypeExpression,
        numeric_reference_kind: NumericReferenceKind,
        visited_schema_names: &mut HashSet<String>,
    ) -> bool {
        match field_type {
            TypeExpression::Number => true,
            TypeExpression::Float => numeric_reference_kind == NumericReferenceKind::Numeric,
            TypeExpression::Object(object_fields) => object_fields.iter().any(|typed_field| {
                self.type_supports_numeric_reference_with_visited(&typed_field.field_type, numeric_reference_kind, visited_schema_names)
            }),
            TypeExpression::Variant { discriminator: _, cases } => cases.iter().any(|variant_case| {
                variant_case.fields.iter().any(|typed_field| {
                    self.type_supports_numeric_reference_with_visited(&typed_field.field_type, numeric_reference_kind, visited_schema_names)
                })
            }),
            TypeExpression::SchemaReference(schema_name) => {
                if !visited_schema_names.insert(schema_name.clone()) {
                    return false;
                }

                let supports_numeric_reference = self.schemas.get(schema_name).is_some_and(|schema_summary| {
                    schema_summary.fields.values().any(|schema_field_type| {
                        self.type_supports_numeric_reference_with_visited(schema_field_type, numeric_reference_kind, visited_schema_names)
                    })
                });

                let _ = visited_schema_names.remove(schema_name);

                supports_numeric_reference
            }
            TypeExpression::Union(union_members) => union_members.iter().any(|union_member| {
                self.type_supports_numeric_reference_with_visited(union_member, numeric_reference_kind, visited_schema_names)
            }),
            TypeExpression::String
            | TypeExpression::Boolean
            | TypeExpression::Null
            | TypeExpression::AnyObject
            | TypeExpression::StringEnum(_)
            | TypeExpression::StringEnumReference(_)
            | TypeExpression::Array {
                item_type: _,
                fixed_length: _,
            }
            | TypeExpression::Tuple(_) => false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NumericReferenceKind {
    Integer,
    Numeric,
}

trait ForLoopIterableType {
    fn supports_for_loop_iterable(&self) -> bool;
}

impl ForLoopIterableType for TypeExpression {
    fn supports_for_loop_iterable(&self) -> bool {
        match self {
            TypeExpression::Array {
                item_type: _,
                fixed_length: _,
            } => true,
            TypeExpression::Union(union_members) => union_members.iter().any(ForLoopIterableType::supports_for_loop_iterable),
            TypeExpression::String
            | TypeExpression::Number
            | TypeExpression::Float
            | TypeExpression::Boolean
            | TypeExpression::Null
            | TypeExpression::AnyObject
            | TypeExpression::SchemaReference(_)
            | TypeExpression::StringEnum(_)
            | TypeExpression::StringEnumReference(_)
            | TypeExpression::Tuple(_)
            | TypeExpression::Object(_)
            | TypeExpression::Variant {
                discriminator: _,
                cases: _,
            } => false,
        }
    }
}

fn is_for_loop_iterable_reference_context(line_prefix: &str) -> bool {
    for_clause_iterable_prefix(line_prefix).is_some()
}
