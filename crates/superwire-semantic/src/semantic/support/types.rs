use crate::semantic::WorkflowSemanticError;
use jsonschema::ValidationError;
use schemars::{JsonSchema, Schema};
use serde_json::{json, Map, Number, Value};
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::fmt::{Display, Formatter, Write};
use std::hash::BuildHasher;
use superwire_types::ast::{Reference, ReferenceAccess, ToolDeclaration, TypeExpression, TypedField};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkflowType {
    Any,
    String,
    Integer,
    Float,
    Boolean,
    Null,
    AnyObject,
    StringEnum(Vec<String>),
    Array {
        item_type: Box<WorkflowType>,
        fixed_length: Option<u64>,
    },
    Tuple(Vec<WorkflowType>),
    Object(BTreeMap<String, WorkflowType>),
    Variant {
        discriminator: String,
        cases: BTreeMap<String, BTreeMap<String, WorkflowType>>,
    },
    Union(Vec<WorkflowType>),
}

type ResolvedVariantCases = (String, BTreeMap<String, BTreeMap<String, WorkflowType>>);

#[derive(Debug, Clone)]
pub struct WorkflowSchemaCache {
    capacity: usize,
    schemas: HashMap<String, Value>,
    insertion_order: VecDeque<String>,
}

impl WorkflowType {
    const DEFAULT_SCHEMA_CACHE_CAPACITY: usize = 256;

