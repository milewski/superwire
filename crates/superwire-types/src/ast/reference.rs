use super::{DeclarationKeyword, ReferenceKeyword, ScalarTypeKeyword, SourceSpan, TypeExpression};
use std::collections::HashSet;
use std::hash::BuildHasher;

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

            return Some(match ScalarTypeKeyword::from_identifier(identifier) {
                Some(ScalarTypeKeyword::String) => TypeExpression::String,
                Some(ScalarTypeKeyword::Number) => TypeExpression::Number,
                Some(ScalarTypeKeyword::Float) => TypeExpression::Float,
                Some(ScalarTypeKeyword::Boolean) => TypeExpression::Boolean,
                Some(ScalarTypeKeyword::Object) => TypeExpression::AnyObject,
                Some(ScalarTypeKeyword::Null) => TypeExpression::Null,
                None => TypeExpression::StringEnumReference(self.clone()),
            });
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
    pub fn root_identifier(&self) -> Option<&str> {
        self.root.as_identifier()
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
    pub fn is_secret_reference(&self) -> bool {
        self.is_keyword_root(ReferenceKeyword::Secrets)
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
    pub fn access_fields_through_count(&self, access_count: usize) -> Option<Vec<&str>> {
        if access_count > self.accesses.len() {
            return None;
        }

        Some(
            self.accesses
                .iter()
                .take(access_count)
                .map(|reference_access| reference_access.field.as_str())
                .collect(),
        )
    }

    #[must_use]
    pub fn last_access(&self) -> Option<&ReferenceAccess> {
        self.accesses.last()
    }

    #[must_use]
    pub fn accesses_from(&self, start_index: usize) -> &[ReferenceAccess] {
        self.accesses.get(start_index..).unwrap_or(&[])
    }

    #[must_use]
    pub fn projection_accesses(&self) -> &[ReferenceAccess] {
        self.accesses_from(1)
    }

    #[must_use]
    pub fn first_projection_access(&self) -> Option<&ReferenceAccess> {
        self.projection_accesses().first()
    }

    #[must_use]
    pub fn has_accesses(&self) -> bool {
        !self.accesses.is_empty()
    }

    #[must_use]
    pub fn has_single_access(&self) -> bool {
        self.accesses.len() == 1
    }

    #[must_use]
    pub fn is_direct_required_reference_to_keyword(&self, reference_keyword: ReferenceKeyword) -> bool {
        self.direct_required_name_for_keyword(reference_keyword).is_some()
    }

    #[must_use]
    pub fn direct_required_name_for_keyword(&self, reference_keyword: ReferenceKeyword) -> Option<&str> {
        if self.root_keyword() != Some(reference_keyword) {
            return None;
        }

        let [reference_access] = self.accesses.as_slice() else {
            return None;
        };

        if reference_access.is_optional() {
            return None;
        }

        Some(reference_access.field.as_str())
    }

    #[must_use]
    pub fn tool_name(&self) -> Option<&str> {
        self.direct_required_name_for_keyword(ReferenceKeyword::Tool)
    }

    #[must_use]
    pub fn import_name(&self, reference_keyword: ReferenceKeyword) -> Option<&str> {
        self.direct_required_name_for_keyword(reference_keyword)
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
            rendered_reference.push_str(reference_access.operator());
            rendered_reference.push_str(reference_access.field.as_str());
        }

        rendered_reference
    }

    pub fn collect_dynamic_dependency(&self, referenced_dynamic_fields: &mut std::collections::HashSet<String>) {
        if self.root_keyword() != Some(ReferenceKeyword::Dynamic) {
            return;
        }

        let Some(dynamic_field_name) = self.first_access_field() else {
            return;
        };

        referenced_dynamic_fields.insert(dynamic_field_name.to_string());
    }

    pub fn collect_runtime_dependency<HashBuilder: BuildHasher>(
        &self,
        referenced_runtime_roots: &mut HashSet<ReferenceKeyword, HashBuilder>,
    ) {
        match self.root_keyword() {
            Some(reference_keyword @ (ReferenceKeyword::Input | ReferenceKeyword::Secrets)) => {
                referenced_runtime_roots.insert(reference_keyword);
            }
            Some(
                ReferenceKeyword::Agent
                | ReferenceKeyword::Dynamic
                | ReferenceKeyword::Model
                | ReferenceKeyword::Tool
                | ReferenceKeyword::Resource
                | ReferenceKeyword::Prompt,
            )
            | None => {}
        }
    }

    pub fn collect_agent_dependency<HashBuilder: BuildHasher>(&self, referenced_agents: &mut HashSet<String, HashBuilder>) {
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReferenceAccess {
    pub field: String,
    pub kind: ReferenceAccessKind,
}

impl ReferenceAccess {
    #[must_use]
    pub fn required(field: impl Into<String>) -> Self {
        Self {
            field: field.into(),
            kind: ReferenceAccessKind::Required,
        }
    }

    #[must_use]
    pub fn optional(field: impl Into<String>) -> Self {
        Self {
            field: field.into(),
            kind: ReferenceAccessKind::Optional,
        }
    }

    #[must_use]
    pub fn array_pluck(field: impl Into<String>) -> Self {
        Self {
            field: field.into(),
            kind: ReferenceAccessKind::ArrayPluck,
        }
    }

    #[must_use]
    pub fn non_null_array_pluck(field: impl Into<String>) -> Self {
        Self {
            field: field.into(),
            kind: ReferenceAccessKind::NonNullArrayPluck,
        }
    }

    #[must_use]
    pub fn strict_array_pluck(field: impl Into<String>) -> Self {
        Self {
            field: field.into(),
            kind: ReferenceAccessKind::StrictArrayPluck,
        }
    }

    #[must_use]
    pub fn is_optional(&self) -> bool {
        self.kind == ReferenceAccessKind::Optional
    }

    #[must_use]
    pub fn is_array_pluck(&self) -> bool {
        self.kind.is_array_pluck()
    }

    #[must_use]
    pub fn filters_null_array_pluck_values(&self) -> bool {
        self.kind == ReferenceAccessKind::NonNullArrayPluck
    }

    #[must_use]
    pub fn requires_strict_array_pluck_values(&self) -> bool {
        self.kind == ReferenceAccessKind::StrictArrayPluck
    }

    #[must_use]
    pub fn operator(&self) -> &'static str {
        self.kind.operator()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReferenceAccessKind {
    Required,
    Optional,
    ArrayPluck,
    NonNullArrayPluck,
    StrictArrayPluck,
}

impl ReferenceAccessKind {
    #[must_use]
    pub fn from_operator(operator: &str) -> Option<Self> {
        match operator {
            "." => Some(Self::Required),
            "?." => Some(Self::Optional),
            ".*." => Some(Self::ArrayPluck),
            ".**." => Some(Self::NonNullArrayPluck),
            ".***." => Some(Self::StrictArrayPluck),
            _ => None,
        }
    }

    #[must_use]
    pub fn is_array_pluck(self) -> bool {
        matches!(self, Self::ArrayPluck | Self::NonNullArrayPluck | Self::StrictArrayPluck)
    }

    #[must_use]
    pub fn operator(self) -> &'static str {
        match self {
            Self::Required => ".",
            Self::Optional => "?.",
            Self::ArrayPluck => ".*.",
            Self::NonNullArrayPluck => ".**.",
            Self::StrictArrayPluck => ".***.",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Reference, ReferenceAccess, ReferenceKeyword, ReferenceRoot};
    use crate::ast::{SourcePosition, SourceSpan};

    #[test]
    fn reference_access_helpers_expose_owned_path_segments() {
        let reference = reference_with_accesses(ReferenceKeyword::Input, [("profile", false), ("address", true), ("city", false)]);

        assert_eq!(reference.first_access_field(), Some("profile"));
        assert_eq!(
            reference
                .first_projection_access()
                .map(|reference_access| reference_access.field.as_str()),
            Some("address")
        );
        assert_eq!(
            reference.last_access().map(|reference_access| reference_access.field.as_str()),
            Some("city")
        );

        let projection_fields = reference
            .projection_accesses()
            .iter()
            .map(|reference_access| reference_access.field.as_str())
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
                .map(|(field_name, optional)| {
                    if optional {
                        return ReferenceAccess::optional(field_name);
                    }

                    ReferenceAccess::required(field_name)
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
