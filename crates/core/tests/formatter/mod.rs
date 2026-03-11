//! Formatter module tests

pub mod macros;

use engine_ai_core::formatter::{Formatter, FormatterConfig};
use std::path::Path;

// Re-export macros for easier use
pub use crate::format_test;
pub use crate::format_test_suite;
pub use crate::format_unchanged;
pub use crate::format_file_test;
pub use crate::readable_format_test;
pub use crate::readable_format_unchanged;

#[cfg(test)]
mod tests {
    use super::*;
    use macros::{create_temp_ai_file, create_temp_test_dir};

    // Basic formatting tests using readable format
    mod indentation_readable {
        use super::*;

        format_test!(normalize_tabs_to_spaces, {
            input: "workflow:\n\tstep1:\n\t\taction: test",
            expected: "workflow:\nstep1:\naction: test\n",
        });

        readable_format_test!(mixed_indentation_complex, {
            input: code! {
                provider ollama {
                 driver <- "ollama"
                	api_endpoint <- "http://localhost:11434"
                  models <- ["llama2"]
                }
            },
            expected: code! {
                provider ollama {
                    driver <- "ollama"
                    api_endpoint <- "http://localhost:11434"
                    models <- ["llama2"]
                }
            },
        });

        format_test!(nested_workflow_structure, {
            input: "workflow:\n step1:\n   action: test\n   params:\n     value: 123\n step2:\n   action: another",
            expected: "workflow:\nstep1:\naction: test\nparams:\nvalue: 123\nstep2:\naction: another\n",
        });
    }

    mod colon_spacing_readable {
        use super::*;

        format_test!(basic_colon_spacing, {
            input: "output <- {\n    files:[string]\n    count   :    number\n    status  : boolean\n}",
            expected: "output <- {\n    files: [string]\n    count: number\n    status: boolean\n}\n",
        });

        format_test!(nested_schema_colons, {
            input: "agent test {\n    output <- {\n        data:{\n            name:string\n            value  :   number\n        }\n        status:boolean\n    }\n}",
            expected: "agent test {\n    output <- {\n        data: {\n            name: string\n            value: number\n        }\n        status: boolean\n    }\n}\n",
        });

        readable_format_test!(preserve_urls, {
            input: code! {
                provider test {
                    api_endpoint <- "http://localhost:8080"
                    secure_endpoint <- "https://api.example.com:443/v1"
                }
            },
            expected: code! {
                provider test {
                    api_endpoint <- "http://localhost:8080"
                    secure_endpoint <- "https://api.example.com:443/v1"
                }
            },
        });
    }

    mod schema_properties_working {
        use super::*;

        format_test!(multiline_conversion, {
            input: "output <- { files: [string] count: number }",
            expected: "output <- {\n    files: [string]\n    count: number\n}\n",
        });

        format_test!(description_spacing_fix, {
            input: "files:[string]\"comment\"",
            expected: "files: [string] \"comment\"\n",
        });

        format_test!(single_property_unchanged, {
            input: "output <- { single: [string] }",
            expected: "output <- { single: [string] }\n",
        });
    }

    mod triple_quote_working {
        use super::*;

        format_test!(fix_unindented_content, {
            input: "prompt <- \"\"\"\nwhat files can you see\n\"\"\"",
            expected: "prompt <- \"\"\"\n    what files can you see\n\"\"\"\n",
        });

        format_test!(preserve_proper_indentation, {
            input: "prompt <- \"\"\"\n    this content is already properly indented\n\"\"\"",
            expected: "prompt <- \"\"\"\n    this content is already properly indented\n\"\"\"\n",
        });

        format_test!(nested_indentation, {
            input: "agent test {\n    prompt <- \"\"\"\nsome content\n    \"\"\"\n}",
            expected: "agent test {\n    prompt <- \"\"\"\n        some content\n    \"\"\"\n}\n",
        });
    }

    mod multiline_strings_readable {
        use super::*;

        readable_format_test!(long_string_conversion, {
            input: code! {
                prompt <- "This is a very long string that should be converted to multiline format because it exceeds the 120 character limit and makes the line too long to read comfortably"
            },
            expected: code! {
                prompt <- """
    This is a very long string that should be converted to multiline format because it exceeds the 120 character limit and makes the line too long to read comfortably
"""
            },
        });

        format_test!(short_string_unchanged, {
            input: "prompt <- \"This is a short string\"",
            expected: "prompt <- \"This is a short string\"\n",
        });

        readable_format_test!(nested_long_string, {
            input: code! {
                agent assistant {
                    prompt <- "This is another very long string that should be converted to multiline format because it exceeds the character limit and needs proper formatting for readability"
                    model <- "test"
                }
            },
            expected: code! {
                agent assistant {
                    prompt <- """
        This is another very long string that should be converted to multiline format because it exceeds the character limit and needs proper formatting for readability
    """
                    model <- "test"
                }
            },
        });
    }

    // Simple working tests with correct 4-space indentation expectations
    mod working_tests {
        use super::*;

        format_test!(basic_operator_spacing, {
            input: "model<-\"test\"",
            expected: "model <- \"test\"\n",
        });

