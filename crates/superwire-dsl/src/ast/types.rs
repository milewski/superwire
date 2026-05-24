use super::{DeclarationKeyword, Reference, ReferenceAccess, SourceSpan, Workflow};
use serde_json::{Map, Value};
use std::collections::{BTreeMap, HashMap};
use std::hash::BuildHasher;

#[derive(Debug, Default)]
pub struct TypeExpressionFieldCache {
    object_field_indices: HashMap<TypeExpressionFieldCollectionKey, HashMap<String, usize>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct TypeExpressionFieldCollectionKey {
    field_names: Vec<String>,
}

impl TypeExpressionFieldCollectionKey {
    fn from_typed_fields(typed_fields: &[TypedField]) -> Self {
        Self {
            field_names: typed_fields.iter().map(|typed_field| typed_field.name.clone()).collect(),
        }
    }
}

impl TypeExpressionFieldCache {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    fn typed_field<'fields>(&mut self, typed_fields: &'fields [TypedField], field_name: &str) -> Option<&'fields TypedField> {
        let field_index = self.typed_field_index(typed_fields, field_name)?;

        typed_fields.get(field_index)
    }

    fn typed_field_index(&mut self, typed_fields: &[TypedField], field_name: &str) -> Option<usize> {
        let field_collection_key = TypeExpressionFieldCollectionKey::from_typed_fields(typed_fields);
        let field_indices = self.object_field_indices.entry(field_collection_key).or_insert_with(|| {
            let mut field_indices = HashMap::new();

            for (field_index, typed_field) in typed_fields.iter().enumerate() {
                field_indices.entry(typed_field.name.clone()).or_insert(field_index);
            }

            field_indices
        });

        field_indices.get(field_name).copied()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypedField {
    pub name: String,
    pub field_type: TypeExpression,
    pub description: Option<String>,
    pub span: SourceSpan,
}

impl TypedField {
    #[must_use]
    pub fn from_type(name: impl Into<String>, field_type: TypeExpression, span: SourceSpan) -> Self {
        Self {
            name: name.into(),
            field_type,
            description: None,
            span,
        }
    }

    #[must_use]
    pub fn from_type_with_description(
        name: impl Into<String>,
        field_type: TypeExpression,
        description: Option<String>,
        span: SourceSpan,
    ) -> Self {
        Self {
            name: name.into(),
            field_type,
            description,
            span,
        }
    }

    #[must_use]
    pub fn summary_text(&self) -> String {
        format!("{}: {}", self.name, self.field_type.summary_text())
    }

    #[must_use]
    pub fn type_map(typed_fields: &[Self]) -> BTreeMap<String, TypeExpression> {
        typed_fields
            .iter()
            .map(|typed_field| (typed_field.name.clone(), typed_field.field_type.clone()))
            .collect()
    }

    #[must_use]
    pub fn hash_type_map(typed_fields: &[Self]) -> HashMap<String, TypeExpression> {
        typed_fields
            .iter()
            .map(|typed_field| (typed_field.name.clone(), typed_field.field_type.clone()))
            .collect()
    }

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
    pub fn object_from_type_map<'field, FieldTypes>(field_types: FieldTypes, span: SourceSpan) -> Self
    where
        FieldTypes: IntoIterator<Item = (&'field String, &'field TypeExpression)>,
    {
        let typed_fields = field_types
            .into_iter()
            .map(|(field_name, field_type)| TypedField::from_type(field_name.clone(), field_type.clone(), span))
            .collect();

        Self::Object(typed_fields)
    }

    #[must_use]
    pub fn summary_text(&self) -> String {
        match self {
            Self::String => "string".to_string(),
            Self::Number => "number".to_string(),
            Self::Float => "float".to_string(),
            Self::Boolean => "boolean".to_string(),
            Self::Null => "null".to_string(),
            Self::AnyObject => "object".to_string(),
            Self::SchemaReference(schema_name) => format!("{}.{}", DeclarationKeyword::Schema.as_str(), schema_name),
            Self::StringEnum(enum_value) => serde_json::to_string(enum_value).expect("string enum value should serialize"),
            Self::StringEnumReference(reference) => reference.render_path(),
            Self::Array { item_type, fixed_length } => {
                if let Some(fixed_length) = fixed_length {
                    return format!("[{}; {fixed_length}]", item_type.summary_text());
                }

                format!("[{}]", item_type.summary_text())
            }
            Self::Tuple(item_types) => {
                let item_summary = item_types.iter().map(Self::summary_text).collect::<Vec<_>>().join(", ");

                format!("({item_summary})")
            }
            Self::Object(typed_fields) => {
                let field_summary = typed_fields.iter().map(TypedField::summary_text).collect::<Vec<_>>().join(", ");

                format!("{{ {field_summary} }}")
            }
            Self::Variant { discriminator, cases } => {
                let case_summary = cases.iter().map(VariantCase::summary_text).collect::<Vec<_>>().join(" | ");

                format!("variant {discriminator} {{ {case_summary} }}")
            }
            Self::Union(type_expressions) => type_expressions.iter().map(Self::summary_text).collect::<Vec<_>>().join(" | "),
        }
    }

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

    #[must_use]
    pub fn field_type_at_path<'expression>(&'expression self, field_path: &[&str]) -> Option<&'expression TypeExpression> {
        let mut field_cache = TypeExpressionFieldCache::new();

        self.field_type_at_path_with_cache(field_path, &mut field_cache)
    }

    #[must_use]
    pub fn field_type_at_path_with_cache<'expression>(
        &'expression self,
        field_path: &[&str],
        field_cache: &mut TypeExpressionFieldCache,
    ) -> Option<&'expression TypeExpression> {
        let Some((field_name, remaining_field_path)) = field_path.split_first() else {
            return Some(self);
        };

        match self {
            Self::Object(typed_fields) => {
                let typed_field = field_cache.typed_field(typed_fields, field_name)?;

                typed_field
                    .field_type
                    .field_type_at_path_with_cache(remaining_field_path, field_cache)
            }
            Self::Union(type_expressions) => {
                for type_expression in type_expressions {
                    if let Some(field_type) = type_expression.field_type_at_path_with_cache(field_path, field_cache) {
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
    pub fn field_span_at_path<SchemaFieldSpanLookup>(
        &self,
        field_path: &[&str],
        schema_field_span_lookup: &mut SchemaFieldSpanLookup,
    ) -> Option<SourceSpan>
    where
        SchemaFieldSpanLookup: FnMut(&str, &[&str]) -> Option<SourceSpan>,
    {
        let mut field_cache = TypeExpressionFieldCache::new();

        self.field_span_at_path_with_cache(field_path, schema_field_span_lookup, &mut field_cache)
    }

    #[must_use]
    pub fn field_span_at_path_with_cache<SchemaFieldSpanLookup>(
        &self,
        field_path: &[&str],
        schema_field_span_lookup: &mut SchemaFieldSpanLookup,
        field_cache: &mut TypeExpressionFieldCache,
    ) -> Option<SourceSpan>
    where
        SchemaFieldSpanLookup: FnMut(&str, &[&str]) -> Option<SourceSpan>,
    {
        let (field_name, remaining_field_path) = field_path.split_first()?;

        match self {
            Self::Object(typed_fields) => {
                let typed_field = field_cache.typed_field(typed_fields, field_name)?;

                if remaining_field_path.is_empty() {
                    return Some(typed_field.span);
                }

                typed_field
                    .field_type
                    .field_span_at_path_with_cache(remaining_field_path, schema_field_span_lookup, field_cache)
            }
            Self::SchemaReference(schema_name) => schema_field_span_lookup(schema_name, field_path),
            Self::Variant { discriminator, cases } => {
                if remaining_field_path.is_empty() && *field_name == discriminator {
                    return cases.first().map(|variant_case| variant_case.span);
                }

                None
            }
            Self::Union(type_expressions) => {
                for type_expression in type_expressions {
                    if let Some(field_span) =
                        type_expression.field_span_at_path_with_cache(field_path, schema_field_span_lookup, field_cache)
                    {
                        return Some(field_span);
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
            | Self::Tuple(_) => None,
        }
    }

    #[must_use]
    pub fn resolved_field_type_at_path<HashBuilder: BuildHasher>(
        &self,
        field_path: &[&str],
        named_schemas: &HashMap<String, TypeExpression, HashBuilder>,
    ) -> Option<TypeExpression> {
        let mut field_cache = TypeExpressionFieldCache::new();

        self.resolved_field_type_at_path_with_cache(field_path, named_schemas, &mut field_cache)
    }

    #[must_use]
    pub fn resolved_field_type_at_path_with_cache<HashBuilder: BuildHasher>(
        &self,
        field_path: &[&str],
        named_schemas: &HashMap<String, TypeExpression, HashBuilder>,
        field_cache: &mut TypeExpressionFieldCache,
    ) -> Option<TypeExpression> {
        let Some((field_name, remaining_field_path)) = field_path.split_first() else {
            return Some(self.clone());
        };

        match self {
            Self::Object(typed_fields) => {
                let typed_field = field_cache.typed_field(typed_fields, field_name)?;

                typed_field
                    .field_type
                    .resolved_field_type_at_path_with_cache(remaining_field_path, named_schemas, field_cache)
            }
            Self::SchemaReference(schema_name) => {
                named_schemas
                    .get(schema_name)?
                    .resolved_field_type_at_path_with_cache(field_path, named_schemas, field_cache)
            }
            Self::Union(type_expressions) => {
                for type_expression in type_expressions {
                    if let Some(field_type) = type_expression.resolved_field_type_at_path_with_cache(field_path, named_schemas, field_cache)
                    {
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

    #[must_use]
    pub fn field_types_for_access<SchemaTypeLookup>(&self, field_name: &str, mut schema_type_lookup: SchemaTypeLookup) -> Vec<Self>
    where
        SchemaTypeLookup: FnMut(&str) -> Option<Self>,
    {
        let mut field_types = Vec::new();
        let mut field_cache = TypeExpressionFieldCache::new();

        self.collect_field_types_for_access_with_cache(field_name, &mut schema_type_lookup, &mut field_types, &mut field_cache);

        field_types
    }

    pub fn collect_field_types_for_access<SchemaTypeLookup>(
        &self,
        field_name: &str,
        schema_type_lookup: &mut SchemaTypeLookup,
        field_types: &mut Vec<Self>,
    ) where
        SchemaTypeLookup: FnMut(&str) -> Option<Self>,
    {
        let mut field_cache = TypeExpressionFieldCache::new();

        self.collect_field_types_for_access_with_cache(field_name, schema_type_lookup, field_types, &mut field_cache);
    }

    pub fn collect_field_types_for_access_with_cache<SchemaTypeLookup>(
        &self,
        field_name: &str,
        schema_type_lookup: &mut SchemaTypeLookup,
        field_types: &mut Vec<Self>,
        field_cache: &mut TypeExpressionFieldCache,
    ) where
        SchemaTypeLookup: FnMut(&str) -> Option<Self>,
    {
        match self {
            Self::Object(typed_fields) => {
                if let Some(typed_field) = field_cache.typed_field(typed_fields, field_name) {
                    field_types.push(typed_field.field_type.clone());
                }
            }
            Self::SchemaReference(schema_name) => {
                if let Some(schema_type) = schema_type_lookup(schema_name) {
                    schema_type.collect_field_types_for_access_with_cache(field_name, schema_type_lookup, field_types, field_cache);
                }
            }
            Self::Variant { discriminator, cases } => {
                if discriminator == field_name {
                    field_types.extend(cases.iter().map(|variant_case| Self::StringEnum(variant_case.name.clone())));
                }
            }
            Self::Union(type_expressions) => {
                for type_expression in type_expressions {
                    type_expression.collect_field_types_for_access_with_cache(field_name, schema_type_lookup, field_types, field_cache);
                }
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
            | Self::Tuple(_) => {}
        }
    }

    pub fn collect_field_types_for_reference_access_with_cache<SchemaTypeLookup>(
        &self,
        reference_access: &ReferenceAccess,
        schema_type_lookup: &mut SchemaTypeLookup,
        field_types: &mut Vec<Self>,
        field_cache: &mut TypeExpressionFieldCache,
    ) where
        SchemaTypeLookup: FnMut(&str) -> Option<Self>,
    {
        if !reference_access.is_array_pluck() {
            self.collect_field_types_for_access_with_cache(reference_access.field.as_str(), schema_type_lookup, field_types, field_cache);

            return;
        }

        match self {
            Self::Array {
                item_type,
                fixed_length: _,
            } => {
                let mut plucked_field_types = Vec::new();
                let mut includes_null = false;

                item_type.collect_array_pluck_field_types_with_cache(
                    reference_access.field.as_str(),
                    schema_type_lookup,
                    &mut plucked_field_types,
                    &mut includes_null,
                    field_cache,
                );

                let mut flattened_field_types = plucked_field_types
                    .into_iter()
                    .flat_map(Self::flatten_plucked_array_types)
                    .collect::<Vec<_>>();

                if reference_access.requires_strict_array_pluck_values() {
                    if includes_null || flattened_field_types.iter().any(Self::can_be_null) {
                        return;
                    }

                    if let Some(item_type) = Self::strict_array_pluck_item_type(flattened_field_types) {
                        field_types.push(Self::Array {
                            item_type: Box::new(item_type),
                            fixed_length: None,
                        });
                    }

                    return;
                }

                if reference_access.filters_null_array_pluck_values() {
                    flattened_field_types = flattened_field_types
                        .into_iter()
                        .map(Self::without_null)
                        .filter(|field_type| !matches!(field_type, Self::Null))
                        .collect();
                } else if includes_null {
                    flattened_field_types.push(Self::Null);
                }

                field_types.push(Self::Array {
                    item_type: Box::new(Self::array_pluck_item_type(flattened_field_types)),
                    fixed_length: None,
                });
            }
            Self::SchemaReference(schema_name) => {
                if let Some(schema_type) = schema_type_lookup(schema_name) {
                    schema_type.collect_field_types_for_reference_access_with_cache(
                        reference_access,
                        schema_type_lookup,
                        field_types,
                        field_cache,
                    );
                }
            }
            Self::Union(type_expressions) => {
                for type_expression in type_expressions {
                    type_expression.collect_field_types_for_reference_access_with_cache(
                        reference_access,
                        schema_type_lookup,
                        field_types,
                        field_cache,
                    );
                }
            }
            Self::String
            | Self::Number
            | Self::Float
            | Self::Boolean
            | Self::Null
            | Self::AnyObject
            | Self::StringEnum(_)
            | Self::StringEnumReference(_)
            | Self::Tuple(_)
            | Self::Object(_)
            | Self::Variant {
                discriminator: _,
                cases: _,
            } => {}
        }
    }

    fn collect_array_pluck_field_types_with_cache<SchemaTypeLookup>(
        &self,
        field_name: &str,
        schema_type_lookup: &mut SchemaTypeLookup,
        field_types: &mut Vec<Self>,
        includes_null: &mut bool,
        field_cache: &mut TypeExpressionFieldCache,
    ) where
        SchemaTypeLookup: FnMut(&str) -> Option<Self>,
    {
        match self {
            Self::Object(typed_fields) => {
                if let Some(typed_field) = field_cache.typed_field(typed_fields, field_name) {
                    field_types.push(typed_field.field_type.clone());
                } else {
                    *includes_null = true;
                }
            }
            Self::SchemaReference(schema_name) => {
                if let Some(schema_type) = schema_type_lookup(schema_name) {
                    schema_type.collect_array_pluck_field_types_with_cache(
                        field_name,
                        schema_type_lookup,
                        field_types,
                        includes_null,
                        field_cache,
                    );
                }
            }
            Self::Variant { discriminator, cases } => {
                if discriminator == field_name {
                    field_types.extend(cases.iter().map(|variant_case| Self::StringEnum(variant_case.name.clone())));
                } else {
                    *includes_null = true;
                }
            }
            Self::Union(type_expressions) => {
                for type_expression in type_expressions {
                    type_expression.collect_array_pluck_field_types_with_cache(
                        field_name,
                        schema_type_lookup,
                        field_types,
                        includes_null,
                        field_cache,
                    );
                }
            }
            Self::AnyObject => {
                field_types.push(Self::AnyObject);
            }
            Self::String
            | Self::Number
            | Self::Float
            | Self::Boolean
            | Self::Null
            | Self::StringEnum(_)
            | Self::StringEnumReference(_)
            | Self::Array {
                item_type: _,
                fixed_length: _,
            }
            | Self::Tuple(_) => {
                *includes_null = true;
            }
        }
    }

    fn flatten_plucked_array_types(field_type: Self) -> Vec<Self> {
        match field_type {
            Self::Array {
                item_type,
                fixed_length: _,
            } => vec![*item_type],
            Self::Union(type_expressions) => type_expressions.into_iter().flat_map(Self::flatten_plucked_array_types).collect(),
            _ => vec![field_type],
        }
    }

    #[must_use]
    fn array_pluck_item_type(field_types: Vec<Self>) -> Self {
        if field_types.is_empty() {
            return Self::Null;
        }

        Self::Union(field_types).normalize()
    }

    fn strict_array_pluck_item_type(field_types: Vec<Self>) -> Option<Self> {
        let item_type = Self::array_pluck_item_type(field_types);

        if matches!(item_type, Self::Union(_)) {
            return None;
        }

        Some(item_type)
    }

    #[must_use]
    fn without_null(self) -> Self {
        match self {
            Self::Union(type_expressions) => {
                let non_null_types = type_expressions
                    .into_iter()
                    .filter(|type_expression| !matches!(type_expression, Self::Null))
                    .collect::<Vec<_>>();

                Self::array_pluck_item_type(non_null_types)
            }
            Self::Null => Self::Null,
            _ => self,
        }
    }

    #[must_use]
    fn normalize(self) -> Self {
        match self {
            Self::Union(type_expressions) => {
                let mut normalized_types = Vec::new();

                for type_expression in type_expressions {
                    match type_expression.normalize() {
                        Self::Union(nested_type_expressions) => normalized_types.extend(nested_type_expressions),
                        normalized_type => normalized_types.push(normalized_type),
                    }
                }

                let mut deduplicated_types = Vec::new();

                for normalized_type in normalized_types {
                    if !deduplicated_types.contains(&normalized_type) {
                        deduplicated_types.push(normalized_type);
                    }
                }

                if deduplicated_types.len() == 1 {
                    return deduplicated_types.into_iter().next().expect("single type expression should exist");
                }

                Self::Union(deduplicated_types)
            }
            _ => self,
        }
    }

    #[must_use]
    pub fn available_field_types<SchemaTypeLookup>(&self, mut schema_type_lookup: SchemaTypeLookup) -> BTreeMap<String, Self>
    where
        SchemaTypeLookup: FnMut(&str) -> Option<Self>,
    {
        let mut available_field_types = BTreeMap::new();

        self.collect_available_field_types(&mut schema_type_lookup, &mut available_field_types);

        available_field_types
    }

    pub fn collect_available_field_types<SchemaTypeLookup>(
        &self,
        schema_type_lookup: &mut SchemaTypeLookup,
        available_field_types: &mut BTreeMap<String, Self>,
    ) where
        SchemaTypeLookup: FnMut(&str) -> Option<Self>,
    {
        match self {
            Self::Object(typed_fields) => {
                for typed_field in typed_fields {
                    available_field_types
                        .entry(typed_field.name.clone())
                        .or_insert_with(|| typed_field.field_type.clone());
                }
            }
            Self::SchemaReference(schema_name) => {
                if let Some(schema_type) = schema_type_lookup(schema_name) {
                    schema_type.collect_available_field_types(schema_type_lookup, available_field_types);
                }
            }
            Self::Variant { discriminator, cases } => {
                available_field_types.entry(discriminator.clone()).or_insert_with(|| {
                    Self::Union(
                        cases
                            .iter()
                            .map(|variant_case| Self::StringEnum(variant_case.name.clone()))
                            .collect(),
                    )
                });
            }
            Self::Union(type_expressions) => {
                for type_expression in type_expressions {
                    type_expression.collect_available_field_types(schema_type_lookup, available_field_types);
                }
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
            | Self::Tuple(_) => {}
        }
    }
}

impl VariantCase {
    #[must_use]
    pub fn summary_text(&self) -> String {
        let field_summary = self.fields.iter().map(TypedField::summary_text).collect::<Vec<_>>().join(", ");

        format!("{} {{ {field_summary} }}", self.name)
    }
}

#[cfg(test)]
mod tests {
    use super::{TypeExpression, TypedField, VariantCase};
    use crate::ast::{Declaration, SchemaDeclaration, SourcePosition, SourceSpan, Workflow};
    use serde_json::json;

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

    #[test]
    fn builds_type_maps_from_typed_fields() {
        let typed_fields = vec![
            typed_field("title", TypeExpression::String),
            typed_field("count", TypeExpression::Number),
        ];

        let type_map = TypedField::type_map(&typed_fields);

        assert_eq!(type_map.get("title"), Some(&TypeExpression::String));
        assert_eq!(type_map.get("count"), Some(&TypeExpression::Number));
    }

    #[test]
    fn collects_available_field_types_through_schema_references() {
        let schema_fields = vec![
            typed_field("status", TypeExpression::StringEnum("open".to_string())),
            typed_field("count", TypeExpression::Number),
        ];
        let schema_type_map = TypedField::type_map(&schema_fields);
        let type_expression = TypeExpression::Union(vec![
            TypeExpression::SchemaReference("Ticket".to_string()),
            TypeExpression::Variant {
                discriminator: "kind".to_string(),
                cases: vec![VariantCase {
                    name: "task".to_string(),
                    fields: Vec::new(),
                    span: test_source_span(),
                }],
            },
        ]);

        let available_field_types = type_expression.available_field_types(|schema_name| {
            if schema_name != "Ticket" {
                return None;
            }

            Some(TypeExpression::object_from_type_map(schema_type_map.iter(), test_source_span()))
        });

        assert_eq!(
            available_field_types.get("status"),
            Some(&TypeExpression::StringEnum("open".to_string()))
        );
        assert_eq!(
            available_field_types.get("kind"),
            Some(&TypeExpression::Union(vec![TypeExpression::StringEnum("task".to_string())]))
        );
    }

    #[test]
    fn collects_field_types_for_access_through_schema_references() {
        let schema_fields = vec![typed_field("status", TypeExpression::StringEnum("open".to_string()))];
        let schema_type_map = TypedField::type_map(&schema_fields);
        let type_expression = TypeExpression::SchemaReference("Ticket".to_string());

        let field_types = type_expression.field_types_for_access("status", |schema_name| {
            if schema_name != "Ticket" {
                return None;
            }

            Some(TypeExpression::object_from_type_map(schema_type_map.iter(), test_source_span()))
        });

        assert_eq!(field_types, vec![TypeExpression::StringEnum("open".to_string())]);
    }

    #[test]
    fn cached_object_field_lookup_preserves_first_duplicate_field() {
        let type_expression = TypeExpression::Object(vec![
            typed_field("status", TypeExpression::String),
            typed_field("status", TypeExpression::Number),
        ]);

        assert_eq!(type_expression.field_type_at_path(&["status"]), Some(&TypeExpression::String));
        assert_eq!(
            type_expression.field_types_for_access("status", |_schema_name| None),
            vec![TypeExpression::String]
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
