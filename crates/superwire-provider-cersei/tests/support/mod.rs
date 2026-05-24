#![allow(dead_code, unused_macros)]

pub mod runner;

#[allow(unused_imports)]
pub use superwire_test_support::fixtures;

macro_rules! call {
    ($name:expr, $arguments:tt $(,)?) => {
        crate::support::runner::ToolCall::new($name, serde_json::json!($arguments))
    };
}

macro_rules! schema {
    () => {
        superwire_test_support::empty_object_schema()
    };

    ($($field_name:ident : $field_type:ty),+ $(,)?) => {{
        #[derive(schemars::JsonSchema)]
        #[allow(dead_code)]
        struct TestSchema {
            $($field_name: $field_type,)*
        }

        superwire_test_support::schema_for_type::<TestSchema>()
    }};
}
