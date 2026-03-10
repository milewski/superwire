// Macro to create input maps for workflow tests
#[macro_export]
macro_rules! input {
    ($($key:ident: $value:expr),* $(,)?) => {{
        let mut inputs = std::collections::HashMap::new();
        $(
            inputs.insert(
                stringify!($key).to_string(),
                serde_json::json!($value)
            );
        )*
        inputs
    }};
}

// Helper function to extract clean test name from function path
// Converts "workflow_tests::test_basic_workflow::{{closure}}::{{closure}}"
// to "test_basic_workflow"
pub fn extract_test_name(full_path: &str) -> &str {
    full_path
        .split("::")
        .find(|segment| segment.starts_with("test_"))
        .unwrap_or(full_path)
}

// Macro to execute workflow and return typed result
// Automatically captures the test function name
// Usage: workflow!("path/to/workflow.ai" => OutputType)
#[macro_export]
macro_rules! workflow {
    ($path:expr => $output_type:ty) => {{
        let test_name = crate::helpers::macros::extract_test_name(stdext::function_name!());
        async move {
            let content = include_str!($path);
            let value = crate::helpers::executor::execute_cached_workflow_from_content(
                test_name,
                $path,
                content,
                std::collections::HashMap::new(),
            )
            .await
            .unwrap();
            serde_json::from_value::<$output_type>(value).unwrap()
        }
    }};

    ($inputs:expr => $path:expr => $output_type:ty) => {{
        let test_name = crate::helpers::macros::extract_test_name(stdext::function_name!());
        async move {
            let content = include_str!($path);
            let value =
                crate::helpers::executor::execute_cached_workflow_from_content(test_name, $path, content, $inputs)
                    .await
                    .unwrap();
            serde_json::from_value::<$output_type>(value).unwrap()
        }
    }};
}
