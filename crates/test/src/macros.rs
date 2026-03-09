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

#[macro_export]
macro_rules! try_workflow {
    ($path:expr) => {{
        async {
            let engine = engine_ai_core::execution::engine::ExecutionEngine::new();
            let content = include_str!($path);
            engine.execute_workflow_from_content(content, $path).await
        }
    }};

    ($path:expr, $inputs:expr) => {{
        async {
            let engine = engine_ai_core::execution::engine::ExecutionEngine::new();
            let content = include_str!($path);
            engine
                .execute_workflow_from_content_with_inputs(content, $path, $inputs)
                .await
        }
    }};
}

#[macro_export]
macro_rules! workflow {
    ($path:expr) => {{
        async {
            let engine = engine_ai_core::execution::engine::ExecutionEngine::new();
            let content = include_str!($path);
            engine.execute_workflow_from_content(content, $path).await.unwrap()
        }
    }};

    ($path:expr, $inputs:expr) => {{
        async {
            let engine = engine_ai_core::execution::engine::ExecutionEngine::new();
            let content = include_str!($path);
            engine
                .execute_workflow_from_content_with_inputs(content, $path, $inputs)
                .await
                .unwrap()
        }
    }};
}

#[macro_export]
macro_rules! assert_output {
    ($output:expr, $($path:expr => $check:ident),+ $(,)?) => {{
        assert!($output.is_object(), "Output is not an object");
        $(
            let value = $output.pointer($path).unwrap_or_else(|| panic!("Path {} not found in output", $path));
            assert!(value.$check(), "Value at {} is not {}", $path, stringify!($check));
        )+
    }};
}
