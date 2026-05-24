#![allow(dead_code, unused_macros)]

pub mod fixtures;
pub mod runner;

macro_rules! call {
    ($name:expr, $arguments:tt $(,)?) => {
        crate::support::runner::ToolCall::new($name, serde_json::json!($arguments))
    };
}

macro_rules! schema {
    () => {
        superwire_core::testing::empty_object_schema()
    };

    ($($field_name:ident : $field_type:ty),+ $(,)?) => {{
        #[derive(schemars::JsonSchema)]
        #[allow(dead_code)]
        struct TestSchema {
            $($field_name: $field_type,)*
        }

        superwire_core::testing::schema_for_type::<TestSchema>()
    }};
}