    #[must_use]
    pub fn can_be_null(&self) -> bool {
        match self {
            Self::Null => true,
            Self::Union(members) => members.iter().any(Self::can_be_null),
            Self::Any
            | Self::String
            | Self::Integer
            | Self::Float
            | Self::Boolean
            | Self::AnyObject
            | Self::StringEnum(_)
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
    pub fn without_null(&self) -> Self {
        match self {
            Self::Union(members) => {
                let non_null_members = members
                    .iter()
                    .filter(|member| !matches!(member, Self::Null))
                    .cloned()
                    .collect::<Vec<_>>();

                normalize_union_members(non_null_members)
            }
            _ => self.clone(),
        }
    }

    #[must_use]
    pub fn nullable(inner_type: Self) -> Self {
        normalize_union_members(vec![inner_type, Self::Null])
    }

    #[must_use]
    pub fn field_type(&self, field_name: &str) -> Option<Self> {
        self.field_type_for_access(field_name, false)
    }

    #[must_use]
    pub fn field_type_for_reference_access(&self, reference_access: &ReferenceAccess) -> Option<Self> {
        if reference_access.is_array_pluck() {
            return self.array_pluck_field_type(reference_access);
        }

        self.field_type_for_access(reference_access.field.as_str(), reference_access.is_optional())
    }

    fn field_type_for_access(&self, field_name: &str, allows_missing_member: bool) -> Option<Self> {
        match self {
            Self::Any | Self::AnyObject => Some(Self::Any),
            Self::Object(fields) => fields.get(field_name).cloned(),
            Self::Variant { discriminator, cases } => {
                if discriminator == field_name {
                    return Some(Self::StringEnum(cases.keys().cloned().collect()));
                }

                None
            }
            Self::Union(members) => {
                let mut field_types = Vec::new();

                for member in members {
                    if matches!(member, Self::Null) {
                        if allows_missing_member {
                            continue;
                        }

                        return None;
                    }

                    let Some(field_type) = member.field_type_for_access(field_name, allows_missing_member) else {
                        if allows_missing_member {
                            continue;
                        }

                        return None;
                    };

                    field_types.push(field_type);
                }

                if field_types.is_empty() {
                    return None;
                }

                Some(normalize_union_members(field_types))
            }
            Self::String
            | Self::Integer
            | Self::Float
            | Self::Boolean
            | Self::Null
            | Self::StringEnum(_)
            | Self::Array {
                item_type: _,
                fixed_length: _,
            }
            | Self::Tuple(_) => None,
        }
    }

    #[must_use]
    pub fn array_pluck_field_type(&self, reference_access: &ReferenceAccess) -> Option<Self> {
        match self {
            Self::Array {
                item_type,
                fixed_length: _,
            } => {
                let mut plucked_field_types = Vec::new();
                let mut includes_null = false;

                item_type.collect_array_pluck_field_types(reference_access.field.as_str(), &mut plucked_field_types, &mut includes_null);

                let mut flattened_field_types = plucked_field_types
                    .into_iter()
                    .flat_map(Self::flatten_plucked_array_types)
                    .collect::<Vec<_>>();

                if reference_access.requires_strict_array_pluck_values() {
                    if includes_null || flattened_field_types.iter().any(WorkflowType::can_be_null) {
                        return None;
                    }

                    return Self::strict_array_pluck_item_type(flattened_field_types).map(|item_type| Self::Array {
                        item_type: Box::new(item_type),
                        fixed_length: None,
                    });
                }

                if reference_access.filters_null_array_pluck_values() {
                    flattened_field_types = flattened_field_types
                        .into_iter()
                        .map(|field_type| field_type.without_null())
                        .filter(|field_type| !matches!(field_type, Self::Null))
                        .collect();
                } else if includes_null {
                    flattened_field_types.push(Self::Null);
                }

                Some(Self::Array {
                    item_type: Box::new(Self::array_pluck_item_type(flattened_field_types)),
                    fixed_length: None,
                })
            }
            Self::Union(members) => {
                let mut field_types = Vec::new();

                for member in members {
                    let field_type = member.array_pluck_field_type(reference_access)?;
                    field_types.push(field_type);
                }

                if field_types.is_empty() {
                    return None;
                }

                Some(normalize_union_members(field_types))
            }
            Self::Any
            | Self::String
            | Self::Integer
            | Self::Float
            | Self::Boolean
            | Self::Null
            | Self::AnyObject
            | Self::StringEnum(_)
            | Self::Tuple(_)
            | Self::Object(_)
            | Self::Variant {
                discriminator: _,
                cases: _,
            } => None,
        }
    }

    fn collect_array_pluck_field_types(&self, field_name: &str, field_types: &mut Vec<Self>, includes_null: &mut bool) {
        match self {
            Self::Object(fields) => {
                if let Some(field_type) = fields.get(field_name) {
                    field_types.push(field_type.clone());
                } else {
                    *includes_null = true;
                }
            }
            Self::Variant { discriminator, cases } => {
                if discriminator == field_name {
                    field_types.push(Self::StringEnum(cases.keys().cloned().collect()));
                } else {
                    let initial_field_count = field_types.len();

                    for case_fields in cases.values() {
                        if let Some(field_type) = case_fields.get(field_name) {
                            field_types.push(field_type.clone());
                        } else {
                            *includes_null = true;
                        }
                    }

                    if field_types.len() == initial_field_count {
                        *includes_null = true;
                    }
                }
            }
            Self::Union(members) => {
                for member in members {
                    member.collect_array_pluck_field_types(field_name, field_types, includes_null);
                }
            }
            Self::Any | Self::AnyObject => {
                field_types.push(Self::Any);
            }
            Self::String
            | Self::Integer
            | Self::Float
            | Self::Boolean
            | Self::Null
            | Self::StringEnum(_)
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
            Self::Union(members) => members.into_iter().flat_map(Self::flatten_plucked_array_types).collect(),
            _ => vec![field_type],
        }
    }

    #[must_use]
    fn array_pluck_item_type(field_types: Vec<Self>) -> Self {
        if field_types.is_empty() {
            return Self::Null;
        }

        normalize_union_members(field_types)
    }

    fn strict_array_pluck_item_type(field_types: Vec<Self>) -> Option<Self> {
        let item_type = Self::array_pluck_item_type(field_types).normalize();

        if let Self::Union(_) = item_type {
            return None;
        }

        Some(item_type)
    }

    #[must_use]
    pub fn field_type_at_path(&self, field_path: &[&str]) -> Option<Self> {
        let mut current_type = self.clone();

        for field_name in field_path {
            current_type = current_type.field_type(field_name)?;
        }

        Some(current_type)
    }

    #[must_use]
    pub fn field_names(&self) -> Option<Vec<String>> {
        match self {
            Self::Object(fields) => Some(fields.keys().cloned().collect()),
            Self::Variant { discriminator, cases } => {
                let mut field_names = cases.values().flat_map(|fields| fields.keys().cloned()).collect::<Vec<_>>();
                field_names.push(discriminator.clone());
                field_names.sort();
                field_names.dedup();

                Some(field_names)
            }
            Self::Union(members) => {
                let mut field_names = members.iter().filter_map(Self::field_names).flatten().collect::<Vec<_>>();
                field_names.sort();
                field_names.dedup();

                (!field_names.is_empty()).then_some(field_names)
            }
            Self::Any
            | Self::String
            | Self::Integer
            | Self::Float
            | Self::Boolean
            | Self::Null
            | Self::AnyObject
            | Self::StringEnum(_)
            | Self::Array {
                item_type: _,
                fixed_length: _,
            }
            | Self::Tuple(_) => None,
        }
    }

    #[must_use]
    pub fn variant_case_field_type(&self, case_name: &str, field_path: &[String]) -> Option<Self> {
        let (_, cases) = self.resolved_variant_cases()?;
        let case_fields = cases.get(case_name)?;
        let Some((first_field_name, remaining_field_path)) = field_path.split_first() else {
            return Some(Self::Object(case_fields.clone()));
        };
        let mut current_type = case_fields.get(first_field_name)?.clone();

        for field_name in remaining_field_path {
            current_type = current_type.field_type(field_name)?;
        }

        Some(current_type)
    }

    #[must_use]
    pub fn variant_case_names(&self) -> Option<Vec<String>> {
        let (_, cases) = self.resolved_variant_cases()?;

        Some(cases.keys().cloned().collect())
    }

    #[must_use]
    pub fn variant_discriminator(&self) -> Option<String> {
        self.resolved_variant_cases().map(|(discriminator, _)| discriminator)
    }

    #[must_use]
    pub fn contains_variant_type(&self) -> bool {
        match self {
            Self::Variant {
                discriminator: _,
                cases: _,
            } => true,
            Self::Union(members) => members.iter().any(Self::contains_variant_type),
            Self::Any
            | Self::String
            | Self::Integer
            | Self::Float
            | Self::Boolean
            | Self::Null
            | Self::AnyObject
            | Self::StringEnum(_)
            | Self::Array {
                item_type: _,
                fixed_length: _,
            }
            | Self::Tuple(_)
            | Self::Object(_) => false,
        }
    }

    fn resolved_variant_cases(&self) -> Option<ResolvedVariantCases> {
        match self {
            Self::Variant { discriminator, cases } => Some((discriminator.clone(), cases.clone())),
            Self::Union(members) => {
                let mut resolved_discriminator = None::<String>;
                let mut resolved_cases = BTreeMap::new();
                let mut found_variant = false;

                for member in members {
                    if matches!(member, Self::Null) {
                        continue;
                    }

                    let (member_discriminator, member_cases) = member.resolved_variant_cases()?;
                    found_variant = true;

                    if let Some(discriminator) = &resolved_discriminator {
                        if discriminator != &member_discriminator {
                            return None;
                        }
                    } else {
                        resolved_discriminator = Some(member_discriminator);
                    }

                    for (case_name, case_fields) in member_cases {
                        if let Some(existing_fields) = resolved_cases.get(&case_name) {
                            if existing_fields != &case_fields {
                                return None;
                            }

                            continue;
                        }

                        resolved_cases.insert(case_name, case_fields);
                    }
                }

                if !found_variant {
                    return None;
                }

                Some((resolved_discriminator?, resolved_cases))
            }
            Self::Any
            | Self::String
            | Self::Integer
            | Self::Float
            | Self::Boolean
            | Self::Null
            | Self::AnyObject
            | Self::StringEnum(_)
            | Self::Array {
                item_type: _,
                fixed_length: _,
            }
            | Self::Tuple(_)
            | Self::Object(_) => None,
        }
    }

    #[must_use]
    pub fn array_item_type(&self) -> Option<Self> {
        match self {
            Self::Array {
                item_type,
                fixed_length: _,
            } => Some((**item_type).clone()),
            Self::Union(members) => {
                if members.is_empty() {
                    return None;
                }

                let mut item_types = Vec::with_capacity(members.len());

                for member in members {
                    item_types.push(member.array_item_type()?);
                }

                Some(normalize_union_members(item_types))
            }
            Self::Any
            | Self::String
            | Self::Integer
            | Self::Float
            | Self::Boolean
            | Self::Null
            | Self::AnyObject
            | Self::StringEnum(_)
            | Self::Tuple(_)
            | Self::Object(_)
            | Self::Variant {
                discriminator: _,
                cases: _,
            } => None,
        }
    }

    #[must_use]
    pub fn to_type_expression(&self) -> TypeExpression {
        match self {
            Self::Any | Self::AnyObject => TypeExpression::AnyObject,
            Self::String => TypeExpression::String,
            Self::Integer => TypeExpression::Number,
            Self::Float => TypeExpression::Float,
            Self::Boolean => TypeExpression::Boolean,
            Self::Null => TypeExpression::Null,
            Self::StringEnum(enum_values) => {
                let mut enum_type_expressions = enum_values.iter().cloned().map(TypeExpression::StringEnum).collect::<Vec<_>>();

                if enum_type_expressions.len() == 1 {
                    enum_type_expressions.pop().expect("single string enum type should exist")
                } else {
                    TypeExpression::Union(enum_type_expressions)
                }
            }
            Self::Array { item_type, fixed_length } => TypeExpression::Array {
                item_type: Box::new(item_type.to_type_expression()),
                fixed_length: *fixed_length,
            },
            Self::Tuple(item_types) => TypeExpression::Tuple(item_types.iter().map(Self::to_type_expression).collect()),
            Self::Object(fields) => TypeExpression::Object(
                fields
                    .iter()
                    .map(|(field_name, field_type)| {
                        TypedField::from_type(
                            field_name.clone(),
                            field_type.to_type_expression(),
                            superwire_types::ast::SourceSpan::generated(),
                        )
                    })
                    .collect(),
            ),
            Self::Variant { discriminator, cases } => TypeExpression::Variant {
                discriminator: discriminator.clone(),
                cases: cases
                    .iter()
                    .map(|(case_name, fields)| {
                        let case_fields = fields
                            .iter()
                            .filter(|(field_name, _)| *field_name != discriminator)
                            .map(|(field_name, field_type)| {
                                TypedField::from_type(
                                    field_name.clone(),
                                    field_type.to_type_expression(),
                                    superwire_types::ast::SourceSpan::generated(),
                                )
                            })
                            .collect();

                        superwire_types::ast::VariantCase {
                            name: case_name.clone(),
                            fields: case_fields,
                            span: superwire_types::ast::SourceSpan::generated(),
                        }
                    })
                    .collect(),
            },
            Self::Union(members) => {
                let mut type_expressions = Vec::new();

                for member in members.iter().filter(|member| !matches!(member, Self::Null)) {
                    match member.to_type_expression() {
                        TypeExpression::Union(nested_type_expressions) => type_expressions.extend(nested_type_expressions),
                        type_expression => type_expressions.push(type_expression),
                    }
                }

                if members.iter().any(|member| matches!(member, Self::Null)) {
                    type_expressions.push(TypeExpression::Null);
                }

                TypeExpression::Union(type_expressions)
            }
        }
    }

    #[must_use]
    pub fn normalize(self) -> Self {
        match self {
            Self::Array { item_type, fixed_length } => Self::Array {
                item_type: Box::new(item_type.normalize()),
                fixed_length,
            },
            Self::Tuple(item_types) => Self::Tuple(item_types.into_iter().map(Self::normalize).collect()),
            Self::Object(fields) => {
                let normalized_fields = fields
                    .into_iter()
                    .map(|(field_name, field_type)| (field_name, field_type.normalize()))
                    .collect();

                Self::Object(normalized_fields)
            }
            Self::Variant { discriminator, cases } => {
                let normalized_cases = cases
                    .into_iter()
                    .map(|(case_name, fields)| {
                        let normalized_fields = fields
                            .into_iter()
                            .map(|(field_name, field_type)| (field_name, field_type.normalize()))
                            .collect();

                        (case_name, normalized_fields)
                    })
                    .collect();

                Self::Variant {
                    discriminator,
                    cases: normalized_cases,
                }
            }
            Self::Union(members) => normalize_union_members(members.into_iter().map(Self::normalize).collect()),
            Self::Any | Self::String | Self::Integer | Self::Float | Self::Boolean | Self::Null | Self::AnyObject | Self::StringEnum(_) => {
                self
            }
        }
    }

    #[must_use]
    pub fn schema_cache_key(&self) -> String {
        let mut cache_key = String::new();
        self.write_schema_cache_key(&mut cache_key);

        cache_key
    }

    fn write_schema_cache_key(&self, cache_key: &mut String) {
        match self {
            Self::Any => cache_key.push('a'),
            Self::String => cache_key.push('s'),
            Self::Integer => cache_key.push('i'),
            Self::Float => cache_key.push('f'),
            Self::Boolean => cache_key.push('b'),
            Self::Null => cache_key.push('n'),
            Self::AnyObject => cache_key.push('o'),
            Self::StringEnum(enum_values) => {
                cache_key.push('e');
                Self::write_schema_cache_key_count(cache_key, enum_values.len());

                for enum_value in enum_values {
                    Self::write_schema_cache_key_text(cache_key, enum_value);
                }
            }
            Self::Array { item_type, fixed_length } => {
                cache_key.push('r');

                if let Some(fixed_length) = fixed_length {
                    cache_key.push('1');
                    write!(cache_key, "{fixed_length};").expect("writing to a String should not fail");
                } else {
                    cache_key.push('0');
                }

                item_type.write_schema_cache_key(cache_key);
            }
            Self::Tuple(item_types) => {
                cache_key.push('t');
                Self::write_schema_cache_key_count(cache_key, item_types.len());

                for item_type in item_types {
                    item_type.write_schema_cache_key(cache_key);
                }
            }
            Self::Object(fields) => {
                cache_key.push('j');
                Self::write_schema_cache_key_count(cache_key, fields.len());

                for (field_name, field_type) in fields {
                    Self::write_schema_cache_key_text(cache_key, field_name);
                    field_type.write_schema_cache_key(cache_key);
                }
            }
            Self::Variant { discriminator, cases } => {
                cache_key.push('v');
                Self::write_schema_cache_key_text(cache_key, discriminator);
                Self::write_schema_cache_key_count(cache_key, cases.len());

                for (case_name, fields) in cases {
                    Self::write_schema_cache_key_text(cache_key, case_name);
                    Self::write_schema_cache_key_count(cache_key, fields.len());

                    for (field_name, field_type) in fields {
                        Self::write_schema_cache_key_text(cache_key, field_name);
                        field_type.write_schema_cache_key(cache_key);
                    }
                }
            }
            Self::Union(members) => {
                cache_key.push('u');
                Self::write_schema_cache_key_count(cache_key, members.len());

                for member in members {
                    member.write_schema_cache_key(cache_key);
                }
            }
        }
    }

    fn write_schema_cache_key_count(cache_key: &mut String, count: usize) {
        write!(cache_key, "{count};").expect("writing to a String should not fail");
    }

    fn write_schema_cache_key_text(cache_key: &mut String, text: &str) {
        write!(cache_key, "{}:", text.len()).expect("writing to a String should not fail");
        cache_key.push_str(text);
    }

    #[must_use]
    pub fn is_guaranteed_array(&self) -> bool {
        match self {
            Self::Array {
                item_type: _,
                fixed_length: _,
            } => true,
            Self::Union(union_members) => union_members.iter().all(Self::is_guaranteed_array),
            Self::Any
            | Self::String
            | Self::Integer
            | Self::Float
            | Self::Boolean
            | Self::Null
            | Self::AnyObject
            | Self::StringEnum(_)
            | Self::Tuple(_)
            | Self::Object(_)
            | Self::Variant {
                discriminator: _,
                cases: _,
            } => false,
        }
    }

    #[must_use]
    pub fn project_json_value(&self, value: &Value) -> Value {
        match self {
            Self::Object(object_fields) => self.project_object_json_value(object_fields, value),
            Self::Array {
                item_type,
                fixed_length: _,
            } => {
                let Some(array_values) = value.as_array() else {
                    return value.clone();
                };

                Value::Array(
                    array_values
                        .iter()
                        .map(|array_value| item_type.project_json_value(array_value))
                        .collect(),
                )
            }
            Self::Tuple(item_types) => {
                let Some(array_values) = value.as_array() else {
                    return value.clone();
                };

                Value::Array(
                    array_values
                        .iter()
                        .zip(item_types)
                        .map(|(array_value, item_type)| item_type.project_json_value(array_value))
                        .collect(),
                )
            }
            Self::Variant { discriminator, cases } => {
                let Some(object_value) = value.as_object() else {
                    return value.clone();
                };
                let Some(case_name) = object_value.get(discriminator).and_then(Value::as_str) else {
                    return value.clone();
                };
                let Some(case_fields) = cases.get(case_name) else {
                    return value.clone();
                };

                let mut projected_value = Self::Object(case_fields.clone()).project_json_value(value);

                if let Value::Object(projected_object) = &mut projected_value {
                    if let Some(discriminator_value) = object_value.get(discriminator) {
                        projected_object.insert(discriminator.clone(), discriminator_value.clone());
                    }
                }

                projected_value
            }
            Self::Union(union_members) => {
                if value.is_null() && union_members.iter().any(|union_member| matches!(union_member, Self::Null)) {
                    return Value::Null;
                }

                union_members
                    .iter()
                    .find(|union_member| !matches!(union_member, Self::Null))
                    .map_or_else(|| value.clone(), |union_member| union_member.project_json_value(value))
            }
            Self::Any | Self::String | Self::Integer | Self::Float | Self::Boolean | Self::Null | Self::AnyObject | Self::StringEnum(_) => {
                value.clone()
            }
        }
    }

    fn project_object_json_value(&self, object_fields: &BTreeMap<String, Self>, value: &Value) -> Value {
        let Some(object_value) = value.as_object() else {
            return value.clone();
        };

        let mut projected_object = Map::new();

        for (field_name, field_type) in object_fields {
            let Some(field_value) = object_value.get(field_name) else {
                continue;
            };

            projected_object.insert(
                field_name.clone(),
                field_type.project_json_value_with_parent_discriminator(field_value, object_value),
            );
        }

        Value::Object(projected_object)
    }

    fn project_json_value_with_parent_discriminator(&self, value: &Value, parent_object: &Map<String, Value>) -> Value {
        let Self::Variant { discriminator, cases: _ } = self else {
            return self.project_json_value(value);
        };
        let Some(discriminator_value) = parent_object.get(discriminator) else {
            return self.project_json_value(value);
        };
        let Some(value_object) = value.as_object() else {
            return self.project_json_value(value);
        };

        let mut value_with_discriminator = value_object.clone();
        value_with_discriminator
            .entry(discriminator.clone())
            .or_insert_with(|| discriminator_value.clone());

        self.project_json_value(&Value::Object(value_with_discriminator))
    }

    #[must_use]
    pub fn json_schema_value(&self) -> Value {
        self.json_schema_value_uncached()
    }

    #[must_use]
    pub fn json_schema_value_with_cache(&self, schema_cache: &mut WorkflowSchemaCache) -> Value {
        let schema_cache_key = self.schema_cache_key();

        if let Some(schema) = schema_cache.schema(&schema_cache_key) {
            return schema.clone();
        }

        let schema = self.json_schema_value_with_recursive_cache(schema_cache);
        schema_cache.insert(schema_cache_key, schema.clone());

        schema
    }

    #[must_use]
    pub fn json_schema_value_with_nullable_fields_optional_with_cache(&self, schema_cache: &mut WorkflowSchemaCache) -> Value {
        let mut schema = self.json_schema_value_with_cache(schema_cache);

        self.make_nullable_fields_optional_in_schema(&mut schema);

        schema
    }

    pub fn validate_value(&self, value: &Value) -> Result<(), String> {
        self.validate_value_with_schema(value, self.json_schema_value())
    }

    pub fn validate_value_allowing_missing_nullable_fields(&self, value: &Value) -> Result<(), String> {
        self.validate_value_with_schema(value, self.json_schema_value_with_nullable_fields_optional())
    }

    fn json_schema_value_uncached(&self) -> Value {
        let mut schema_cache = WorkflowSchemaCache::disabled();

        self.json_schema_value_with_recursive_cache(&mut schema_cache)
    }

    fn json_schema_value_with_nullable_fields_optional(&self) -> Value {
        let mut schema_cache = WorkflowSchemaCache::disabled();

        self.json_schema_value_with_nullable_fields_optional_with_cache(&mut schema_cache)
    }

    fn validate_value_with_schema(&self, value: &Value, schema: Value) -> Result<(), String> {
        let validator = jsonschema::validator_for(&schema)
            .map_err(|compile_error| format!("failed to compile generated schema for `{self}`: {compile_error}"))?;

        let mut validation_issues = validator.iter_errors(value).map(format_validation_issue).collect::<Vec<_>>();

        if validation_issues.is_empty() {
            return Ok(());
        }

        validation_issues.sort();
        validation_issues.dedup();

        Err(validation_issues.join("; "))
    }

    fn make_nullable_fields_optional_in_schema(&self, schema: &mut Value) {
        match self {
            Self::Object(object_fields) => {
                self.make_object_nullable_fields_optional_in_schema(object_fields, schema);
            }
            Self::Array {
                item_type,
                fixed_length: _,
            } => {
                if let Some(item_schema) = schema.get_mut("items") {
                    item_type.make_nullable_fields_optional_in_schema(item_schema);
                }
            }
            Self::Tuple(item_types) => {
                let Some(prefix_items) = schema.get_mut("prefixItems").and_then(Value::as_array_mut) else {
                    return;
                };

                for (item_type, item_schema) in item_types.iter().zip(prefix_items) {
                    item_type.make_nullable_fields_optional_in_schema(item_schema);
                }
            }
            Self::Variant { discriminator: _, cases } => {
                let Some(case_schemas) = schema.get_mut("oneOf").and_then(Value::as_array_mut) else {
                    return;
                };

                for ((_, case_fields), case_schema) in cases.iter().zip(case_schemas) {
                    let case_object_type = Self::Object(case_fields.clone());

                    case_object_type.make_nullable_fields_optional_in_schema(case_schema);
                }
            }
            Self::Union(union_members) => {
                let Some(union_schemas) = schema.get_mut("anyOf").and_then(Value::as_array_mut) else {
                    return;
                };

                for (union_member, union_schema) in union_members.iter().zip(union_schemas) {
                    union_member.make_nullable_fields_optional_in_schema(union_schema);
                }
            }
            Self::Any | Self::String | Self::Integer | Self::Float | Self::Boolean | Self::Null | Self::AnyObject | Self::StringEnum(_) => {
            }
        }
    }

    fn make_object_nullable_fields_optional_in_schema(&self, object_fields: &BTreeMap<String, Self>, schema: &mut Value) {
        if let Some(properties) = schema.get_mut("properties").and_then(Value::as_object_mut) {
            for (field_name, field_type) in object_fields {
                if let Some(field_schema) = properties.get_mut(field_name) {
                    field_type.make_nullable_fields_optional_in_schema(field_schema);
                }
            }
        }

        let nullable_field_names = object_fields
            .iter()
            .filter_map(|(field_name, field_type)| field_type.can_be_null().then_some(field_name.as_str()))
            .collect::<HashSet<_>>();

        let mut remove_required = false;

        if let Some(required_fields) = schema.get_mut("required").and_then(Value::as_array_mut) {
            required_fields.retain(|required_field| {
                required_field
                    .as_str()
                    .is_none_or(|required_field_name| !nullable_field_names.contains(required_field_name))
            });
            remove_required = required_fields.is_empty();
        }

        if remove_required {
            if let Some(schema_object) = schema.as_object_mut() {
                schema_object.remove("required");
            }
        }
    }

    fn json_schema_value_with_recursive_cache(&self, schema_cache: &mut WorkflowSchemaCache) -> Value {
        match self {
            Self::Any => json!({}),
            Self::String => json!({ "type": "string" }),
            Self::Integer => json!({ "type": "integer" }),
            Self::Float => json!({ "type": "number" }),
            Self::Boolean => json!({ "type": "boolean" }),
            Self::Null => json!({ "type": "null" }),
            Self::AnyObject => json!({ "type": "object" }),
            Self::StringEnum(enum_values) => json!({
                "type": "string",
                "enum": enum_values,
            }),
            Self::Array { item_type, fixed_length } => {
                let mut array_schema = json!({
                    "type": "array",
                    "items": item_type.json_schema_value_with_cache(schema_cache),
                });

                if let Some(fixed_length) = fixed_length {
                    array_schema["minItems"] = json!(fixed_length);
                    array_schema["maxItems"] = json!(fixed_length);
                }

                array_schema
            }
            Self::Tuple(tuple_items) => json!({
                "type": "array",
                "prefixItems": tuple_items
                    .iter()
                    .map(|tuple_item| tuple_item.json_schema_value_with_cache(schema_cache))
                    .collect::<Vec<_>>(),
                "minItems": tuple_items.len(),
                "maxItems": tuple_items.len(),
            }),
            Self::Object(object_fields) => {
                let properties = object_fields
                    .iter()
                    .map(|(field_name, field_type)| (field_name.clone(), field_type.json_schema_value_with_cache(schema_cache)))
                    .collect::<serde_json::Map<_, _>>();

                let required = object_fields.keys().cloned().collect::<Vec<_>>();

                json!({
                    "type": "object",
                    "properties": properties,
                    "required": required,
                    "additionalProperties": false,
                })
            }
            Self::Variant { discriminator, cases } => json!({
                "oneOf": cases
                    .iter()
                    .map(|(case_name, fields)| {
                        let mut properties = fields
                            .iter()
                            .map(|(field_name, field_type)| {
                                (field_name.clone(), field_type.json_schema_value_with_cache(schema_cache))
                            })
                            .collect::<serde_json::Map<_, _>>();
                        properties.insert(discriminator.clone(), json!({ "const": case_name }));

                        let required = properties.keys().cloned().collect::<Vec<_>>();

                        json!({
                            "type": "object",
                            "properties": properties,
                            "required": required,
                            "additionalProperties": false,
                        })
                    })
                    .collect::<Vec<_>>(),
                "discriminator": {
                    "propertyName": discriminator,
                },
            }),
            Self::Union(union_members) => json!({
                "anyOf": union_members
                    .iter()
                    .map(|union_member| union_member.json_schema_value_with_cache(schema_cache))
                    .collect::<Vec<_>>(),
            }),
        }
    }
}

impl Default for WorkflowSchemaCache {
    fn default() -> Self {
        Self::new()
    }
}

impl WorkflowSchemaCache {
    #[must_use]
    pub fn new() -> Self {
        Self::with_capacity(WorkflowType::DEFAULT_SCHEMA_CACHE_CAPACITY)
    }

    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            capacity,
            schemas: HashMap::new(),
            insertion_order: VecDeque::new(),
        }
    }

