use std::borrow::Cow;

use schemars::{JsonSchema, Schema, SchemaGenerator};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub struct {{agent_input_type_name}}(Value);

impl JsonSchema for {{agent_input_type_name}} {
    fn schema_name() -> Cow<'static, str> {
        Cow::Borrowed("{{agent_input_type_name}}")
    }

    fn json_schema(schema_generator: &mut SchemaGenerator) -> Schema {
        let _ = schema_generator;

        serde_json::from_str::<Schema>("{{agent_input_schema_json}}").expect("agent input schema json should be valid")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub struct {{bound_input_type_name}}(Value);

impl JsonSchema for {{bound_input_type_name}} {
    fn schema_name() -> Cow<'static, str> {
        Cow::Borrowed("{{bound_input_type_name}}")
    }

    fn json_schema(schema_generator: &mut SchemaGenerator) -> Schema {
        let _ = schema_generator;

        serde_json::from_str::<Schema>("{{bound_input_schema_json}}").expect("bound input schema json should be valid")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub struct {{output_type_name}}(Value);

impl JsonSchema for {{output_type_name}} {
    fn schema_name() -> Cow<'static, str> {
        Cow::Borrowed("{{output_type_name}}")
    }

    fn json_schema(schema_generator: &mut SchemaGenerator) -> Schema {
        let _ = schema_generator;

        serde_json::from_str::<Schema>("{{output_schema_json}}").expect("output schema json should be valid")
    }
}

crate::php_proxy_tool!(
    tool = {{tool_type_name}}Tool,
    name = "{{tool_name}}",
    description = "{{tool_description}}",
    endpoint = "{{tool_endpoint}}",
    input = {{agent_input_type_name}},
    bound_input = {{bound_input_type_name}},
    output = {{output_type_name}},
);
