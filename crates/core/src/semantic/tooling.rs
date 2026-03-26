use crate::dsl::{
    parse_workflow, Declaration, DeclarationKeyword, SingletonDeclarationKind, SourcePosition, SourceSpan, TypeExpression, TypedField,
    Workflow,
};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ToolingSymbolCategory {
    Provider,
    Schema,
    Agent,
}

impl ToolingSymbolCategory {
    #[must_use]
    pub fn declaration_keyword(self) -> DeclarationKeyword {
        match self {
            Self::Provider => DeclarationKeyword::Provider,
            Self::Schema => DeclarationKeyword::Schema,
            Self::Agent => DeclarationKeyword::Agent,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NamedSymbolSpan {
    pub category: ToolingSymbolCategory,
    pub name: String,
    pub span: SourceSpan,
}

impl NamedSymbolSpan {
    #[must_use]
    pub fn contains_position(&self, source_position: SourcePosition) -> bool {
        self.span.contains_position(source_position)
    }
}

#[derive(Debug, Clone, Default)]
pub struct ToolingDeclarationIndex {
    symbols: Vec<NamedSymbolSpan>,
}

impl ToolingDeclarationIndex {
    fn push_symbol(&mut self, category: ToolingSymbolCategory, name: impl Into<String>, span: SourceSpan) {
        self.symbols.push(NamedSymbolSpan {
            category,
            name: name.into(),
            span,
        });
    }

    #[must_use]
    pub fn all_symbols(&self) -> &[NamedSymbolSpan] {
        &self.symbols
    }

    pub fn symbols_by_category(&self, category: ToolingSymbolCategory) -> impl Iterator<Item = &NamedSymbolSpan> + Clone {
        self.symbols.iter().filter(move |named_symbol| named_symbol.category == category)
    }

    #[must_use]
    pub fn symbol_span(&self, category: ToolingSymbolCategory, symbol_name: &str) -> Option<SourceSpan> {
        self.symbols_by_category(category)
            .find(|named_symbol| named_symbol.name == symbol_name)
            .map(|named_symbol| named_symbol.span)
    }

    #[must_use]
    pub fn symbol_name_at_position(&self, category: ToolingSymbolCategory, source_position: SourcePosition) -> Option<&str> {
        self.symbols_by_category(category)
            .find(|named_symbol| named_symbol.contains_position(source_position))
            .map(|named_symbol| named_symbol.name.as_str())
    }

    #[must_use]
    pub fn provider_name_at_position(&self, source_position: SourcePosition) -> Option<&str> {
        self.symbol_name_at_position(ToolingSymbolCategory::Provider, source_position)
    }

    #[must_use]
    pub fn schema_name_at_position(&self, source_position: SourcePosition) -> Option<&str> {
        self.symbol_name_at_position(ToolingSymbolCategory::Schema, source_position)
    }

    #[must_use]
    pub fn agent_name_at_position(&self, source_position: SourcePosition) -> Option<&str> {
        self.symbol_name_at_position(ToolingSymbolCategory::Agent, source_position)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ToolingReferencePathRoot {
    Input,
    Secrets,
    Agent,
    Schema,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolingReferencePath {
    root: ToolingReferencePathRoot,
    segments: Vec<String>,
}

impl ToolingReferencePath {
    #[must_use]
    pub fn input(field_accesses: Vec<String>) -> Self {
        Self {
            root: ToolingReferencePathRoot::Input,
            segments: field_accesses,
        }
    }

    #[must_use]
    pub fn secrets(field_accesses: Vec<String>) -> Self {
        Self {
            root: ToolingReferencePathRoot::Secrets,
            segments: field_accesses,
        }
    }

    #[must_use]
    pub fn agent(agent_name: impl Into<String>, field_accesses: Vec<String>) -> Self {
        let mut segments = Vec::with_capacity(field_accesses.len().saturating_add(1));
        segments.push(agent_name.into());
        segments.extend(field_accesses);

        Self {
            root: ToolingReferencePathRoot::Agent,
            segments,
        }
    }

    #[must_use]
    pub fn schema(schema_name: impl Into<String>, field_accesses: Vec<String>) -> Self {
        let mut segments = Vec::with_capacity(field_accesses.len().saturating_add(1));
        segments.push(schema_name.into());
        segments.extend(field_accesses);

        Self {
            root: ToolingReferencePathRoot::Schema,
            segments,
        }
    }

    #[must_use]
    pub fn root(&self) -> ToolingReferencePathRoot {
        self.root
    }

    #[must_use]
    pub fn segments(&self) -> &[String] {
        &self.segments
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolingSnapshotConstruction {
    ParsedWorkflow,
    TolerantSourceFallback,
}

#[derive(Debug, Clone)]
pub struct SemanticToolingSnapshot {
    declaration_index: ToolingDeclarationIndex,
    input_fields: BTreeMap<String, TypeExpression>,
    secrets_fields: BTreeMap<String, TypeExpression>,
    schemas: BTreeMap<String, BTreeMap<String, TypeExpression>>,
    agents: BTreeMap<String, Option<TypeExpression>>,
    construction: ToolingSnapshotConstruction,
    parse_error_span: Option<SourceSpan>,
}

impl SemanticToolingSnapshot {
    #[must_use]
    pub fn from_workflow(workflow: &Workflow) -> Self {
        let mut declaration_index = ToolingDeclarationIndex::default();
        let mut input_fields = BTreeMap::new();
        let mut secrets_fields = BTreeMap::new();
        let mut schemas = BTreeMap::new();
        let mut agents = BTreeMap::new();

        for declaration in workflow.declarations() {
            match declaration {
                Declaration::Provider(provider_declaration) => {
                    declaration_index.push_symbol(
                        ToolingSymbolCategory::Provider,
                        provider_declaration.name.clone(),
                        provider_declaration.span,
                    );
                }
                Declaration::Schema(schema_declaration) => {
                    declaration_index.push_symbol(
                        ToolingSymbolCategory::Schema,
                        schema_declaration.name.clone(),
                        schema_declaration.span,
                    );
                    schemas.insert(schema_declaration.name.clone(), typed_fields_to_map(&schema_declaration.fields));
                }
                Declaration::Input(input_declaration) => {
                    if input_fields.is_empty() {
                        input_fields = typed_fields_to_map(&input_declaration.fields);
                    }
                }
                Declaration::Secrets(secrets_declaration) => {
                    if secrets_fields.is_empty() {
                        secrets_fields = typed_fields_to_map(&secrets_declaration.fields);
                    }
                }
                Declaration::Agent(agent_declaration) => {
                    declaration_index.push_symbol(ToolingSymbolCategory::Agent, agent_declaration.name.clone(), agent_declaration.span);
                    agents.insert(agent_declaration.name.clone(), agent_declaration.output_type().cloned());
                }
                Declaration::Output(_) => {}
            }
        }

        Self {
            declaration_index,
            input_fields,
            secrets_fields,
            schemas,
            agents,
            construction: ToolingSnapshotConstruction::ParsedWorkflow,
            parse_error_span: None,
        }
    }

    #[must_use]
    pub fn from_source_tolerant(source_text: &str) -> Self {
        match parse_workflow(source_text) {
            Ok(workflow) => Self::from_workflow(&workflow),
            Err(parse_error) => {
                let parse_error_span = parse_error.span();

                if let Some(snapshot_from_prefix) = Self::from_prefix_before_parse_error(source_text, parse_error_span) {
                    return snapshot_from_prefix;
                }

                TolerantSourceExtractor::new(source_text).into_snapshot(parse_error_span)
            }
        }
    }

    fn from_prefix_before_parse_error(source_text: &str, parse_error_span: Option<SourceSpan>) -> Option<Self> {
        let parse_error_span = parse_error_span?;
        let parse_error_byte_offset = parse_error_span.start.to_byte_offset(source_text)?;
        let source_prefix = source_text.get(..parse_error_byte_offset)?;
        let parsed_prefix_workflow = parse_workflow(source_prefix).ok()?;
        let mut parsed_prefix_snapshot = Self::from_workflow(&parsed_prefix_workflow);

        parsed_prefix_snapshot.construction = ToolingSnapshotConstruction::TolerantSourceFallback;
        parsed_prefix_snapshot.parse_error_span = Some(parse_error_span);

        Some(parsed_prefix_snapshot)
    }

    #[must_use]
    pub fn declaration_index(&self) -> &ToolingDeclarationIndex {
        &self.declaration_index
    }

    #[must_use]
    pub fn input_fields(&self) -> &BTreeMap<String, TypeExpression> {
        &self.input_fields
    }

    #[must_use]
    pub fn secrets_fields(&self) -> &BTreeMap<String, TypeExpression> {
        &self.secrets_fields
    }

    #[must_use]
    pub fn schemas(&self) -> &BTreeMap<String, BTreeMap<String, TypeExpression>> {
        &self.schemas
    }

    #[must_use]
    pub fn agents(&self) -> &BTreeMap<String, Option<TypeExpression>> {
        &self.agents
    }

    #[must_use]
    pub fn construction(&self) -> ToolingSnapshotConstruction {
        self.construction
    }

    #[must_use]
    pub fn parse_error_span(&self) -> Option<SourceSpan> {
        self.parse_error_span
    }

    #[must_use]
    pub fn symbol_span(&self, category: ToolingSymbolCategory, symbol_name: &str) -> Option<SourceSpan> {
        self.declaration_index.symbol_span(category, symbol_name)
    }

    #[must_use]
    pub fn provider_name_at_position(&self, source_position: SourcePosition) -> Option<&str> {
        self.declaration_index.provider_name_at_position(source_position)
    }

    #[must_use]
    pub fn schema_name_at_position(&self, source_position: SourcePosition) -> Option<&str> {
        self.declaration_index.schema_name_at_position(source_position)
    }

    #[must_use]
    pub fn agent_name_at_position(&self, source_position: SourcePosition) -> Option<&str> {
        self.declaration_index.agent_name_at_position(source_position)
    }

    #[must_use]
    pub fn resolve_reference_path_types(&self, reference_path: &ToolingReferencePath) -> Vec<TypeExpression> {
        let Some((root_type, remaining_accesses)) = self.root_type_for_reference_path(reference_path) else {
            return Vec::new();
        };

        if remaining_accesses.is_empty() {
            return vec![root_type];
        }

        self.resolve_access_path_types(vec![root_type], remaining_accesses)
    }

    #[must_use]
    pub fn resolve_reference_path_type(&self, reference_path: &ToolingReferencePath) -> Option<TypeExpression> {
        self.resolve_reference_path_types(reference_path).into_iter().next()
    }

    #[must_use]
    pub fn resolve_access_path_types(&self, start_types: Vec<TypeExpression>, access_path_segments: &[String]) -> Vec<TypeExpression> {
        let mut candidate_types = start_types;

        for access_path_segment in access_path_segments {
            let mut next_candidate_types = Vec::<TypeExpression>::new();

            for candidate_type in &candidate_types {
                candidate_type.collect_next_types_for_field(self, access_path_segment, &mut next_candidate_types);
            }

            if next_candidate_types.is_empty() {
                return Vec::new();
            }

            candidate_types = next_candidate_types;
        }

        candidate_types
    }

    #[must_use]
    pub fn available_fields_for_types(&self, candidate_types: &[TypeExpression]) -> BTreeMap<String, TypeExpression> {
        let mut available_fields = BTreeMap::<String, TypeExpression>::new();

        for candidate_type in candidate_types {
            candidate_type.collect_available_fields(self, &mut available_fields);
        }

        available_fields
    }

    fn root_type_for_reference_path<'path>(
        &self,
        reference_path: &'path ToolingReferencePath,
    ) -> Option<(TypeExpression, &'path [String])> {
        match reference_path.root() {
            ToolingReferencePathRoot::Input => {
                let first_segment = reference_path.segments().first()?;
                let root_type = self.input_fields.get(first_segment)?.clone();
                let remaining_accesses = &reference_path.segments()[1..];

                Some((root_type, remaining_accesses))
            }
            ToolingReferencePathRoot::Secrets => {
                let first_segment = reference_path.segments().first()?;
                let root_type = self.secrets_fields.get(first_segment)?.clone();
                let remaining_accesses = &reference_path.segments()[1..];

                Some((root_type, remaining_accesses))
            }
            ToolingReferencePathRoot::Agent => {
                let agent_name = reference_path.segments().first()?;
                let root_type = self.agents.get(agent_name)?.clone()?;
                let remaining_accesses = &reference_path.segments()[1..];

                Some((root_type, remaining_accesses))
            }
            ToolingReferencePathRoot::Schema => {
                let schema_name = reference_path.segments().first()?;
                let root_type = self.schema_object_type(schema_name)?;
                let remaining_accesses = &reference_path.segments()[1..];

                Some((root_type, remaining_accesses))
            }
        }
    }

    fn schema_object_type(&self, schema_name: &str) -> Option<TypeExpression> {
        let schema_fields = self.schemas.get(schema_name)?;

        Some(TypeExpression::Object(
            schema_fields
                .iter()
                .map(|(field_name, field_type)| TypedField {
                    name: field_name.clone(),
                    field_type: field_type.clone(),
                    description: None,
                    span: SourceSpan {
                        start: SourcePosition { line: 1, column: 1 },
                        end: SourcePosition { line: 1, column: 1 },
                    },
                })
                .collect(),
        ))
    }
}

impl SourceSpan {
    #[must_use]
    pub fn contains_position(self, source_position: SourcePosition) -> bool {
        let starts_before_or_at = (self.start.line < source_position.line)
            || (self.start.line == source_position.line && self.start.column <= source_position.column);

        let ends_after_or_at =
            (self.end.line > source_position.line) || (self.end.line == source_position.line && self.end.column >= source_position.column);

        starts_before_or_at && ends_after_or_at
    }
}

impl TypeExpression {
    fn collect_next_types_for_field(
        &self,
        tooling_snapshot: &SemanticToolingSnapshot,
        field_name: &str,
        next_candidate_types: &mut Vec<TypeExpression>,
    ) {
        match self {
            TypeExpression::Object(typed_fields) => {
                if let Some(typed_field) = typed_fields.iter().find(|typed_field| typed_field.name == field_name) {
                    next_candidate_types.push(typed_field.field_type.clone());
                }
            }
            TypeExpression::SchemaReference(schema_name) => {
                let Some(schema_fields) = tooling_snapshot.schemas.get(schema_name) else {
                    return;
                };

                if let Some(field_type) = schema_fields.get(field_name) {
                    next_candidate_types.push(field_type.clone());
                }
            }
            TypeExpression::Union(union_members) => {
                for union_member in union_members {
                    union_member.collect_next_types_for_field(tooling_snapshot, field_name, next_candidate_types);
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
            | TypeExpression::StringEnum(_) => {}
        }
    }

    fn collect_available_fields(
        &self,
        tooling_snapshot: &SemanticToolingSnapshot,
        available_fields: &mut BTreeMap<String, TypeExpression>,
    ) {
        match self {
            TypeExpression::Object(typed_fields) => {
                for typed_field in typed_fields {
                    available_fields
                        .entry(typed_field.name.clone())
                        .or_insert_with(|| typed_field.field_type.clone());
                }
            }
            TypeExpression::SchemaReference(schema_name) => {
                let Some(schema_fields) = tooling_snapshot.schemas.get(schema_name) else {
                    return;
                };

                for (field_name, field_type) in schema_fields {
                    available_fields.entry(field_name.clone()).or_insert_with(|| field_type.clone());
                }
            }
            TypeExpression::Union(union_members) => {
                for union_member in union_members {
                    union_member.collect_available_fields(tooling_snapshot, available_fields);
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
            | TypeExpression::StringEnum(_) => {}
        }
    }
}

struct TolerantSourceExtractor<'source> {
    source_text: &'source str,
}

impl<'source> TolerantSourceExtractor<'source> {
    fn new(source_text: &'source str) -> Self {
        Self { source_text }
    }

    fn into_snapshot(self, parse_error_span: Option<SourceSpan>) -> SemanticToolingSnapshot {
        let mut declaration_index = ToolingDeclarationIndex::default();

        for category in [
            ToolingSymbolCategory::Provider,
            ToolingSymbolCategory::Schema,
            ToolingSymbolCategory::Agent,
        ] {
            for named_symbol in self.collect_named_symbols(category) {
                declaration_index.push_symbol(category, named_symbol.name, named_symbol.span);
            }
        }

        let schemas = declaration_index
            .symbols_by_category(ToolingSymbolCategory::Schema)
            .map(|named_symbol| (named_symbol.name.clone(), BTreeMap::new()))
            .collect();

        let agents = declaration_index
            .symbols_by_category(ToolingSymbolCategory::Agent)
            .map(|named_symbol| (named_symbol.name.clone(), None))
            .collect();

        SemanticToolingSnapshot {
            declaration_index,
            input_fields: self.collect_singleton_field_types(SingletonDeclarationKind::Input),
            secrets_fields: self.collect_singleton_field_types(SingletonDeclarationKind::Secrets),
            schemas,
            agents,
            construction: ToolingSnapshotConstruction::TolerantSourceFallback,
            parse_error_span,
        }
    }

    fn collect_named_symbols(&self, category: ToolingSymbolCategory) -> Vec<NamedSymbolSpan> {
        let mut named_symbols = Vec::new();
        let declaration_keyword = category.declaration_keyword().as_str();

        for (line_index, source_line) in self.source_text.lines().enumerate() {
            let leading_whitespace_characters = source_line.chars().take_while(|character| character.is_whitespace()).count();
            let trimmed_line = source_line.trim_start();

            let Some(line_after_keyword) = trimmed_line.strip_prefix(declaration_keyword) else {
                continue;
            };

            if !line_after_keyword.starts_with(char::is_whitespace) {
                continue;
            }

            let whitespace_after_keyword_characters = line_after_keyword.chars().take_while(|character| character.is_whitespace()).count();
            let declaration_name = line_after_keyword
                .trim_start()
                .chars()
                .take_while(|character| Self::is_identifier_character(*character))
                .collect::<String>();

            if declaration_name.is_empty() {
                continue;
            }

            let symbol_start_column =
                leading_whitespace_characters + declaration_keyword.chars().count() + whitespace_after_keyword_characters + 1;
            let symbol_end_column = symbol_start_column + declaration_name.chars().count().saturating_sub(1);
            let symbol_span = self.declaration_symbol_span(line_index, symbol_start_column, symbol_end_column);

            named_symbols.push(NamedSymbolSpan {
                category,
                name: declaration_name,
                span: symbol_span,
            });
        }

        named_symbols
    }

    fn declaration_symbol_span(
        &self,
        declaration_line_index: usize,
        declaration_name_start_column: usize,
        declaration_name_end_column: usize,
    ) -> SourceSpan {
        let declaration_end_position = self.declaration_end_position(declaration_line_index, declaration_name_end_column);

        SourceSpan {
            start: SourcePosition {
                line: declaration_line_index + 1,
                column: declaration_name_start_column,
            },
            end: declaration_end_position,
        }
    }

    fn declaration_end_position(&self, declaration_line_index: usize, declaration_name_end_column: usize) -> SourcePosition {
        let mut inside_string_literal = false;
        let mut escaping_character = false;
        let mut seen_open_brace = false;
        let mut declaration_block_depth = 0_i32;
        let mut last_line_position = SourcePosition {
            line: declaration_line_index + 1,
            column: declaration_name_end_column.max(1),
        };

        for (line_offset, source_line) in self.source_text.lines().skip(declaration_line_index).enumerate() {
            let current_line_index = declaration_line_index + line_offset;
            let current_line_character_count = source_line.chars().count();

            last_line_position = SourcePosition {
                line: current_line_index + 1,
                column: current_line_character_count.max(1),
            };

            for (character_index, character) in source_line.chars().enumerate() {
                let current_column = character_index + 1;

                if current_line_index == declaration_line_index && current_column <= declaration_name_end_column {
                    continue;
                }

                if inside_string_literal {
                    if escaping_character {
                        escaping_character = false;
                        continue;
                    }

                    if character == '\\' {
                        escaping_character = true;
                        continue;
                    }

                    if character == '"' {
                        inside_string_literal = false;
                    }

                    continue;
                }

                if character == '"' {
                    inside_string_literal = true;
                    continue;
                }

                if character == '{' {
                    seen_open_brace = true;
                    declaration_block_depth += 1;
                    continue;
                }

                if character == '}' {
                    if !seen_open_brace {
                        continue;
                    }

                    declaration_block_depth -= 1;

                    if declaration_block_depth <= 0 {
                        return SourcePosition {
                            line: current_line_index + 1,
                            column: current_column,
                        };
                    }
                }
            }
        }

        if seen_open_brace {
            return last_line_position;
        }

        SourcePosition {
            line: declaration_line_index + 1,
            column: declaration_name_end_column.max(1),
        }
    }

    fn collect_singleton_field_types(&self, singleton_declaration_kind: SingletonDeclarationKind) -> BTreeMap<String, TypeExpression> {
        let block_name = singleton_declaration_kind.as_str();
        let mut field_types = BTreeMap::new();
        let mut inside_block = false;
        let mut brace_depth = 0_isize;

        for source_line in self.source_text.lines() {
            let trimmed_line = source_line.trim();

            if !inside_block {
                let starts_named_block =
                    trimmed_line.starts_with(block_name) && trimmed_line[block_name.len()..].trim_start().starts_with('{');

                if starts_named_block {
                    inside_block = true;
                    brace_depth = 1;
                }

                continue;
            }

            if brace_depth == 1 {
                if let Some(field_name) = self.line_field_name(trimmed_line) {
                    field_types.insert(field_name, TypeExpression::String);
                }
            }

            let open_brace_count =
                isize::try_from(trimmed_line.chars().filter(|character| *character == '{').count()).unwrap_or(isize::MAX);
            let close_brace_count =
                isize::try_from(trimmed_line.chars().filter(|character| *character == '}').count()).unwrap_or(isize::MAX);

            brace_depth = brace_depth.saturating_add(open_brace_count);
            brace_depth = brace_depth.saturating_sub(close_brace_count);

            if brace_depth <= 0 {
                inside_block = false;
                brace_depth = 0;
            }
        }

        field_types
    }

    fn line_field_name(&self, source_line: &str) -> Option<String> {
        let (field_name, _) = source_line.split_once(':')?;
        let field_name = field_name.trim();

        if !field_name.chars().all(Self::is_identifier_character) {
            return None;
        }

        Some(field_name.to_string())
    }

    fn is_identifier_character(character: char) -> bool {
        character.is_ascii_alphanumeric() || character == '_'
    }
}

fn typed_fields_to_map(typed_fields: &[TypedField]) -> BTreeMap<String, TypeExpression> {
    typed_fields
        .iter()
        .map(|typed_field| (typed_field.name.clone(), typed_field.field_type.clone()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{SemanticToolingSnapshot, ToolingReferencePath, ToolingSnapshotConstruction, ToolingSymbolCategory};
    use crate::dsl::{parse_workflow, TypeExpression};
    use crate::{parse_inline_workflow, workflow_source};

    #[test]
    fn snapshot_indexes_declaration_spans_and_position_lookups() {
        let workflow = parse_inline_workflow! {
            provider openai {
                driver: "openai"
                models: ["gpt-4o"]
            }

            schema Report {
                title: string
            }

            agent writer {
                model: openai("gpt-4o")
                prompt: "Write"
                output: schema.Report
            }

            output {
                value: agent.writer.title
            }
        };

        let tooling_snapshot = SemanticToolingSnapshot::from_workflow(&workflow);
        let provider_span = tooling_snapshot
            .symbol_span(ToolingSymbolCategory::Provider, "openai")
            .expect("provider symbol should exist");
        let schema_span = tooling_snapshot
            .symbol_span(ToolingSymbolCategory::Schema, "Report")
            .expect("schema symbol should exist");
        let agent_span = tooling_snapshot
            .symbol_span(ToolingSymbolCategory::Agent, "writer")
            .expect("agent symbol should exist");

        assert_eq!(tooling_snapshot.provider_name_at_position(provider_span.start), Some("openai"));
        assert_eq!(tooling_snapshot.schema_name_at_position(schema_span.start), Some("Report"));
        assert_eq!(tooling_snapshot.agent_name_at_position(agent_span.start), Some("writer"));
    }

    #[test]
    fn snapshot_resolves_reference_path_types_for_input_agent_and_schema_paths() {
        let workflow = parse_inline_workflow! {
            input {
                topic: string
            }

            schema Report {
                title: string
            }

            agent writer {
                model: openai("gpt-4o")
                prompt: input.topic
                output: schema.Report
            }

            output {
                value: agent.writer.title
            }
        };

        let tooling_snapshot = SemanticToolingSnapshot::from_workflow(&workflow);

        let input_topic_path = ToolingReferencePath::input(vec!["topic".to_string()]);
        let schema_title_path = ToolingReferencePath::schema("Report", vec!["title".to_string()]);
        let agent_title_path = ToolingReferencePath::agent("writer", vec!["title".to_string()]);

        assert_eq!(
            tooling_snapshot.resolve_reference_path_type(&input_topic_path),
            Some(TypeExpression::String)
        );
        assert_eq!(
            tooling_snapshot.resolve_reference_path_type(&schema_title_path),
            Some(TypeExpression::String)
        );
        assert_eq!(
            tooling_snapshot.resolve_reference_path_type(&agent_title_path),
            Some(TypeExpression::String)
        );
    }

    #[test]
    fn tolerant_source_snapshot_recovers_symbols_and_singleton_fields_after_parse_failure() {
        let broken_source = workflow_source! {
            provider openai {
                driver: "openai"
            }

            input {
                topic: string
            }

            schema Report {
                title: string
            }

            agent writer {
                model: openai("gpt-4o")
                prompt: input.topic
            }

            @
        };

        let expected_parse_error_span = parse_workflow(broken_source)
            .expect_err("source should fail parsing")
            .span()
            .expect("parse error should expose span");

        let tooling_snapshot = SemanticToolingSnapshot::from_source_tolerant(broken_source);

        assert_eq!(tooling_snapshot.construction(), ToolingSnapshotConstruction::TolerantSourceFallback);
        assert_eq!(tooling_snapshot.parse_error_span(), Some(expected_parse_error_span));
        assert!(tooling_snapshot.symbol_span(ToolingSymbolCategory::Provider, "openai").is_some());

        let schema_span = tooling_snapshot
            .symbol_span(ToolingSymbolCategory::Schema, "Report")
            .expect("schema symbol should exist");

        assert!(tooling_snapshot.symbol_span(ToolingSymbolCategory::Agent, "writer").is_some());
        assert!(schema_span.end.line > schema_span.start.line);

        let topic_type = tooling_snapshot.resolve_reference_path_type(&ToolingReferencePath::input(vec!["topic".to_string()]));

        assert_eq!(topic_type, Some(TypeExpression::String));

        let schema_name = tooling_snapshot.schema_name_at_position(schema_span.start);

        assert_eq!(schema_name, Some("Report"));

        let provider_span = tooling_snapshot
            .symbol_span(ToolingSymbolCategory::Provider, "openai")
            .expect("provider symbol should exist");

        let provider_name = tooling_snapshot.provider_name_at_position(provider_span.start);

        assert_eq!(provider_name, Some("openai"));
    }
}