    #[must_use]
    pub fn disabled() -> Self {
        Self::with_capacity(0)
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.schemas.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.schemas.is_empty()
    }

    fn schema(&self, schema_cache_key: &str) -> Option<&Value> {
        self.schemas.get(schema_cache_key)
    }

    fn insert(&mut self, schema_cache_key: String, schema: Value) {
        if self.capacity == 0 {
            return;
        }

        if self.schemas.insert(schema_cache_key.clone(), schema).is_none() {
            self.insertion_order.push_back(schema_cache_key);
        }

        while self.schemas.len() > self.capacity {
            let Some(expired_schema_cache_key) = self.insertion_order.pop_front() else {
                break;
            };

            self.schemas.remove(&expired_schema_cache_key);
        }
    }
}

impl Display for WorkflowType {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Any => write!(formatter, "any"),
            Self::String => write!(formatter, "string"),
            Self::Integer => write!(formatter, "number"),
            Self::Float => write!(formatter, "float"),
            Self::Boolean => write!(formatter, "boolean"),
            Self::Null => write!(formatter, "null"),
            Self::AnyObject => write!(formatter, "object"),
            Self::StringEnum(enum_values) => {
                let formatted_values = enum_values
                    .iter()
                    .map(|enum_value| format!("\"{enum_value}\""))
                    .collect::<Vec<_>>()
                    .join(" | ");

                write!(formatter, "{formatted_values}")
            }
            Self::Array { item_type, fixed_length } => {
                if let Some(fixed_length) = fixed_length {
                    return write!(formatter, "[{item_type}; {fixed_length}]");
                }

                write!(formatter, "[{item_type}]")
            }
            Self::Tuple(item_types) => {
                let formatted_items = item_types.iter().map(ToString::to_string).collect::<Vec<_>>().join(", ");

                write!(formatter, "({formatted_items})")
            }
            Self::Object(fields) => {
                let formatted_fields = fields
                    .iter()
                    .map(|(field_name, field_type)| format!("{field_name}: {field_type}"))
                    .collect::<Vec<_>>()
                    .join(", ");

                write!(formatter, "{{ {formatted_fields} }}")
            }
            Self::Variant { discriminator, cases } => {
                let formatted_cases = cases.keys().cloned().collect::<Vec<_>>().join(" | ");

                write!(formatter, "variant {discriminator} {{ {formatted_cases} }}")
            }
            Self::Union(members) => {
                let formatted_members = members.iter().map(ToString::to_string).collect::<Vec<_>>().join(" | ");

                write!(formatter, "{formatted_members}")
            }
        }
    }
}

