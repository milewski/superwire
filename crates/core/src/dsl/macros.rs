#[macro_export]
macro_rules! parse_inline_workflow {
    (
        $(#$base_workflow:expr;)+
        $($workflow_tokens:tt)*
    ) => {{
        let mut merged_workflow = $crate::dsl::Workflow {
            declarations: Vec::new(),
        };

        $(
            let included_workflow: &$crate::dsl::Workflow = &($base_workflow);
            merged_workflow
                .declarations
                .extend(included_workflow.declarations().iter().cloned());
        )*

        let parsed_workflow = $crate::dsl::parse_workflow(stringify!($($workflow_tokens)*))
            .unwrap_or_else(|parse_error| panic!("inline workflow failed to parse: {parse_error}"));

        merged_workflow.declarations.extend(parsed_workflow.declarations);

        merged_workflow
    }};

    ($($workflow_tokens:tt)*) => {{
        $crate::dsl::parse_workflow(stringify!($($workflow_tokens)*))
            .unwrap_or_else(|parse_error| panic!("inline workflow failed to parse: {parse_error}"))
    }};
}

pub use parse_inline_workflow;
