/// Macro for generating error formatter functions with consistent structure
///
/// This macro reduces boilerplate by automatically generating formatter functions
/// that follow the same pattern: format a message and delegate to a base formatter.
///
/// # Usage
///
/// ```ignore
/// define_error_formatters! {
///     base_formatter: format_validation_error,
///     formatters: [
///         (format_duplicate_name, "duplicate name '{}' (first defined at {})", [name: &str, first_defined_at: &str]),
///         (format_undefined_reference, "undefined reference '{}'", [reference: &str]),
///     ]
/// }
/// ```
#[macro_export]
macro_rules! define_error_formatters {
    (
        base_formatter: $base_fn:ident,
        formatters: [
            $(
                (
                    $fn_name:ident,
                    $message_template:expr,
                    [$($param_name:ident: $param_type:ty),*]
                )
            ),* $(,)?
        ]
    ) => {
        $(
            fn $fn_name(
                file_path: &str,
                line: usize,
                column: usize,
                $($param_name: $param_type,)*
                suggestion: Option<&String>,
            ) -> String {
                $base_fn(
                    &format!($message_template, $($param_name),*),
                    file_path,
                    line,
                    column,
                    suggestion,
                )
            }
        )*
    };
}

/// Macro for generating simple error formatter functions
///
/// This is for cases where the formatter follows a simple pattern without a base formatter.
///
/// # Usage
///
/// ```ignore
/// define_simple_error_formatters! {
///     (format_undefined_reference, "undefined reference '{}'", [reference: &str]),
///     (format_template_mismatch, "template variable mismatch: {}", [message: &str]),
/// }
/// ```
#[macro_export]
macro_rules! define_simple_error_formatters {
    (
        $(
            (
                $fn_name:ident,
                $message_template:expr,
                [$($param_name:ident: $param_type:ty),*]
            )
        ),* $(,)?
    ) => {
        $(
            fn $fn_name(
                file_path: &str,
                line: usize,
                column: usize,
                $($param_name: $param_type,)*
                suggestion: Option<&String>,
            ) -> String {
                use std::fmt::Write;
                let mut result = format!("Error: {}\n  --> {}:{}:{}\n   |", format!($message_template, $($param_name),*), file_path, line, column);

                if let Some(suggestion_text) = suggestion {
                    write!(result, "\n   = help: {suggestion_text}").unwrap();
                }

                result
            }
        )*
    };
}

/// Macro for generating error formatter functions that don't use a base formatter
///
/// This is for cases where the formatter needs custom logic beyond simple message formatting.
///
/// # Usage
///
/// ```ignore
/// define_custom_error_formatters! {
///     (format_syntax_error, [message: &str, source_line: Option<&String>], {
///         let mut result = format!("Error: {message}\n  --> {file_path}:{line}:{column}\n   |");
///         // custom logic here...
///         result
///     }),
/// }
/// ```
#[macro_export]
macro_rules! define_custom_error_formatters {
    (
        $(
            (
                $fn_name:ident,
                [$($param_name:ident: $param_type:ty),*],
                $body:expr
            )
        ),* $(,)?
    ) => {
        $(
            fn $fn_name(
                file_path: &str,
                line: usize,
                column: usize,
                $($param_name: $param_type,)*
                suggestion: Option<&String>,
            ) -> String {
                $body
            }
        )*
    };
}