        format_test!(basic_colon_spacing_simple, {
            input: "files:[string]",
            expected: "files: [string]\n",
        });

        format_test!(basic_indentation, {
            input: "agent test {\nmodel <- \"test\"\n}",
            expected: "agent test {\n    model <- \"test\"\n}\n",
        });

        format_test!(schema_block_simple, {
            input: "output <- { files: [string] count: number }",
            expected: "output <- {\n    files: [string]\n    count: number\n}\n",
        });

        format_test!(triple_quote_basic, {
            input: "prompt <- \"\"\"\nsome content\n\"\"\"",
            expected: "prompt <- \"\"\"\n    some content\n\"\"\"\n",
        });
    }

    // Keep some of the original simple tests for edge cases
    format_test_suite!(basic_whitespace, [
        (trim_trailing_spaces, {
            input: "content   \nmore content  ",
            expected: "content\nmore content\n",
        }),
        (preserve_internal_spaces, {
            input: "content with  multiple   spaces",
            expected: "content with  multiple   spaces\n",
        }),
        (empty_lines_with_spaces, {
            input: "line1\n   \nline2",
            expected: "line1\n\nline2\n",
        }),
    ]);

    format_test_suite!(blank_lines, [
        (remove_excessive_blank_lines, {
            input: "line1\n\n\n\n\nline2",
            expected: "line1\n\nline2\n",
        }),
        (preserve_single_blank_line, {
            input: "line1\n\nline2",
            expected: "line1\n\nline2\n",
        }),
        (remove_trailing_blank_lines, {
            input: "content\n\n\n",
            expected: "content\n",
        }),
    ]);

    format_unchanged!(already_formatted, "workflow:\nstep1:\naction: test\n");

    format_unchanged!(empty_content, "");
    format_unchanged!(single_line, "single line content\n");

    #[test]
    fn test_custom_config() {
        let config = FormatterConfig {
            indent_size: 2,
            max_line_length: 80,
            trim_trailing_whitespace: true,
            ensure_final_newline: true,
        };

        let formatter = Formatter::with_config(config);
        let result = formatter.format_content("\tcontent").unwrap();

        assert_eq!(result, "content\n"); // No indentation for top-level content
    }

    #[test]
    fn test_format_file_operation() {
        let temp_file = create_temp_ai_file("\tcontent with tabs   \n\n\n").unwrap();
        let formatter = Formatter::new();

        let result = formatter.format_file(temp_file.path()).unwrap();

        assert!(result.changed);
        assert_eq!(result.content, "content with tabs\n"); // No indentation for top-level content
    }

    #[test]
    fn test_format_directory() {
        let test_files = &[
            ("test1.ai", "\tcontent1   \n"),
            ("test2.ai", "content2\n"), // Already correctly formatted (no indentation for top-level)
            ("subdir/test3.ai", "\t\tcontent3\n\n\n"),
            ("not_ai.txt", "should be ignored"),
        ];

        let temp_dir = create_temp_test_dir(test_files).unwrap();
        let formatter = Formatter::new();

        let formatted_files = formatter.format_directory(temp_dir.path()).unwrap();

        // Should format 2 files (test1.ai and subdir/test3.ai)
        // test2.ai is already correctly formatted
        assert_eq!(formatted_files.len(), 2);

        // Verify the files were actually formatted
        let test1_content = std::fs::read_to_string(temp_dir.path().join("test1.ai")).unwrap();
        assert_eq!(test1_content, "content1\n"); // No indentation for top-level

        let test3_content = std::fs::read_to_string(temp_dir.path().join("subdir/test3.ai")).unwrap();
        assert_eq!(test3_content, "content3\n"); // No indentation for top-level
    }

    #[test]
    fn test_check_directory() {
        let test_files = &[
            ("formatted.ai", "content\n"), // Correctly formatted (no indentation for top-level)
            ("unformatted.ai", "\tcontent   \n"),
        ];

        let temp_dir = create_temp_test_dir(test_files).unwrap();
        let formatter = Formatter::new();

        let unformatted_files = formatter.check_directory(temp_dir.path()).unwrap();

        assert_eq!(unformatted_files.len(), 1);
        assert!(unformatted_files[0].file_name().unwrap() == "unformatted.ai");
    }

    #[test]
    fn test_invalid_extension() {
        let temp_file = tempfile::Builder::new()
            .suffix(".txt")
            .tempfile()
            .unwrap();

        let formatter = Formatter::new();
        let result = formatter.format_file(temp_file.path());

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(),
            engine_ai_core::formatter::FormatterError::InvalidExtension { .. }));
    }

    #[test]
    fn test_directory_not_found() {
        let formatter = Formatter::new();
        let result = formatter.format_directory(Path::new("/nonexistent/directory"));

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(),
            engine_ai_core::formatter::FormatterError::DirectoryNotFound { .. }));
    }

    #[test]
    fn test_no_ai_files_found() {
        let temp_dir = tempfile::tempdir().unwrap();
        std::fs::write(temp_dir.path().join("test.txt"), "not an ai file").unwrap();

        let formatter = Formatter::new();
        let result = formatter.format_directory(temp_dir.path());

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(),
            engine_ai_core::formatter::FormatterError::NoFilesFound { .. }));
    }
}