pub trait TypeExpressionWorkflowTypeExt {
    fn to_workflow_type<HashBuilder: BuildHasher>(
        &self,
        named_schemas: &HashMap<String, TypeExpression, HashBuilder>,
    ) -> Result<WorkflowType, WorkflowSemanticError>;
}

impl TypeExpressionWorkflowTypeExt for TypeExpression {
    fn to_workflow_type<HashBuilder: BuildHasher>(
        &self,
        named_schemas: &HashMap<String, TypeExpression, HashBuilder>,
    ) -> Result<WorkflowType, WorkflowSemanticError> {
        workflow_type_from_dsl_with_stack(self, named_schemas, &mut Vec::new()).map(WorkflowType::normalize)
    }
}

pub trait ToolDeclarationWorkflowTypeExt {
    fn resolved_input_type<HashBuilder: BuildHasher>(
        &self,
        named_schema_types: &HashMap<String, TypeExpression, HashBuilder>,
    ) -> Result<WorkflowType, WorkflowSemanticError>;

    fn resolved_full_input_type<HashBuilder: BuildHasher>(
        &self,
        named_schema_types: &HashMap<String, TypeExpression, HashBuilder>,
    ) -> Result<WorkflowType, WorkflowSemanticError>;

    fn resolved_output_type<HashBuilder: BuildHasher>(
        &self,
        named_schema_types: &HashMap<String, TypeExpression, HashBuilder>,
    ) -> Result<WorkflowType, WorkflowSemanticError>;
}

impl ToolDeclarationWorkflowTypeExt for ToolDeclaration {
    fn resolved_full_input_type<HashBuilder: BuildHasher>(
        &self,
        named_schema_types: &HashMap<String, TypeExpression, HashBuilder>,
    ) -> Result<WorkflowType, WorkflowSemanticError> {
        let Some(mcp_schema) = self.mcp_schema.as_ref().filter(|mcp_schema| mcp_schema.uses_discovered_input) else {
            return TypeExpression::Object(self.input_fields.clone()).to_workflow_type(named_schema_types);
        };

        workflow_type_from_json_schema(&mcp_schema.input)
    }

    fn resolved_input_type<HashBuilder: BuildHasher>(
        &self,
        named_schema_types: &HashMap<String, TypeExpression, HashBuilder>,
    ) -> Result<WorkflowType, WorkflowSemanticError> {
        let mut input_type = self.resolved_full_input_type(named_schema_types)?;

        if let WorkflowType::Object(input_fields) = &mut input_type {
            for fixed_binding_field in &self.fixed_binding_fields {
                input_fields.remove(&fixed_binding_field.name);
            }
        }

        Ok(input_type)
    }

    fn resolved_output_type<HashBuilder: BuildHasher>(
        &self,
        named_schema_types: &HashMap<String, TypeExpression, HashBuilder>,
    ) -> Result<WorkflowType, WorkflowSemanticError> {
        if self.has_untyped_mcp_output() {
            return Ok(WorkflowType::Any);
        }

        let Some(mcp_schema) = self.mcp_schema.as_ref().filter(|mcp_schema| mcp_schema.uses_discovered_output) else {
            return TypeExpression::Object(self.output_fields.clone()).to_workflow_type(named_schema_types);
        };

        mcp_schema
            .output
            .as_ref()
            .map_or(Ok(WorkflowType::Any), workflow_type_from_json_schema)
    }
}

pub fn workflow_type_from_dsl<HashBuilder: BuildHasher>(
    type_expression: &TypeExpression,
    named_schemas: &HashMap<String, TypeExpression, HashBuilder>,
) -> Result<WorkflowType, WorkflowSemanticError> {
    type_expression.to_workflow_type(named_schemas)
}

