#[macro_export]
macro_rules! parse_inline_workflow {
    (
        $(#$base_workflow:expr;)+
        $($workflow_tokens:tt)*
    ) => {{
        let mut merged_workflow = $crate::dsl::Workflow {
            declarations: Vec::new(),
            source_text: None,
        };

        $(
            let included_workflow: &$crate::dsl::Workflow = &($base_workflow);
            merged_workflow
                .declarations
                .extend(included_workflow.declarations().iter().cloned());
        )*

        let workflow_source = stringify!($($workflow_tokens)*);
        let parsed_workflow = $crate::dsl::parse_workflow(workflow_source).unwrap_or_else(|parse_error| {
            panic!(
                "inline workflow failed to parse:\n{}",
                parse_error.render_with_source(workflow_source, "<inline workflow>")
            )
        });

        merged_workflow.declarations.extend(parsed_workflow.declarations);

        merged_workflow
    }};

    ($($workflow_tokens:tt)*) => {{
        let workflow_source = stringify!($($workflow_tokens)*);
        $crate::dsl::parse_workflow(workflow_source).unwrap_or_else(|parse_error| {
            panic!(
                "inline workflow failed to parse:\n{}",
                parse_error.render_with_source(workflow_source, "<inline workflow>")
            )
        })
    }};
}

pub use parse_inline_workflow;

#[macro_export]
macro_rules! workflow_source {
    ($($workflow_tokens:tt)*) => {
        stringify!($($workflow_tokens)*)
    };
}

pub use workflow_source;
