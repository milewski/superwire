//! Macros for simplifying formatter tests

/// Macro to create readable multi-line formatter test cases
///
/// Usage:
/// ```
/// readable_format_test!(test_name, {
///     input: code! {
///         workflow:
///           step1:
///         action: test
///     },
///     expected: code! {
///         workflow:
///             step1:
///                 action: test
///     },
/// });
/// ```
#[macro_export]
macro_rules! readable_format_test {
    ($test_name:ident, {
        input: code! { $($input:tt)* },
        expected: code! { $($expected:tt)* },
    }) => {
        #[test]
        fn $test_name() {
            use engine_ai_core::formatter::Formatter;

            let input = crate::formatter::macros::normalize_test_indentation(stringify!($($input)*));
            let expected = crate::formatter::macros::normalize_test_indentation(stringify!($($expected)*));

            let formatter = Formatter::new();
            let result = formatter.format_content(&input).unwrap();

            assert_eq!(result, expected,
                "Formatting failed for test '{}'\n\nInput:\n{}\n\nExpected:\n{}\n\nActual:\n{}",
                stringify!($test_name), input, expected, result
            );
        }
    };

    ($test_name:ident, {
        input: code! { $($input:tt)* },
        expected: code! { $($expected:tt)* },
        config: $config:expr,
    }) => {
        #[test]
        fn $test_name() {
            use engine_ai_core::formatter::{Formatter, FormatterConfig};

            let input = crate::formatter::macros::normalize_test_indentation(stringify!($($input)*));
            let expected = crate::formatter::macros::normalize_test_indentation(stringify!($($expected)*));

            let formatter = Formatter::with_config($config);
            let result = formatter.format_content(&input).unwrap();

            assert_eq!(result, expected,
                "Formatting failed for test '{}'\n\nInput:\n{}\n\nExpected:\n{}\n\nActual:\n{}",
                stringify!($test_name), input, expected, result
            );
        }
    };
}

/// Macro to test that multi-line content should remain unchanged after formatting
///
/// Usage:
/// ```
/// readable_format_unchanged!(test_name, code! {
///     workflow:
///         step1:
///             action: test
/// });
/// ```
#[macro_export]
macro_rules! readable_format_unchanged {
    ($test_name:ident, code! { $($content:tt)* }) => {
        #[test]
        fn $test_name() {
            use engine_ai_core::formatter::Formatter;

            let content = crate::formatter::macros::normalize_test_indentation(stringify!($($content)*));
            let formatter = Formatter::new();
            let result = formatter.format_content(&content).unwrap();

            assert_eq!(result, content,
                "Content should remain unchanged for test '{}'\n\nOriginal:\n{}\n\nFormatted:\n{}",
                stringify!($test_name), content, result
            );
        }
    };
}

/// Helper function to normalize indentation in test strings
/// This removes the leading/trailing whitespace and normalizes indentation
/// so that test cases can be written in a readable format
pub fn normalize_test_indentation(input: &str) -> String {
    // Handle single-line stringify output by converting braces to newlines
    let mut processed = input.to_string();

    // Add newlines around braces to make it more readable
    processed = processed.replace(" {", " {\n");
    processed = processed.replace("{ ", "{\n");
    processed = processed.replace(" }", "\n}");
    processed = processed.replace("}", "\n}");

    // Clean up multiple consecutive newlines
    while processed.contains("\n\n\n") {
        processed = processed.replace("\n\n\n", "\n\n");
    }

    let lines: Vec<&str> = processed.lines().collect();

    // Skip empty lines at start and end
    let start = lines.iter().position(|line| !line.trim().is_empty()).unwrap_or(0);
    let end = lines.iter().rposition(|line| !line.trim().is_empty()).map(|i| i + 1).unwrap_or(lines.len());

    if start >= end {
        return String::new();
    }

    let content_lines = &lines[start..end];

    // Find minimum indentation (excluding empty lines)
    let min_indent = content_lines
        .iter()
        .filter(|line| !line.trim().is_empty())
        .map(|line| line.len() - line.trim_start().len())
        .min()
        .unwrap_or(0);

    // Remove common indentation and join
    let normalized_lines: Vec<String> = content_lines
        .iter()
        .map(|line| {
            if line.trim().is_empty() {
                String::new()
            } else {
                line.chars().skip(min_indent).collect()
            }
        })
        .collect();

    let mut result = normalized_lines.join("\n");

    // Ensure final newline
    if !result.is_empty() && !result.ends_with('\n') {
        result.push('\n');
    }

    result
}

