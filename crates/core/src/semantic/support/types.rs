use crate::dsl::{Reference, TypeExpression, TypedField};
use crate::semantic::WorkflowSemanticError;
use jsonschema::ValidationError;
use schemars::{JsonSchema, Schema};
use serde_json::{json, Number, Value};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt::{Display, Formatter};
use std::hash::BuildHasher;

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

impl WorkflowType {
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
        match self {
            Self::Object(fields) => fields.get(field_name).cloned(),
            Self::Variant { discriminator, cases } => {
                if discriminator == field_name {
                    return Some(Self::StringEnum(cases.keys().cloned().collect()));
                }

                None
            }
            Self::Union(members) => {
                let field_types = members
                    .iter()
                    .filter(|member| !matches!(member, Self::Null))
                    .filter_map(|member| member.field_type(field_name))
                    .collect::<Vec<_>>();

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
            | Self::Array {
                item_type: _,
                fixed_length: _,
            }
            | Self::Tuple(_) => None,
        }
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
        match self {
            Self::Variant { discriminator: _, cases } => {
                let case_fields = cases.get(case_name)?;
                let (first_field_name, remaining_field_path) = field_path.split_first()?;
                let mut current_type = case_fields.get(first_field_name)?.clone();

                for field_name in remaining_field_path {
                    current_type = current_type.field_type(field_name)?;
                }

                Some(current_type)
            }
            Self::Union(members) => members
                .iter()
                .find_map(|member| member.variant_case_field_type(case_name, field_path)),
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
    pub fn variant_case_names(&self) -> Option<Vec<String>> {
        match self {
            Self::Variant { discriminator: _, cases } => Some(cases.keys().cloned().collect()),
            Self::Union(members) => members.iter().find_map(Self::variant_case_names),
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

    fn canonical_key(&self) -> String {
        match self {
            Self::Any => "any".to_string(),
            Self::String => "string".to_string(),
            Self::Integer => "integer".to_string(),
            Self::Float => "float".to_string(),
            Self::Boolean => "boolean".to_string(),
            Self::Null => "null".to_string(),
            Self::AnyObject => "object".to_string(),
            Self::StringEnum(enum_values) => format!("enum({})", enum_values.join("|")),
            Self::Array { item_type, fixed_length } => {
                let Some(fixed_length) = fixed_length else {
                    return format!("array({})", item_type.canonical_key());
                };

                format!("array({};{fixed_length})", item_type.canonical_key())
            }
            Self::Tuple(item_types) => {
                let joined_item_keys = item_types.iter().map(Self::canonical_key).collect::<Vec<_>>().join(",");

                format!("tuple({joined_item_keys})")
            }
            Self::Object(fields) => {
                let field_pairs = fields
                    .iter()
                    .map(|(field_name, field_type)| format!("{field_name}:{}", field_type.canonical_key()))
                    .collect::<Vec<_>>()
                    .join(",");

                format!("object({field_pairs})")
            }
            Self::Variant { discriminator, cases } => {
                let case_pairs = cases
                    .iter()
                    .map(|(case_name, fields)| {
                        let field_pairs = fields
                            .iter()
                            .map(|(field_name, field_type)| format!("{field_name}:{}", field_type.canonical_key()))
                            .collect::<Vec<_>>()
                            .join(",");

                        format!("{case_name}({field_pairs})")
                    })
                    .collect::<Vec<_>>()
                    .join("|");

                format!("variant({discriminator};{case_pairs})")
            }
            Self::Union(members) => {
                let mut member_keys = members.iter().map(Self::canonical_key).collect::<Vec<_>>();

                member_keys.sort();

                format!("union({})", member_keys.join("|"))
            }
        }
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

impl TypeExpression {
    pub fn to_workflow_type<HashBuilder: BuildHasher>(
        &self,
        named_schemas: &HashMap<String, TypeExpression, HashBuilder>,
    ) -> Result<WorkflowType, WorkflowSemanticError> {
        workflow_type_from_dsl_with_stack(self, named_schemas, &mut Vec::new()).map(WorkflowType::normalize)
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
            let mut resolved_members = Vec::with_capacity(members.len());

            for union_member in members {
                resolved_members.push(workflow_type_from_dsl_with_stack(union_member, named_schemas, resolution_stack)?);
            }

            Ok(WorkflowType::Union(resolved_members))
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

impl Reference {
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

    let definitions = extract_schema_definitions(&schema_value);

    parse_json_schema(&schema_value, &definitions).map(WorkflowType::normalize)
}

fn extract_schema_definitions(root_schema: &Value) -> HashMap<String, Value> {
    let mut definitions = HashMap::new();

    if let Some(definition_entries) = root_schema.get("$defs").and_then(Value::as_object) {
        for (definition_name, definition_schema) in definition_entries {
            definitions.insert(definition_name.clone(), definition_schema.clone());
        }
    }

    if let Some(definition_entries) = root_schema.get("definitions").and_then(Value::as_object) {
        for (definition_name, definition_schema) in definition_entries {
            definitions.insert(definition_name.clone(), definition_schema.clone());
        }
    }

    definitions
}

fn parse_json_schema(schema_value: &Value, definitions: &HashMap<String, Value>) -> Result<WorkflowType, WorkflowSemanticError> {
    if schema_value.is_boolean() {
        return Err(WorkflowSemanticError::Other {
            message: "unsupported boolean schema; dynamic schemas are not allowed".to_string(),
        });
    }

    if let Some(reference) = schema_value.get("$ref").and_then(Value::as_str) {
        return parse_reference_schema(reference, definitions);
    }

    if let Some(enum_values) = schema_value.get("enum").and_then(Value::as_array) {
        let mut parsed_enum_values = Vec::new();

        for enum_value in enum_values {
            let Some(string_value) = enum_value.as_str() else {
                return Err(WorkflowSemanticError::Other {
                    message: format!("unsupported non-string enum variant in schema: {enum_value}"),
                });
            };

            parsed_enum_values.push(string_value.to_string());
        }

        parsed_enum_values.sort();
        parsed_enum_values.dedup();

        return Ok(WorkflowType::StringEnum(parsed_enum_values));
    }

    if let Some(any_of_schemas) = schema_value.get("anyOf").and_then(Value::as_array) {
        return parse_union_schema(any_of_schemas, definitions);
    }

    if let Some(one_of_schemas) = schema_value.get("oneOf").and_then(Value::as_array) {
        return parse_union_schema(one_of_schemas, definitions);
    }

    if let Some(type_entry) = schema_value.get("type") {
        if let Some(type_name) = type_entry.as_str() {
            return parse_type_name(type_name, schema_value, definitions);
        }

        if let Some(type_names) = type_entry.as_array() {
            let mut union_members = Vec::new();

            for type_name in type_names {
                let Some(type_name) = type_name.as_str() else {
                    return Err(WorkflowSemanticError::Other {
                        message: format!("invalid schema `type` entry: {type_name}"),
                    });
                };

                union_members.push(parse_type_name(type_name, schema_value, definitions)?);
            }

            return Ok(WorkflowType::Union(union_members));
        }
    }

    if let Some(constant_value) = schema_value.get("const") {
        if constant_value.is_null() {
            return Ok(WorkflowType::Null);
        }

        if let Some(string_constant) = constant_value.as_str() {
            return Ok(WorkflowType::StringEnum(vec![string_constant.to_string()]));
        }
    }

    Err(WorkflowSemanticError::Other {
        message: format!("unsupported schema shape: {schema_value}"),
    })
}

fn parse_reference_schema(reference: &str, definitions: &HashMap<String, Value>) -> Result<WorkflowType, WorkflowSemanticError> {
    let Some(reference_name) = reference
        .strip_prefix("#/$defs/")
        .or_else(|| reference.strip_prefix("#/definitions/"))
    else {
        return Err(WorkflowSemanticError::Other {
            message: format!("unsupported schema reference path `{reference}`"),
        });
    };

    let Some(referenced_schema) = definitions.get(reference_name) else {
        return Err(WorkflowSemanticError::Other {
            message: format!("missing schema definition for reference `{reference}`"),
        });
    };

    parse_json_schema(referenced_schema, definitions)
}

fn parse_union_schema(union_members: &[Value], definitions: &HashMap<String, Value>) -> Result<WorkflowType, WorkflowSemanticError> {
    let mut parsed_members = Vec::new();

    for union_member in union_members {
        parsed_members.push(parse_json_schema(union_member, definitions)?);
    }

    Ok(WorkflowType::Union(parsed_members))
}

fn parse_type_name(
    type_name: &str,
    schema_value: &Value,
    definitions: &HashMap<String, Value>,
) -> Result<WorkflowType, WorkflowSemanticError> {
    match type_name {
        "string" => Ok(WorkflowType::String),
        "integer" => Ok(WorkflowType::Integer),
        "number" => Ok(WorkflowType::Float),
        "boolean" => Ok(WorkflowType::Boolean),
        "null" => Ok(WorkflowType::Null),
        "array" => parse_array_schema(schema_value, definitions),
        "object" => parse_object_schema(schema_value, definitions),
        _ => Err(WorkflowSemanticError::Other {
            message: format!("unsupported schema type `{type_name}`"),
        }),
    }
}

fn parse_array_schema(schema_value: &Value, definitions: &HashMap<String, Value>) -> Result<WorkflowType, WorkflowSemanticError> {
    if let Some(prefix_items) = schema_value.get("prefixItems").and_then(Value::as_array) {
        let mut tuple_items = Vec::new();

        for prefix_item in prefix_items {
            tuple_items.push(parse_json_schema(prefix_item, definitions)?);
        }

        return Ok(WorkflowType::Tuple(tuple_items));
    }

    let item_type = match schema_value.get("items") {
        Some(item_schema) => parse_json_schema(item_schema, definitions)?,
        None => {
            return Err(WorkflowSemanticError::Other {
                message: "array schema must include `items`".to_string(),
            });
        }
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

fn parse_object_schema(schema_value: &Value, definitions: &HashMap<String, Value>) -> Result<WorkflowType, WorkflowSemanticError> {
    let mut fields = BTreeMap::new();

    if let Some(properties) = schema_value.get("properties").and_then(Value::as_object) {
        for (field_name, field_schema) in properties {
            fields.insert(field_name.clone(), parse_json_schema(field_schema, definitions)?);
        }
    }

    if fields.is_empty() && schema_value.get("additionalProperties") != Some(&Value::Bool(false)) {
        return Ok(WorkflowType::AnyObject);
    }

    Ok(WorkflowType::Object(fields))
}

fn normalize_union_members(union_members: Vec<WorkflowType>) -> WorkflowType {
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

        let member_key = union_member.canonical_key();

        if seen_member_keys.insert(member_key) {
            deduplicated_members.push(union_member);
        }
    }

    if !collected_string_enums.is_empty() {
        collected_string_enums.sort();
        collected_string_enums.dedup();
        deduplicated_members.push(WorkflowType::StringEnum(collected_string_enums));
    }

    deduplicated_members.sort_by_key(WorkflowType::canonical_key);

    if deduplicated_members.len() == 1 {
        return deduplicated_members.into_iter().next().expect("single union member should exist");
    }

    WorkflowType::Union(deduplicated_members)
}

pub fn validate_value_against_type(value: &Value, expected_type: &WorkflowType) -> Result<(), String> {
    let schema = workflow_type_to_json_schema(expected_type);
    let validator = jsonschema::validator_for(&schema)
        .map_err(|compile_error| format!("failed to compile generated schema for `{expected_type}`: {compile_error}"))?;

    let mut validation_issues = validator.iter_errors(value).map(format_validation_issue).collect::<Vec<_>>();

    if validation_issues.is_empty() {
        return Ok(());
    }

    validation_issues.sort();
    validation_issues.dedup();

    Err(validation_issues.join("; "))
}

pub fn workflow_type_to_json_schema(workflow_type: &WorkflowType) -> Value {
    match workflow_type {
        WorkflowType::Any => json!({}),
        WorkflowType::String => json!({ "type": "string" }),
        WorkflowType::Integer => json!({ "type": "integer" }),
        WorkflowType::Float => json!({ "type": "number" }),
        WorkflowType::Boolean => json!({ "type": "boolean" }),
        WorkflowType::Null => json!({ "type": "null" }),
        WorkflowType::AnyObject => json!({ "type": "object" }),
        WorkflowType::StringEnum(enum_values) => json!({
            "type": "string",
            "enum": enum_values,
        }),
        WorkflowType::Array { item_type, fixed_length } => {
            let mut array_schema = json!({
                "type": "array",
                "items": workflow_type_to_json_schema(item_type),
            });

            if let Some(fixed_length) = fixed_length {
                array_schema["minItems"] = json!(fixed_length);
                array_schema["maxItems"] = json!(fixed_length);
            }

            array_schema
        }
        WorkflowType::Tuple(tuple_items) => json!({
            "type": "array",
            "prefixItems": tuple_items.iter().map(workflow_type_to_json_schema).collect::<Vec<_>>(),
            "minItems": tuple_items.len(),
            "maxItems": tuple_items.len(),
        }),
        WorkflowType::Object(object_fields) => {
            let properties = object_fields
                .iter()
                .map(|(field_name, field_type)| (field_name.clone(), workflow_type_to_json_schema(field_type)))
                .collect::<serde_json::Map<_, _>>();

            let required = object_fields.keys().cloned().collect::<Vec<_>>();

            json!({
                "type": "object",
                "properties": properties,
                "required": required,
                "additionalProperties": false,
            })
        }
        WorkflowType::Variant { discriminator, cases } => json!({
            "oneOf": cases
                .iter()
                .map(|(case_name, fields)| {
                    let mut properties = fields
                        .iter()
                        .map(|(field_name, field_type)| (field_name.clone(), workflow_type_to_json_schema(field_type)))
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
        WorkflowType::Union(union_members) => json!({
            "oneOf": union_members
                .iter()
                .map(workflow_type_to_json_schema)
                .collect::<Vec<_>>(),
        }),
    }
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

    if instance_path == "$" {
        return validation_error.to_string();
    }

    format!("{instance_path}: {validation_error}")
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
            expected_fields.len() == actual_fields.len()
                && expected_fields.iter().all(|(field_name, expected_field_type)| {
                    actual_fields
                        .get(field_name)
                        .is_some_and(|actual_field_type| ensure_type_matches(expected_field_type, actual_field_type))
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
    use super::{ensure_type_matches, workflow_type_from_dsl, workflow_type_to_json_schema, WorkflowType};
    use crate::dsl::{SourcePosition, SourceSpan, TypeExpression, TypedField, VariantCase};
    use serde_json::json;
    use std::collections::{BTreeMap, HashMap};

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

    fn generated_span() -> SourceSpan {
        SourceSpan {
            start: SourcePosition { line: 1, column: 1 },
            end: SourcePosition { line: 1, column: 1 },
        }
    }
}
