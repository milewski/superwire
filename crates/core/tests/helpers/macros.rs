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
// For single-value outputs, the macro will auto-unwrap if the workflow returns a single-field object
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

            // Try to deserialize directly first
            match serde_json::from_value::<$output_type>(value.clone()) {
                Ok(result) => result,
                Err(_) => {
                    // If direct deserialization fails, try to unwrap single-field object
                    if let serde_json::Value::Object(map) = value {
                        if map.len() == 1 {
                            let (_, value) = map.into_iter().next().unwrap();
                            serde_json::from_value::<$output_type>(value).unwrap()
                        } else {
                            panic!("Cannot deserialize workflow output to target type");
                        }
                    } else {
                        panic!("Cannot deserialize workflow output to target type");
                    }
                }
            }
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

            // Try to deserialize directly first
            match serde_json::from_value::<$output_type>(value.clone()) {
                Ok(result) => result,
                Err(_) => {
                    // If direct deserialization fails, try to unwrap single-field object
                    if let serde_json::Value::Object(map) = value {
                        if map.len() == 1 {
                            let (_key, val) = map.into_iter().next().unwrap();
                            serde_json::from_value::<$output_type>(val).unwrap()
                        } else {
                            panic!("Cannot deserialize workflow output to target type");
                        }
                    } else {
                        panic!("Cannot deserialize workflow output to target type");
                    }
                }
            }
        }
    }};
}