fn workflow_type_from_dsl_with_stack<HashBuilder: BuildHasher>(
    type_expression: &TypeExpression,
    named_schemas: &HashMap<String, TypeExpression, HashBuilder>,
    resolution_stack: &mut Vec<String>,
) -> Result<WorkflowType, WorkflowSemanticError> {
    match type_expression {
        TypeExpression::String => Ok(WorkflowType::String),
        TypeExpression::Number => Ok(WorkflowType::Integer),
        TypeExpression::Float => Ok(WorkflowType::Float),
        TypeExpression::Boolean => Ok(WorkflowType::Boolean),
        TypeExpression::Null => Ok(WorkflowType::Null),
        TypeExpression::AnyObject => Ok(WorkflowType::AnyObject),
        TypeExpression::StringEnum(enum_value) => Ok(WorkflowType::StringEnum(vec![enum_value.clone()])),
        TypeExpression::StringEnumReference(reference) => {
            reference.workflow_type_for_string_enum_reference(named_schemas, resolution_stack)
        }
        TypeExpression::Array { item_type, fixed_length } => Ok(WorkflowType::Array {
            item_type: Box::new(workflow_type_from_dsl_with_stack(item_type, named_schemas, resolution_stack)?),
            fixed_length: *fixed_length,
        }),
        TypeExpression::Tuple(item_types) => {
            let mut resolved_item_types = Vec::with_capacity(item_types.len());

            for item_type in item_types {
                resolved_item_types.push(workflow_type_from_dsl_with_stack(item_type, named_schemas, resolution_stack)?);
            }

            Ok(WorkflowType::Tuple(resolved_item_types))
        }
        TypeExpression::Object(fields) => Ok(WorkflowType::Object(resolve_object_fields(
            fields,
            named_schemas,
            resolution_stack,
        )?)),
        TypeExpression::Variant { discriminator, cases } => {
            if cases.is_empty() {
                return Err(WorkflowSemanticError::Other {
                    message: "variant type requires at least one case".to_string(),
                });
            }

            let mut resolved_cases = BTreeMap::new();

            for case in cases {
                let mut fields = resolve_object_fields(&case.fields, named_schemas, resolution_stack)?;
                fields.insert(discriminator.clone(), WorkflowType::StringEnum(vec![case.name.clone()]));
                resolved_cases.insert(case.name.clone(), fields);
            }

            Ok(WorkflowType::Variant {
                discriminator: discriminator.clone(),
                cases: resolved_cases,
            })
        }
        TypeExpression::Union(members) => {
            if members.is_empty() {
                return Err(WorkflowSemanticError::Other {
                    message: "union type requires at least one member".to_string(),
                });
            }

            let mut resolved_members = Vec::with_capacity(members.len());

            for union_member in members {
                resolved_members.push(workflow_type_from_dsl_with_stack(union_member, named_schemas, resolution_stack)?);
            }

            normalize_union_members_checked(resolved_members)
        }
        TypeExpression::SchemaReference(schema_name) => {
            if resolution_stack.contains(schema_name) {
                return Err(WorkflowSemanticError::Other {
                    message: format!("recursive schema reference is not supported: {}", resolution_stack.join(" -> ")),
                });
            }

            let Some(schema_type_expression) = named_schemas.get(schema_name) else {
                return Err(WorkflowSemanticError::Other {
                    message: format!("unknown schema reference `{schema_name}`"),
                });
            };

            resolution_stack.push(schema_name.clone());

            let resolved_schema_type = workflow_type_from_dsl_with_stack(schema_type_expression, named_schemas, resolution_stack);

            resolution_stack.pop();

            resolved_schema_type
        }
    }
}

fn resolve_object_fields<HashBuilder: BuildHasher>(
    fields: &[TypedField],
    named_schemas: &HashMap<String, TypeExpression, HashBuilder>,
    resolution_stack: &mut Vec<String>,
) -> Result<BTreeMap<String, WorkflowType>, WorkflowSemanticError> {
    let mut resolved_fields = BTreeMap::new();

    for field in fields {
        let field_type = workflow_type_from_dsl_with_stack(&field.field_type, named_schemas, resolution_stack)?;
        resolved_fields.insert(field.name.clone(), field_type);
    }

    Ok(resolved_fields)
}

trait ReferenceStringEnumExt {
    fn workflow_type_for_string_enum_reference<HashBuilder: BuildHasher>(
        &self,
        named_schemas: &HashMap<String, TypeExpression, HashBuilder>,
        resolution_stack: &mut Vec<String>,
    ) -> Result<WorkflowType, WorkflowSemanticError>;
}

impl ReferenceStringEnumExt for Reference {
    fn workflow_type_for_string_enum_reference<HashBuilder: BuildHasher>(
        &self,
        named_schemas: &HashMap<String, TypeExpression, HashBuilder>,
        resolution_stack: &mut Vec<String>,
    ) -> Result<WorkflowType, WorkflowSemanticError> {
        let Some((schema_name, field_path)) = self.schema_name_and_field_path() else {
            return Ok(WorkflowType::String);
        };

        if field_path.is_empty() {
            return Ok(WorkflowType::String);
        }

        if resolution_stack.contains(&schema_name.to_string()) {
            return Err(WorkflowSemanticError::Other {
                message: format!("recursive schema reference is not supported: {}", resolution_stack.join(" -> ")),
            });
        }

        let Some(schema_type_expression) = named_schemas.get(schema_name) else {
            return Err(WorkflowSemanticError::Other {
                message: format!("unknown schema reference `{schema_name}`"),
            });
        };

        let Some(field_type_expression) = schema_type_expression.resolved_field_type_at_path(&field_path, named_schemas) else {
            return Err(WorkflowSemanticError::Other {
                message: format!("unknown schema enum reference `{}`", self.render_path()),
            });
        };

        resolution_stack.push(schema_name.to_string());

        let resolved_field_type = workflow_type_from_dsl_with_stack(&field_type_expression, named_schemas, resolution_stack);

        resolution_stack.pop();

        resolved_field_type
    }
}

pub fn workflow_type_from_rust_schema<TypeMarker>() -> Result<WorkflowType, WorkflowSemanticError>
where
    TypeMarker: JsonSchema,
{
    let schema = schemars::schema_for!(TypeMarker);
    let schema_value = serde_json::to_value(schema).map_err(|source| WorkflowSemanticError::SerializationFailed {
        context: "Rust JsonSchema".to_string(),
        source,
    })?;

    workflow_type_from_json_schema(&schema_value)
}

