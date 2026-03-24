#[macro_export]
macro_rules! try_workflow {
    ($workflow_path:expr) => {{ async move { $crate::runtime::try_workflow_from_source(include_str!($workflow_path)).await } }};
    ($workflow_path:expr, input = $input_values:expr) => {{
        async move {
            $crate::runtime::try_workflow_from_source_with_values(include_str!($workflow_path), $input_values, serde_json::json!({})).await
        }
    }};
    ($workflow_path:expr, input = $input_values:expr, secrets = $secret_values:expr) => {{
        async move {
            $crate::runtime::try_workflow_from_source_with_values(include_str!($workflow_path), $input_values, $secret_values).await
        }
    }};
}

pub use try_workflow;
