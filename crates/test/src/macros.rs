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

#[doc(hidden)]
#[must_use]
pub fn current_test_name(function_name: &str) -> &str {
    function_name
        .rsplit("::")
        .find(|segment| !segment.is_empty() && *segment != "{{closure}}")
        .unwrap_or("unknown")
}

#[macro_export]
macro_rules! try_workflow {
    ($path:expr) => {{
        let test_name = $crate::current_test_name(stdext::function_name!());

        async move {
            let content = include_str!($path);
            $crate::executor::execute_cached_workflow_from_content(
                test_name,
                $path,
                content,
                std::collections::HashMap::new(),
            )
            .await
        }
    }};

    ($inputs:expr => $path:expr) => {{
        let test_name = $crate::current_test_name(stdext::function_name!());

        async move {
            let content = include_str!($path);
            $crate::executor::execute_cached_workflow_from_content(test_name, $path, content, $inputs).await
        }
    }};

    ($test_name:expr, $path:expr) => {{
        async {
            let content = include_str!($path);
            $crate::executor::execute_cached_workflow_from_content(
                $test_name,
                $path,
                content,
                std::collections::HashMap::new(),
            )
            .await
        }
    }};

    ($test_name:expr, $inputs:expr => $path:expr) => {{
        async {
            let content = include_str!($path);
            $crate::executor::execute_cached_workflow_from_content($test_name, $path, content, $inputs).await
        }
    }};
}

#[macro_export]
macro_rules! workflow {
    ($path:expr) => {{
        let test_name = $crate::current_test_name(stdext::function_name!());

        async move {
            let content = include_str!($path);
            $crate::executor::execute_cached_workflow_from_content(
                test_name,
                $path,
                content,
                std::collections::HashMap::new(),
            )
            .await
            .unwrap()
        }
    }};

    ($path:expr => $output_type:ty) => {{
        let test_name = $crate::current_test_name(stdext::function_name!());

        async move {
            let content = include_str!($path);
            let value = $crate::executor::execute_cached_workflow_from_content(
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

    ($inputs:expr => $path:expr) => {{
        let test_name = $crate::current_test_name(stdext::function_name!());

        async move {
            let content = include_str!($path);
            $crate::executor::execute_cached_workflow_from_content(test_name, $path, content, $inputs)
                .await
                .unwrap()
        }
    }};

    ($inputs:expr => $path:expr => $output_type:ty) => {{
        let test_name = $crate::current_test_name(stdext::function_name!());

        async move {
            let content = include_str!($path);
            let value = $crate::executor::execute_cached_workflow_from_content(test_name, $path, content, $inputs)
                .await
                .unwrap();
            serde_json::from_value::<$output_type>(value).unwrap()
        }
    }};

    ($test_name:expr, $path:expr) => {{
        async {
            let content = include_str!($path);
            $crate::executor::execute_cached_workflow_from_content(
                $test_name,
                $path,
                content,
                std::collections::HashMap::new(),
            )
            .await
            .unwrap()
        }
    }};

    ($test_name:expr, $path:expr => $output_type:ty) => {{
        async {
            let content = include_str!($path);
            let value = $crate::executor::execute_cached_workflow_from_content(
                $test_name,
                $path,
                content,
                std::collections::HashMap::new(),
            )
            .await
            .unwrap();
            serde_json::from_value::<$output_type>(value).unwrap()
        }
    }};

    ($test_name:expr, $inputs:expr => $path:expr) => {{
        async {
            let content = include_str!($path);
            $crate::executor::execute_cached_workflow_from_content($test_name, $path, content, $inputs)
                .await
                .unwrap()
        }
    }};

    ($test_name:expr, $inputs:expr => $path:expr => $output_type:ty) => {{
        async {
            let content = include_str!($path);
            let value = $crate::executor::execute_cached_workflow_from_content($test_name, $path, content, $inputs)
                .await
                .unwrap();
            serde_json::from_value::<$output_type>(value).unwrap()
        }
    }};
}