pub fn workflow_type_from_json_schema(schema_value: &Value) -> Result<WorkflowType, WorkflowSemanticError> {
    WorkflowJsonSchemaParser::new(schema_value)
        .parse(schema_value)
        .map(WorkflowType::normalize)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum JsonSchemaTypeName {
    String,
    Integer,
    Number,
    Boolean,
    Null,
    Array,
    Object,
}

impl JsonSchemaTypeName {
    fn from_identifier(identifier: &str) -> Option<Self> {
        match identifier {
            "string" => Some(Self::String),
            "integer" => Some(Self::Integer),
            "number" => Some(Self::Number),
            "boolean" => Some(Self::Boolean),
            "null" => Some(Self::Null),
            "array" => Some(Self::Array),
            "object" => Some(Self::Object),
            _ => None,
        }
    }
}

struct WorkflowJsonSchemaParser {
    definitions: HashMap<String, Value>,
    resolution_stack: Vec<String>,
}

impl WorkflowJsonSchemaParser {
    fn new(root_schema: &Value) -> Self {
        let mut definitions = HashMap::new();

        for definitions_keyword in ["$defs", "definitions"] {
            if let Some(definition_entries) = root_schema.get(definitions_keyword).and_then(Value::as_object) {
                definitions.extend(
                    definition_entries
                        .iter()
                        .map(|(definition_name, definition_schema)| (definition_name.clone(), definition_schema.clone())),
                );
            }
        }

        Self {
            definitions,
            resolution_stack: Vec::new(),
        }
    }

    fn parse(&mut self, schema_value: &Value) -> Result<WorkflowType, WorkflowSemanticError> {
        if let Some(boolean_schema) = schema_value.as_bool() {
            return if boolean_schema {
                Ok(WorkflowType::Any)
            } else {
                Err(Self::unsupported("boolean `false` schema has no valid values"))
            };
        }

        if let Some(reference) = schema_value.get("$ref").and_then(Value::as_str) {
            return self.parse_reference(reference);
        }

        if schema_value.get("allOf").is_some() {
            return Err(Self::unsupported("`allOf` schemas are not supported"));
        }

        if let Some(enum_values) = schema_value.get("enum").and_then(Value::as_array) {
            return self.parse_enum(enum_values);
        }

        if let Some(any_of_schemas) = schema_value.get("anyOf").and_then(Value::as_array) {
            return self.parse_union(any_of_schemas, Self::discriminator_name(schema_value));
        }

        if let Some(one_of_schemas) = schema_value.get("oneOf").and_then(Value::as_array) {
            return self.parse_union(one_of_schemas, Self::discriminator_name(schema_value));
        }

        if let Some(constant_value) = schema_value.get("const") {
            return match constant_value {
                Value::Null => Ok(WorkflowType::Null),
                Value::String(string_constant) => Ok(WorkflowType::StringEnum(vec![string_constant.clone()])),
                _ => Err(Self::unsupported(format!(
                    "non-string `const` value is not supported: {constant_value}"
                ))),
            };
        }

        if schema_value.get("properties").is_some() {
            return self.parse_object(schema_value);
        }

        if let Some(type_entry) = schema_value.get("type") {
            if let Some(type_name) = type_entry.as_str() {
                return self.parse_type(type_name, schema_value);
            }

            if let Some(type_names) = type_entry.as_array() {
                let mut union_members = Vec::with_capacity(type_names.len());

                for type_name in type_names {
                    let Some(type_name) = type_name.as_str() else {
                        return Err(Self::unsupported(format!("invalid schema `type` entry: {type_name}")));
                    };

                    union_members.push(self.parse_type(type_name, schema_value)?);
                }

                return normalize_union_members_checked(union_members);
            }

            return Err(Self::unsupported(format!("invalid schema `type` entry: {type_entry}")));
        }

        if schema_value.as_object().is_some_and(Map::is_empty) {
            return Ok(WorkflowType::Any);
        }

        Err(Self::unsupported(format!("unsupported schema shape: {schema_value}")))
    }

    fn parse_reference(&mut self, reference: &str) -> Result<WorkflowType, WorkflowSemanticError> {
        let Some(encoded_reference_name) = reference
            .strip_prefix("#/$defs/")
            .or_else(|| reference.strip_prefix("#/definitions/"))
        else {
            return Err(Self::unsupported(format!("unsupported schema reference path `{reference}`")));
        };
        let reference_name = encoded_reference_name.replace("~1", "/").replace("~0", "~");

        if self.resolution_stack.contains(&reference_name) {
            let mut cycle = self.resolution_stack.clone();
            cycle.push(reference_name);

            return Err(Self::unsupported(format!(
                "recursive schema reference is not supported: {}",
                cycle.join(" -> ")
            )));
        }

        let referenced_schema = self
            .definitions
            .get(&reference_name)
            .cloned()
            .ok_or_else(|| Self::unsupported(format!("missing schema definition for reference `{reference}`")))?;
        self.resolution_stack.push(reference_name);
        let parsed_reference = self.parse(&referenced_schema);
        self.resolution_stack.pop();

        parsed_reference
    }

    fn parse_enum(&mut self, enum_values: &[Value]) -> Result<WorkflowType, WorkflowSemanticError> {
        if enum_values.is_empty() {
            return Err(Self::unsupported("empty enum schema has no valid values"));
        }

        let mut string_values = Vec::new();
        let mut includes_null = false;

        for enum_value in enum_values {
            match enum_value {
                Value::String(string_value) => string_values.push(string_value.clone()),
                Value::Null => includes_null = true,
                _ => {
                    return Err(Self::unsupported(format!(
                        "unsupported non-string enum variant in schema: {enum_value}"
                    )));
                }
            }
        }

        string_values.sort();
        string_values.dedup();
        let mut enum_type = if string_values.is_empty() {
            WorkflowType::Null
        } else {
            WorkflowType::StringEnum(string_values)
        };

        if includes_null && !matches!(enum_type, WorkflowType::Null) {
            enum_type = WorkflowType::nullable(enum_type);
        }

        Ok(enum_type)
    }

    fn parse_union(
        &mut self,
        union_members: &[Value],
        declared_discriminator: Option<&str>,
    ) -> Result<WorkflowType, WorkflowSemanticError> {
        if union_members.is_empty() {
            return Err(Self::unsupported("empty union schema has no valid values"));
        }

        let mut parsed_members = Vec::with_capacity(union_members.len());

        for union_member in union_members {
            parsed_members.push(self.parse(union_member)?);
        }

        if let Some(variant_type) = Self::infer_variant_union(&parsed_members, declared_discriminator)? {
            return Ok(variant_type);
        }

        normalize_union_members_checked(parsed_members)
    }

    fn discriminator_name(schema_value: &Value) -> Option<&str> {
        schema_value
            .get("discriminator")
            .and_then(|discriminator| discriminator.get("propertyName"))
            .and_then(Value::as_str)
    }

    fn infer_variant_union(
        union_members: &[WorkflowType],
        declared_discriminator: Option<&str>,
    ) -> Result<Option<WorkflowType>, WorkflowSemanticError> {
        let Some(WorkflowType::Object(first_fields)) = union_members.first() else {
            return Ok(None);
        };
        let inferred_discriminators = first_fields
            .iter()
            .filter_map(|(field_name, field_type)| {
                let WorkflowType::StringEnum(enum_values) = field_type else {
                    return None;
                };

                (enum_values.len() == 1).then_some(field_name.as_str())
            })
            .filter(|field_name| {
                union_members.iter().all(|union_member| {
                    let WorkflowType::Object(fields) = union_member else {
                        return false;
                    };

                    matches!(fields.get(*field_name), Some(WorkflowType::StringEnum(enum_values)) if enum_values.len() == 1)
                })
            })
            .collect::<Vec<_>>();
        let discriminator = match declared_discriminator {
            Some(discriminator) => discriminator,
            None if inferred_discriminators.len() == 1 => inferred_discriminators[0],
            None => return Ok(None),
        };
        let mut cases = BTreeMap::new();

        for union_member in union_members {
            let WorkflowType::Object(fields) = union_member else {
                return Ok(None);
            };
            let Some(WorkflowType::StringEnum(case_names)) = fields.get(discriminator) else {
                return Err(Self::unsupported(format!(
                    "discriminated union member is missing string discriminator `{discriminator}`"
                )));
            };

            if case_names.len() != 1 {
                return Err(Self::unsupported(format!(
                    "discriminated union member requires one `{discriminator}` value"
                )));
            }

            let case_name = case_names[0].clone();

            if cases.insert(case_name.clone(), fields.clone()).is_some() {
                return Err(Self::unsupported(format!(
                    "discriminated union contains duplicate case `{case_name}`"
                )));
            }
        }

        Ok(Some(WorkflowType::Variant {
            discriminator: discriminator.to_string(),
            cases,
        }))
    }

    fn parse_type(&mut self, type_name: &str, schema_value: &Value) -> Result<WorkflowType, WorkflowSemanticError> {
        let schema_type = JsonSchemaTypeName::from_identifier(type_name)
            .ok_or_else(|| Self::unsupported(format!("unsupported schema type `{type_name}`")))?;

        match schema_type {
            JsonSchemaTypeName::String => Ok(WorkflowType::String),
            JsonSchemaTypeName::Integer => Ok(WorkflowType::Integer),
            JsonSchemaTypeName::Number => Ok(WorkflowType::Float),
            JsonSchemaTypeName::Boolean => Ok(WorkflowType::Boolean),
            JsonSchemaTypeName::Null => Ok(WorkflowType::Null),
            JsonSchemaTypeName::Array => self.parse_array(schema_value),
            JsonSchemaTypeName::Object => self.parse_object(schema_value),
        }
    }

    fn parse_array(&mut self, schema_value: &Value) -> Result<WorkflowType, WorkflowSemanticError> {
        if let Some(prefix_items) = schema_value.get("prefixItems").and_then(Value::as_array) {
            let mut tuple_items = Vec::with_capacity(prefix_items.len());

            for prefix_item in prefix_items {
                tuple_items.push(self.parse(prefix_item)?);
            }

            return Ok(WorkflowType::Tuple(tuple_items));
        }

        let item_type = match schema_value.get("items") {
            Some(item_schema) => self.parse(item_schema)?,
            None => WorkflowType::Any,
        };
        let minimum_items = schema_value.get("minItems").and_then(Value::as_u64);
        let maximum_items = schema_value.get("maxItems").and_then(Value::as_u64);
        let fixed_length = if minimum_items.is_some() && minimum_items == maximum_items {
            minimum_items
        } else {
            None
        };

        Ok(WorkflowType::Array {
            item_type: Box::new(item_type),
            fixed_length,
        })
    }

    fn parse_object(&mut self, schema_value: &Value) -> Result<WorkflowType, WorkflowSemanticError> {
        let mut fields = BTreeMap::new();
        let required_fields = schema_value
            .get("required")
            .and_then(Value::as_array)
            .map(|required| required.iter().filter_map(Value::as_str).collect::<HashSet<_>>())
            .unwrap_or_default();

        if let Some(properties) = schema_value.get("properties").and_then(Value::as_object) {
            for (field_name, field_schema) in properties {
                let mut field_type = self.parse(field_schema)?;

                if !required_fields.contains(field_name.as_str()) {
                    field_type = WorkflowType::nullable(field_type);
                }

                fields.insert(field_name.clone(), field_type);
            }
        }

        if fields.is_empty() && schema_value.get("additionalProperties") != Some(&Value::Bool(false)) {
            return Ok(WorkflowType::AnyObject);
        }

        Ok(WorkflowType::Object(fields))
    }

    fn unsupported(message: impl Into<String>) -> WorkflowSemanticError {
        WorkflowSemanticError::Other { message: message.into() }
    }
}

fn normalize_union_members(union_members: Vec<WorkflowType>) -> WorkflowType {
    normalize_union_members_checked(union_members.clone())
        .unwrap_or_else(|_| normalize_union_members_without_variant_aggregation(union_members))
}

fn normalize_union_members_checked(mut union_members: Vec<WorkflowType>) -> Result<WorkflowType, WorkflowSemanticError> {
    aggregate_variant_union_members(&mut union_members)?;

    Ok(normalize_union_members_without_variant_aggregation(union_members))
}

fn aggregate_variant_union_members(union_members: &mut Vec<WorkflowType>) -> Result<(), WorkflowSemanticError> {
    let mut discriminator = None::<String>;
    let mut aggregated_cases = BTreeMap::new();
    let mut variant_member_count = 0_usize;

    for union_member in union_members.iter() {
        let WorkflowType::Variant {
            discriminator: member_discriminator,
            cases,
        } = union_member
        else {
            continue;
        };
        variant_member_count += 1;

        if discriminator
            .as_ref()
            .is_some_and(|discriminator| discriminator != member_discriminator)
        {
            return Err(WorkflowSemanticError::Other {
                message: format!(
                    "union variant members use incompatible discriminators `{}` and `{member_discriminator}`",
                    discriminator.as_deref().unwrap_or_default()
                ),
            });
        }

        discriminator.get_or_insert_with(|| member_discriminator.clone());

        for (case_name, case_fields) in cases {
            if let Some(existing_case_fields) = aggregated_cases.get(case_name) {
                if existing_case_fields != case_fields {
                    return Err(WorkflowSemanticError::Other {
                        message: format!("union variant case `{case_name}` has incompatible field schemas"),
                    });
                }

                continue;
            }

            aggregated_cases.insert(case_name.clone(), case_fields.clone());
        }
    }

    if variant_member_count < 2 {
        return Ok(());
    }

    union_members.retain(|union_member| !matches!(union_member, WorkflowType::Variant { .. }));
    union_members.push(WorkflowType::Variant {
        discriminator: discriminator.unwrap_or_default(),
        cases: aggregated_cases,
    });

    Ok(())
}

fn normalize_union_members_without_variant_aggregation(union_members: Vec<WorkflowType>) -> WorkflowType {
    let mut flattened_members = Vec::new();

    for union_member in union_members {
        if let WorkflowType::Union(nested_union_members) = union_member {
            flattened_members.extend(nested_union_members);

            continue;
        }

        flattened_members.push(union_member);
    }

    let mut collected_string_enums = Vec::<String>::new();
    let mut deduplicated_members = Vec::new();
    let mut seen_member_keys = HashSet::new();

    for union_member in flattened_members {
        if let WorkflowType::StringEnum(enum_values) = union_member {
            collected_string_enums.extend(enum_values);

            continue;
        }

        let member_key = union_member.schema_cache_key();

        if seen_member_keys.insert(member_key) {
            deduplicated_members.push(union_member);
        }
    }

    if !collected_string_enums.is_empty() {
        collected_string_enums.sort();
        collected_string_enums.dedup();
        deduplicated_members.push(WorkflowType::StringEnum(collected_string_enums));
    }

    deduplicated_members.sort_by_key(WorkflowType::schema_cache_key);

    if deduplicated_members.len() == 1 {
        return deduplicated_members.into_iter().next().expect("single union member should exist");
    }

    WorkflowType::Union(deduplicated_members)
}

pub fn validate_value_against_type(value: &Value, expected_type: &WorkflowType) -> Result<(), String> {
    expected_type.validate_value(value)
}

pub fn validate_value_against_json_schema(value: &Value, schema: &Value) -> Result<(), String> {
    let validator = jsonschema::validator_for(schema).map_err(|compile_error| format!("failed to compile JSON schema: {compile_error}"))?;
    let mut validation_issues = validator.iter_errors(value).map(format_validation_issue).collect::<Vec<_>>();

    if validation_issues.is_empty() {
        return Ok(());
    }

    validation_issues.sort();
    validation_issues.dedup();

    Err(validation_issues.join("; "))
}

#[must_use]
pub fn workflow_type_to_json_schema(workflow_type: &WorkflowType) -> Value {
    workflow_type.json_schema_value()
}

pub fn workflow_type_to_schemars_schema(
    workflow_type: &WorkflowType,
    schema_description: Option<&str>,
) -> Result<Schema, WorkflowSemanticError> {
    let mut json_schema_value = workflow_type_to_json_schema(workflow_type);

    if let Some(schema_description) = schema_description {
        if let Some(schema_object) = json_schema_value.as_object_mut() {
            schema_object.insert("description".to_string(), Value::String(schema_description.to_string()));
        }
    }

    serde_json::from_value::<Schema>(json_schema_value).map_err(|error| WorkflowSemanticError::Other {
        message: format!("failed to convert workflow type into schemars schema: {error}"),
    })
}

fn format_validation_issue(validation_error: ValidationError<'_>) -> String {
    let instance_path = normalize_instance_path(&validation_error.instance_path().to_string());
    let validation_message = validation_error.masked().to_string();

    if instance_path == "$" {
        return format!("{instance_path}: {validation_message}");
    }

    format!("{instance_path}: {validation_message}")
}

fn normalize_instance_path(instance_path: &str) -> String {
    if instance_path.is_empty() {
        return "$".to_string();
    }

    let mut normalized_path = String::from("$");

    for path_segment in instance_path.trim_start_matches('/').split('/') {
        if path_segment.is_empty() {
            continue;
        }

        if path_segment.chars().all(|character| character.is_ascii_digit()) {
            normalized_path.push('[');
            normalized_path.push_str(path_segment);
            normalized_path.push(']');

            continue;
        }

        normalized_path.push('.');
        normalized_path.push_str(path_segment);
    }

    normalized_path
}

pub fn parse_number_literal(number_literal: &str) -> Result<Number, WorkflowSemanticError> {
    let normalized_number_literal = number_literal.replace('_', "");

    if let Ok(integer_value) = normalized_number_literal.parse::<i64>() {
        return Ok(Number::from(integer_value));
    }

    if let Ok(unsigned_integer_value) = normalized_number_literal.parse::<u64>() {
        return Ok(Number::from(unsigned_integer_value));
    }

    if !normalized_number_literal.contains('.') {
        return Err(WorkflowSemanticError::Other {
            message: format!("integer literal `{number_literal}` is outside the supported 64-bit range"),
        });
    }

    let float_value = normalized_number_literal
        .parse::<f64>()
        .map_err(|error| WorkflowSemanticError::Other {
            message: format!("invalid number literal `{number_literal}`: {error}"),
        })?;

    let Some(serialized_number) = Number::from_f64(float_value) else {
        return Err(WorkflowSemanticError::Other {
            message: format!("invalid floating number literal `{number_literal}`"),
        });
    };

    Ok(serialized_number)
}

#[must_use]
pub fn ensure_type_matches(expected_type: &WorkflowType, actual_type: &WorkflowType) -> bool {
    let expected_type = expected_type.clone().normalize();
    let actual_type = actual_type.clone().normalize();

    if matches!(expected_type, WorkflowType::Any) || matches!(actual_type, WorkflowType::Any) {
        return true;
    }

    match (&expected_type, &actual_type) {
        (WorkflowType::Float, WorkflowType::Integer)
        | (WorkflowType::AnyObject, WorkflowType::Object(_))
        | (WorkflowType::String, WorkflowType::StringEnum(_)) => true,
        (
            WorkflowType::Array {
                item_type: expected_item_type,
                fixed_length: expected_fixed_length,
            },
            WorkflowType::Array {
                item_type: actual_item_type,
                fixed_length: actual_fixed_length,
            },
        ) => expected_fixed_length == actual_fixed_length && ensure_type_matches(expected_item_type, actual_item_type),
        (WorkflowType::Tuple(expected_item_types), WorkflowType::Tuple(actual_item_types)) => {
            expected_item_types.len() == actual_item_types.len()
                && expected_item_types
                    .iter()
                    .zip(actual_item_types)
                    .all(|(expected_item_type, actual_item_type)| ensure_type_matches(expected_item_type, actual_item_type))
        }
        (WorkflowType::Object(expected_fields), WorkflowType::Object(actual_fields)) => {
            actual_fields.keys().all(|field_name| expected_fields.contains_key(field_name))
                && expected_fields.iter().all(|(field_name, expected_field_type)| {
                    actual_fields.get(field_name).map_or_else(
                        || expected_field_type.can_be_null(),
                        |actual_field_type| ensure_type_matches(expected_field_type, actual_field_type),
                    )
                })
        }
        (
            WorkflowType::Variant {
                discriminator: expected_discriminator,
                cases: expected_cases,
            },
            WorkflowType::Variant {
                discriminator: actual_discriminator,
                cases: actual_cases,
            },
        ) => {
            expected_discriminator == actual_discriminator
                && expected_cases.len() == actual_cases.len()
                && expected_cases.iter().all(|(case_name, expected_fields)| {
                    actual_cases.get(case_name).is_some_and(|actual_fields| {
                        ensure_type_matches(
                            &WorkflowType::Object(expected_fields.clone()),
                            &WorkflowType::Object(actual_fields.clone()),
                        )
                    })
                })
        }
        (WorkflowType::Union(expected_members), WorkflowType::Union(actual_members)) => {
            expected_members.len() == actual_members.len()
                && expected_members.iter().all(|expected_member| {
                    actual_members
                        .iter()
                        .any(|actual_member| ensure_type_matches(expected_member, actual_member))
                })
        }
        (WorkflowType::Union(expected_members), _) => expected_members
            .iter()
            .any(|expected_member| ensure_type_matches(expected_member, &actual_type)),
        (_, WorkflowType::Union(actual_members)) => actual_members
            .iter()
            .all(|actual_member| ensure_type_matches(&expected_type, actual_member)),
        _ => expected_type == actual_type,
    }
}

#[must_use]
pub fn value_kind_name(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(number_value) => {
            if number_value.is_i64() || number_value.is_u64() {
                "number"
            } else {
                "float"
            }
        }
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

#[cfg(test)]
mod tests {
    use super::{ensure_type_matches, workflow_type_from_dsl, workflow_type_to_json_schema, WorkflowSchemaCache, WorkflowType};
    use serde_json::json;
    use std::collections::{BTreeMap, HashMap};
    use superwire_types::ast::{SourcePosition, SourceSpan, TypeExpression, TypedField, VariantCase};

    #[test]
    fn numeric_compatibility_only_widens_integer_to_float() {
        assert!(ensure_type_matches(&WorkflowType::Float, &WorkflowType::Integer));
        assert!(!ensure_type_matches(&WorkflowType::Integer, &WorkflowType::Float));
    }

    #[test]
    fn schema_cache_keys_distinguish_delimiter_shaped_enum_values() {
        let first_type = WorkflowType::StringEnum(vec!["alpha|beta".to_string(), "gamma".to_string()]);
        let second_type = WorkflowType::StringEnum(vec!["alpha".to_string(), "beta|gamma".to_string()]);

        assert_ne!(first_type.schema_cache_key(), second_type.schema_cache_key());

        let mut schema_cache = WorkflowSchemaCache::new();
        let first_schema = first_type.json_schema_value_with_cache(&mut schema_cache);
        let second_schema = second_type.json_schema_value_with_cache(&mut schema_cache);

        assert_ne!(first_schema, second_schema);
        assert_eq!(first_schema, first_type.json_schema_value());
        assert_eq!(second_schema, second_type.json_schema_value());
        assert_eq!(schema_cache.len(), 2);
    }

    #[test]
    fn rejects_empty_type_expression_collections_from_direct_ast_construction() {
        let named_schemas = HashMap::<String, TypeExpression>::new();
        let empty_union_error =
            workflow_type_from_dsl(&TypeExpression::Union(Vec::new()), &named_schemas).expect_err("empty union AST should be rejected");
        let empty_variant_error = workflow_type_from_dsl(
            &TypeExpression::Variant {
                discriminator: "kind".to_string(),
                cases: Vec::new(),
            },
            &named_schemas,
        )
        .expect_err("empty variant AST should be rejected");

        assert!(empty_union_error.to_string().contains("requires at least one member"));
        assert!(empty_variant_error.to_string().contains("requires at least one case"));
    }
    #[test]
    fn lowers_nullable_enum_to_string_enum_or_null() {
        let type_expression = TypeExpression::nullable(TypeExpression::Union(vec![
            TypeExpression::StringEnum("draft".to_string()),
            TypeExpression::StringEnum("ready".to_string()),
        ]));

        let workflow_type =
            workflow_type_from_dsl(&type_expression, &HashMap::<String, TypeExpression>::new()).expect("nullable enum should lower");

        assert_eq!(
            workflow_type,
            WorkflowType::Union(vec![
                WorkflowType::StringEnum(vec!["draft".to_string(), "ready".to_string()]),
                WorkflowType::Null
            ])
        );
    }

    #[test]
    fn checks_type_compatibility_for_nested_and_nullable_types() {
        struct TypeCompatibilityCase {
            expected_type: WorkflowType,
            actual_type: WorkflowType,
            matches: bool,
        }

        let expected_object_fields = BTreeMap::from([
            ("project_id".to_string(), WorkflowType::Integer),
            ("title".to_string(), WorkflowType::String),
        ]);
        let actual_object_fields = BTreeMap::from([
            ("title".to_string(), WorkflowType::String),
            ("project_id".to_string(), WorkflowType::Integer),
        ]);
        let missing_object_fields = BTreeMap::from([("project_id".to_string(), WorkflowType::Integer)]);
        let type_compatibility_cases = [
            TypeCompatibilityCase {
                expected_type: WorkflowType::String,
                actual_type: WorkflowType::String,
                matches: true,
            },
            TypeCompatibilityCase {
                expected_type: WorkflowType::Any,
                actual_type: WorkflowType::Object(actual_object_fields.clone()),
                matches: true,
            },
            TypeCompatibilityCase {
                expected_type: WorkflowType::Object(expected_object_fields.clone()),
                actual_type: WorkflowType::Object(actual_object_fields),
                matches: true,
            },
            TypeCompatibilityCase {
                expected_type: WorkflowType::Object(expected_object_fields),
                actual_type: WorkflowType::Object(missing_object_fields),
                matches: false,
            },
            TypeCompatibilityCase {
                expected_type: WorkflowType::Array {
                    item_type: Box::new(WorkflowType::String),
                    fixed_length: Some(2),
                },
                actual_type: WorkflowType::Array {
                    item_type: Box::new(WorkflowType::String),
                    fixed_length: Some(3),
                },
                matches: false,
            },
            TypeCompatibilityCase {
                expected_type: WorkflowType::nullable(WorkflowType::String),
                actual_type: WorkflowType::Union(vec![WorkflowType::Null, WorkflowType::String]),
                matches: true,
            },
            TypeCompatibilityCase {
                expected_type: WorkflowType::String,
                actual_type: WorkflowType::nullable(WorkflowType::String),
                matches: false,
            },
        ];

        for type_compatibility_case in type_compatibility_cases {
            assert_eq!(
                ensure_type_matches(&type_compatibility_case.expected_type, &type_compatibility_case.actual_type),
                type_compatibility_case.matches
            );
        }
    }

    #[test]
    fn projects_json_values_to_declared_object_fields() {
        let workflow_type = WorkflowType::Object(BTreeMap::from([
            ("name".to_string(), WorkflowType::String),
            (
                "answers".to_string(),
                WorkflowType::Array {
                    item_type: Box::new(WorkflowType::Object(BTreeMap::from([("text".to_string(), WorkflowType::String)]))),
                    fixed_length: None,
                },
            ),
        ]));
        let value = json!({
            "name": "survey",
            "extra": "ignored",
            "answers": [
                {
                    "text": "hello",
                    "score": 10
                }
            ]
        });

        assert_eq!(
            workflow_type.project_json_value(&value),
            json!({
                "name": "survey",
                "answers": [
                    {
                        "text": "hello"
                    }
                ]
            })
        );
    }

    #[test]
    fn projects_nested_variant_values_using_parent_discriminator() {
        let workflow_type = WorkflowType::Object(BTreeMap::from([(
            "answers".to_string(),
            WorkflowType::Array {
                item_type: Box::new(WorkflowType::Object(BTreeMap::from([
                    (
                        "answer".to_string(),
                        WorkflowType::Variant {
                            discriminator: "task_type".to_string(),
                            cases: BTreeMap::from([(
                                "open_written".to_string(),
                                BTreeMap::from([("text".to_string(), WorkflowType::String)]),
                            )]),
                        },
                    ),
                    ("task_type".to_string(), WorkflowType::String),
                ]))),
                fixed_length: None,
            },
        )]));
        let value = json!({
            "answers": [
                {
                    "task_type": "open_written",
                    "answer": {
                        "text": "hello world",
                        "ignored": true
                    }
                }
            ]
        });

        assert_eq!(
            workflow_type.project_json_value(&value),
            json!({
                "answers": [
                    {
                        "task_type": "open_written",
                        "answer": {
                            "task_type": "open_written",
                            "text": "hello world"
                        }
                    }
                ]
            })
        );
    }

    #[test]
    fn leaves_untyped_json_values_unfiltered() {
        let value = json!({
            "name": "survey",
            "extra": "retained"
        });

        assert_eq!(WorkflowType::Any.project_json_value(&value), value);
    }

    #[test]
    fn maps_variant_to_discriminated_json_schema() {
        let type_expression = TypeExpression::Variant {
            discriminator: "type".to_string(),
            cases: vec![VariantCase {
                name: "user_created".to_string(),
                fields: vec![TypedField {
                    name: "user_id".to_string(),
                    field_type: TypeExpression::String,
                    description: None,
                    span: generated_span(),
                }],
                span: generated_span(),
            }],
        };
        let workflow_type =
            workflow_type_from_dsl(&type_expression, &HashMap::<String, TypeExpression>::new()).expect("variant should lower");
        let json_schema = workflow_type_to_json_schema(&workflow_type);

        assert_eq!(json_schema["discriminator"]["propertyName"], json!("type"));
        assert_eq!(json_schema["oneOf"][0]["properties"]["type"]["const"], json!("user_created"));
        assert_eq!(json_schema["oneOf"][0]["required"], json!(["type", "user_id"]));
    }

    #[test]
    fn cached_schema_conversion_matches_uncached_conversion() {
        let workflow_type = WorkflowType::Object(BTreeMap::from([
            (
                "items".to_string(),
                WorkflowType::Array {
                    item_type: Box::new(WorkflowType::String),
                    fixed_length: Some(2),
                },
            ),
            ("status".to_string(), WorkflowType::StringEnum(vec!["ready".to_string()])),
        ]));
        let mut schema_cache = WorkflowSchemaCache::new();

        assert_eq!(
            workflow_type.json_schema_value_with_cache(&mut schema_cache),
            workflow_type_to_json_schema(&workflow_type)
        );
        assert!(!schema_cache.is_empty());
    }

    #[test]
    fn schema_cache_evicts_when_capacity_is_reached() {
        let mut schema_cache = WorkflowSchemaCache::with_capacity(1);

        let _string_schema = WorkflowType::String.json_schema_value_with_cache(&mut schema_cache);
        let _integer_schema = WorkflowType::Integer.json_schema_value_with_cache(&mut schema_cache);

        assert_eq!(schema_cache.len(), 1);
    }

    fn generated_span() -> SourceSpan {
        SourceSpan {
            start: SourcePosition { line: 1, column: 1 },
            end: SourcePosition { line: 1, column: 1 },
        }
    }
}
