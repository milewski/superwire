#![allow(dead_code, unused_macros)]

pub mod fixtures;
pub mod runner;

macro_rules! input {
    ($input:tt $(,)?) => {
        serde_json::json!($input)
    };
}

macro_rules! secret {
    ($secrets:tt $(,)?) => {
        serde_json::json!($secrets)
    };
}

macro_rules! call {
    ($name:expr, $arguments:tt $(,)?) => {
        crate::support::runner::ToolCall::new($name, serde_json::json!($arguments))
    };
}

macro_rules! schema {
    () => {
        serde_json::to_value(schemars::json_schema!({
            "type": "object",
            "properties": {},
            "required": [],
            "additionalProperties": false,
        }))
        .expect("test schema should serialize")
    };

    ($($field_name:ident : $field_type:ty),+ $(,)?) => {{
        #[derive(schemars::JsonSchema)]
        #[allow(dead_code)]
        struct TestSchema {
            $($field_name: $field_type,)*
        }

        let mut schema = serde_json::to_value(schemars::schema_for!(TestSchema)).expect("test schema should serialize");

        if let Some(schema_object) = schema.as_object_mut() {
            schema_object.remove("$schema");
            schema_object.remove("title");
        }

        schema
    }};
}
