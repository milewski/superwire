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
    ({ $($field_name:ident : $field_type:ident),* $(,)? }) => {
        serde_json::to_value(schemars::json_schema!({
            "type": "object",
            "properties": {
                $(stringify!($field_name): schema!(@type $field_type)),*
            },
            "required": [$(stringify!($field_name)),*],
            "additionalProperties": false,
        }))
        .expect("test schema should serialize")
    };

    (@type string) => {
        schemars::json_schema!({ "type": "string" })
    };

    (@type number) => {
        schemars::json_schema!({ "type": "integer" })
    };

    (@type float) => {
        schemars::json_schema!({ "type": "number" })
    };

    (@type boolean) => {
        schemars::json_schema!({ "type": "boolean" })
    };

    ($schema:tt $(,)?) => {
        serde_json::to_value(schemars::json_schema!($schema)).expect("test schema should serialize")
    };
}
