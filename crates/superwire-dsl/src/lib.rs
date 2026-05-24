pub mod diagnostic {
    pub use superwire_types::diagnostic::*;
}

pub mod dsl;

pub use dsl::*;

#[cfg(test)]
macro_rules! parse_inline_workflow {
    (
        $(#$base_workflow:expr;)+
        $($workflow_tokens:tt)*
    ) => {{
        let mut merged_workflow = $crate::Workflow {
            declarations: Vec::new(),
            source_text: None,
        };

        $(
            let included_workflow: &$crate::Workflow = &($base_workflow);
            merged_workflow
                .declarations
                .extend(included_workflow.declarations().iter().cloned());
        )*

        let workflow_source_template = $crate::testing::WorkflowSourceTemplate::from_inline(stringify!($($workflow_tokens)*));
        let parsed_workflow = workflow_source_template.parse_workflow().unwrap_or_else(|parse_error| {
            panic!(
                "inline workflow failed to parse:\n{}",
                parse_error.render_with_source(workflow_source_template.source(), "<inline workflow>")
            )
        });

        merged_workflow.declarations.extend(parsed_workflow.declarations);

        merged_workflow
    }};

    ($($workflow_tokens:tt)*) => {{
        let workflow_source_template = $crate::testing::WorkflowSourceTemplate::from_inline(stringify!($($workflow_tokens)*));
        workflow_source_template.parse_workflow().unwrap_or_else(|parse_error| {
            panic!(
                "inline workflow failed to parse:\n{}",
                parse_error.render_with_source(workflow_source_template.source(), "<inline workflow>")
            )
        })
    }};
}

#[cfg(test)]
pub(crate) use parse_inline_workflow;

#[cfg(test)]
macro_rules! workflow_source {
    ($($workflow_tokens:tt)*) => {
        stringify!($($workflow_tokens)*)
    };
}

#[cfg(test)]
pub(crate) use workflow_source;

#[cfg(test)]
mod testing {
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct WorkflowSourceTemplate {
        source_text: String,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct SnapshotAssertion {
        pub name: String,
        pub expected: String,
        pub actual: String,
    }

    impl WorkflowSourceTemplate {
        #[must_use]
        pub fn from_inline(source_text: impl Into<String>) -> Self {
            Self {
                source_text: Self::normalize_rust_doc_comment_tokens(&source_text.into()),
            }
        }

        #[must_use]
        pub fn source(&self) -> &str {
            &self.source_text
        }

        pub fn parse_workflow(&self) -> Result<crate::Workflow, crate::DslParseError> {
            crate::parse_workflow(&self.source_text)
        }

        #[must_use]
        fn normalize_rust_doc_comment_tokens(source_template: &str) -> String {
            let mut normalized_source = String::new();
            let mut remaining_source = source_template;

            while let Some(doc_attribute_start) = remaining_source.find("#[doc = r\"") {
                normalized_source.push_str(&remaining_source[..doc_attribute_start]);
                remaining_source = &remaining_source[doc_attribute_start + "#[doc = r\"".len()..];

                let Some(doc_attribute_end) = remaining_source.find("\"]") else {
                    normalized_source.push_str("#[doc = r\"");
                    normalized_source.push_str(remaining_source);

                    return normalized_source;
                };

                normalized_source.push_str("///");
                normalized_source.push_str(&remaining_source[..doc_attribute_end]);
                normalized_source.push('\n');
                remaining_source = &remaining_source[doc_attribute_end + "\"]".len()..];
            }

            normalized_source.push_str(remaining_source);
            normalized_source
        }
    }

    impl SnapshotAssertion {
        #[must_use]
        pub fn new(name: impl Into<String>, expected: impl Into<String>, actual: impl Into<String>) -> Self {
            Self {
                name: name.into(),
                expected: expected.into(),
                actual: actual.into(),
            }
        }

        pub fn assert_matches(&self) {
            assert_eq!(self.actual, self.expected, "snapshot {} did not match", self.name);
        }
    }
}
