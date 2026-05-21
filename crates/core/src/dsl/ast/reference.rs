use super::{DeclarationKeyword, ReferenceKeyword, SourceSpan, TypeExpression};
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

        if reference_access.optional {
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReferenceAccess {
    pub field: String,
    pub optional: bool,
}
