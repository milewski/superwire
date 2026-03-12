/// Macro to format validation errors with consistent structure
#[macro_export]
macro_rules! format_validation_error {
    ($error_type:expr, $location:expr, $suggestion:expr) => {{
        let mut message = $error_type;

        if let Some(suggestion) = $suggestion {
            message.push_str("\n  = help: ");
            message.push_str(&suggestion);
        }

        format!(
            "Error: {}\n  --> {}:{}:{}\n",
            message, $location.file_path, $location.line, $location.column
        )
    }};
}

/// Macro to create validation error builders
#[macro_export]
macro_rules! validation_error {
    (DuplicateName { name: $name:expr, span: $span:expr, first: $first:expr }) => {
        $crate::validation::error::ValidationError::DuplicateName {
            file_path: "workflow".to_string(),
            line: $span.line,
            column: $span.column,
            name: $name,
            first_defined_at: format!("{}:{}", $first.line, $first.column),
            suggestion: Some(format!("Rename one of the '{}' definitions", $name)),
        }
    };

    (UndefinedReference { reference: $ref:expr, span: $span:expr, available: $avail:expr }) => {
        $crate::validation::error::ValidationError::UndefinedReference {
            file_path: "workflow".to_string(),
            line: $span.line,
            column: $span.column,
            reference: $ref,
            suggestion: if $avail.is_empty() {
                None
            } else {
                Some(format!("Available: {}", $avail.join(", ")))
            },
        }
    };

    (InvalidProperty { property: $prop:expr, span: $span:expr, expected: $exp:expr }) => {
        $crate::validation::error::ValidationError::InvalidProperty {
            file_path: "workflow".to_string(),
            line: $span.line,
            column: $span.column,
            property_name: $prop,
            expected_type: $exp,
            suggestion: Some(format!("Property '{}' should be of type {}", $prop, $exp)),
        }
    };
}

/// Trait for validation rules
pub trait ValidationRule {
    fn name(&self) -> &'static str;
    fn validate(&self, workflow: &crate::ast::Workflow, errors: &mut Vec<crate::validation::error::ValidationError>);
}

/// Macro to implement a validation rule
#[macro_export]
macro_rules! impl_validation_rule {
    (
        $name:ident,
        $rule_name:expr,
        |$workflow:ident, $errors:ident| $body:block
    ) => {
        pub struct $name;

        impl $crate::validation::macros::ValidationRule for $name {
            fn name(&self) -> &'static str {
                $rule_name
            }

            fn validate(
                &self,
                $workflow: &$crate::ast::Workflow,
                $errors: &mut Vec<crate::validation::error::ValidationError>,
            ) {
                $body
            }
        }
    };
}
