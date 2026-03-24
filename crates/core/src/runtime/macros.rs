#[macro_export]
macro_rules! try_workflow {
    ($workflow_path:literal) => {{ async move { $crate::runtime::try_workflow_from_source(include_str!($workflow_path)).await } }};
    ($workflow_path:literal, input = $input_values:expr) => {{
        async move {
            $crate::runtime::try_workflow_from_source_with_values(include_str!($workflow_path), $input_values, serde_json::json!({})).await
        }
    }};
    ($workflow_path:literal, input = $input_values:expr, secrets = $secret_values:expr) => {{
        async move {
            $crate::runtime::try_workflow_from_source_with_values(include_str!($workflow_path), $input_values, $secret_values).await
        }
    }};
    ($workflow:expr) => {{
        async move {
            let workflow = $workflow;

            $crate::runtime::try_workflow_from_workflow(&workflow).await
        }
    }};
    ($workflow:expr, input = $input_values:expr) => {{
        async move {
            let workflow = $workflow;

            $crate::runtime::try_workflow_from_workflow_with_values(&workflow, $input_values, serde_json::json!({})).await
        }
    }};
    ($workflow:expr, input = $input_values:expr, secrets = $secret_values:expr) => {{
        async move {
            let workflow = $workflow;

            $crate::runtime::try_workflow_from_workflow_with_values(&workflow, $input_values, $secret_values).await
        }
    }};
}

pub use try_workflow;
