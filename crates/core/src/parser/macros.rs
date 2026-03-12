/// Macros for reducing parser boilerplate
///
/// Macro to check for duplicates in a collection
#[macro_export]
macro_rules! check_duplicates {
    ($collection:expr, $name_getter:expr, $error_constructor:expr, $errors:expr) => {{
        let mut seen = std::collections::HashMap::new();

        for item in $collection {
            let name = $name_getter(item);

            if let Some(first_location) = seen.get(name) {
                $errors.push($error_constructor(item, first_location));
            } else {
                seen.insert(name.clone(), item);
            }
        }
    }};
}

/// Macro to create a span
#[macro_export]
macro_rules! make_span {
    ($line:expr, $column:expr) => {
        $crate::ast::Span {
            line: $line,
            column: $column,
        }
    };
}