/// Macro to create formatter test cases with input and expected output
///
/// Usage:
/// ```
/// format_test!(test_name, {
///     input: "unformatted content",
///     expected: "formatted content",
/// });
/// ```
#[macro_export]
macro_rules! format_test {
    ($test_name:ident, {
        input: $input:expr,
        expected: $expected:expr,
    }) => {
        #[test]
        fn $test_name() {
            use engine_ai_core::formatter::Formatter;

            let formatter = Formatter::new();
            let result = formatter.format_content($input).unwrap();

            assert_eq!(
                result,
                $expected,
                "Formatting failed for test '{}'\nInput:\n{}\nExpected:\n{}\nActual:\n{}",
                stringify!($test_name),
                $input,
                $expected,
                result
            );
        }
    };

    ($test_name:ident, {
        input: $input:expr,
        expected: $expected:expr,
        config: $config:expr,
    }) => {
        #[test]
        fn $test_name() {
            use engine_ai_core::formatter::{Formatter, FormatterConfig};

            let formatter = Formatter::with_config($config);
            let result = formatter.format_content($input).unwrap();

            assert_eq!(
                result,
                $expected,
                "Formatting failed for test '{}'\nInput:\n{}\nExpected:\n{}\nActual:\n{}",
                stringify!($test_name),
                $input,
                $expected,
                result
            );
        }
    };
}

/// Macro to test that content should remain unchanged after formatting
///
/// Usage:
/// ```
/// format_unchanged!(test_name, "content that should not change");
/// ```
#[macro_export]
macro_rules! format_unchanged {
    ($test_name:ident, $content:expr) => {
        #[test]
        fn $test_name() {
            use engine_ai_core::formatter::Formatter;

            let formatter = Formatter::new();
            let result = formatter.format_content($content).unwrap();

            assert_eq!(
                result,
                $content,
                "Content should remain unchanged for test '{}'\nOriginal:\n{}\nFormatted:\n{}",
                stringify!($test_name),
                $content,
                result
            );
        }
    };
}

/// Macro to test formatting with file operations
///
/// Usage:
/// ```
/// format_file_test!(test_name, {
///     input_file: "test_input.ai",
///     expected_file: "test_expected.ai",
/// });
/// ```
#[macro_export]
macro_rules! format_file_test {
    ($test_name:ident, {
        input_file: $input_file:expr,
        expected_file: $expected_file:expr,
    }) => {
        #[test]
        fn $test_name() {
            use engine_ai_core::formatter::Formatter;
            use std::path::Path;

            let test_data_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests")
                .join("formatter")
                .join("test_data");

            let input_path = test_data_dir.join($input_file);
            let expected_path = test_data_dir.join($expected_file);

            let formatter = Formatter::new();
            let result = formatter.format_file(&input_path).unwrap();

            let expected_content = std::fs::read_to_string(&expected_path).expect(&format!(
                "Failed to read expected file: {}",
                expected_path.display()
            ));

            assert_eq!(
                result.content,
                expected_content,
                "File formatting failed for test '{}'\nInput file: {}\nExpected file: {}",
                stringify!($test_name),
                input_path.display(),
                expected_path.display()
            );
        }
    };
}

/// Macro to create multiple related format tests
///
/// Usage:
/// ```
/// format_test_suite!(indentation, [
///     (spaces_to_tabs, {
///         input: "    content",
///         expected: "    content",
///     }),
///     (mixed_indentation, {
///         input: "\t  content",
///         expected: "      content",
///     }),
/// ]);
/// ```
#[macro_export]
macro_rules! format_test_suite {
    ($suite_name:ident, [
        $(($test_name:ident, {
            input: $input:expr,
            expected: $expected:expr,
        }),)*
    ]) => {
        mod $suite_name {
            use super::*;

            $(
                format_test!($test_name, {
                    input: $input,
                    expected: $expected,
                });
            )*
        }
    };

    ($suite_name:ident, [
        $(($test_name:ident, {
            input: $input:expr,
            expected: $expected:expr,
            config: $config:expr,
        }),)*
    ]) => {
        mod $suite_name {
            use super::*;

            $(
                format_test!($test_name, {
                    input: $input,
                    expected: $expected,
                    config: $config,
                });
            )*
        }
    };
}

/// Helper function to create temporary test files
pub fn create_temp_ai_file(content: &str) -> std::io::Result<tempfile::NamedTempFile> {
    use std::io::Write;

    let mut temp_file = tempfile::Builder::new().suffix(".ai").tempfile()?;

    temp_file.write_all(content.as_bytes())?;
    temp_file.flush()?;

    Ok(temp_file)
}

/// Helper function to create temporary test directory with .ai files
pub fn create_temp_test_dir(files: &[(&str, &str)]) -> std::io::Result<tempfile::TempDir> {
    use std::fs;
    use std::io::Write;

    let temp_dir = tempfile::tempdir()?;

    for (filename, content) in files {
        let file_path = temp_dir.path().join(filename);

        // Create parent directories if needed
        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let mut file = fs::File::create(&file_path)?;
        file.write_all(content.as_bytes())?;
    }

    Ok(temp_dir)
}
