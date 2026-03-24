#[macro_export]
macro_rules! parse_inline_workflow {
    ($($workflow_tokens:tt)*) => {{
        $crate::dsl::parse_workflow(stringify!($($workflow_tokens)*))
            .unwrap_or_else(|parse_error| panic!("inline workflow failed to parse: {parse_error}"))
    }};
}

pub use parse_inline_workflow;
