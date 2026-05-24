use thiserror::Error;

use super::parse_workflow;
use super::parser::DslParseError;

mod comments;
mod declarations;
mod expressions;
mod mcp;
mod tools;
mod types;
mod wrapping;

use comments::CommentPreserver;

#[derive(Debug, Error)]
pub enum DslFormatError {
    #[error("failed to parse DSL while formatting: {0}")]
    Parse(#[from] DslParseError),
}

pub fn format_workflow_source(source_text: &str) -> Result<String, DslFormatError> {
    let workflow = parse_workflow(source_text)?;
    let mut formatter = DslFormatter::new();
    formatter.push_workflow(&workflow);

    let formatted_without_comments = formatter.finish();

    Ok(CommentPreserver::new(source_text, formatted_without_comments).with_preserved_comments())
}

struct DslFormatter {
    output: String,
    indentation_depth: usize,
}

impl DslFormatter {
    fn new() -> Self {
        Self {
            output: String::new(),
            indentation_depth: 0,
        }
    }

    fn finish(mut self) -> String {
        if !self.output.ends_with('\n') {
            self.output.push('\n');
        }

        self.output
    }
}

#[cfg(test)]
mod tests {
    use super::format_workflow_source;
    use crate::dsl::parse_workflow;
    use crate::testing::SnapshotAssertion;
    use crate::workflow_source;
    use std::fs;
    use std::path::{Path, PathBuf};

    #[test]
    fn formatter_is_idempotent_for_all_workflow_examples() {
        for workflow_path in discover_workflow_examples() {
            let workflow_source = fs::read_to_string(&workflow_path)
                .unwrap_or_else(|read_error| panic!("failed to read {}: {read_error}", workflow_path.display()));

            let first_formatted_output = format_workflow_source(&workflow_source)
                .unwrap_or_else(|format_error| panic!("failed to format {}: {format_error}", workflow_path.display()));

            let second_formatted_output = format_workflow_source(&first_formatted_output)
                .unwrap_or_else(|format_error| panic!("failed to re-format {}: {format_error}", workflow_path.display()));

            assert_eq!(
                first_formatted_output,
                second_formatted_output,
                "formatter output should be stable for {}",
                workflow_path.display()
            );
        }
    }

    #[test]
    fn formatter_fixtures_are_parseable_and_idempotent() {
        for fixture_case in FormatterFixtureCase::discover_all() {
            fixture_case.assert_original_parses();

            let formatted_source = fixture_case.format_original();

            fixture_case.assert_formatted_parses(&formatted_source);

            let reformatted_source = fixture_case.format_source(&formatted_source, "formatted");

            SnapshotAssertion::new(
                format!("formatter fixture {} idempotence", fixture_case.fixture_name),
                formatted_source,
                reformatted_source,
            )
            .assert_matches();
        }
    }

    #[test]
    fn formatter_matches_expected_output_for_representative_source() {
        let source_text =
            "provider openai from openai{}\nmodel openai_model from openai{id:\"gpt-4o-mini\"}\n\noutput { result: \"ok\" }\n";

        let expected_output =
            "provider openai from openai {\n}\n\nmodel openai_model from openai {\n    id: \"gpt-4o-mini\"\n}\n\noutput {\n    result: \"ok\"\n}\n";

        let formatted_source = format_workflow_source(source_text).expect("representative workflow should format successfully");

        assert_eq!(formatted_source, expected_output);
    }

    #[test]
    fn formatter_places_standalone_comment_before_next_declaration_when_source_is_single_line_block() {
        let source_text =
            "// provider declaration\nprovider openai from openai {\n// provider driver\n}\n\n// output heading\noutput { value: \"ok\" }\n";

        let expected_output =
            "// provider declaration\nprovider openai from openai {\n// provider driver\n}\n\n// output heading\noutput {\n    value: \"ok\"\n}\n";

        let formatted_source = format_workflow_source(source_text).expect("workflow with standalone comment should format successfully");

        assert_eq!(formatted_source, expected_output);
    }

    #[test]
    fn formatter_renders_object_destructuring_for_loop_pattern() {
        let source_text = "agent analyzer for {id,name,} in agent.alpha.participants {instruction:\"hello\" output{value:string}}\n";
        let expected_output =
            "agent analyzer for { id, name } in agent.alpha.participants {\n    instruction: \"hello\"\n    output {\n        value: string\n    }\n}\n";

        let formatted_source = format_workflow_source(source_text).expect("workflow should format successfully");

        assert_eq!(formatted_source, expected_output);
    }

    #[test]
    fn formatter_renders_mcp_headers_as_block_property() {
        let source_text = workflow_source! {
            mcp local {
                endpoint: secrets.mcp_summarizer_endpoint
                headers {
                    Accept: "application/json"
                    Authorization: "Bearer {{ secrets.mcp_token }}"
                }
            }
        };
        let expected_output = "mcp local {\n    endpoint: secrets.mcp_summarizer_endpoint\n    headers {\n        Accept: \"application/json\"\n        Authorization: \"Bearer {{ secrets.mcp_token }}\"\n    }\n}\n";

        let formatted_source = format_workflow_source(source_text).expect("MCP headers workflow should format successfully");

        assert_eq!(formatted_source, expected_output);
    }

    #[test]
    fn formatter_renders_mcp_tool_batch_imports() {
        let source_text =
            "from mcp.local.tool{bindings{project_id:1 task_id:2}tool create_sorting_task{bindings{title:\"Sort\"}}tool assign_task}\n";
        let expected_output = "from mcp.local.tool {\n    bindings {\n        project_id: 1\n        task_id: 2\n    }\n\n    tool create_sorting_task {\n        bindings {\n            title: \"Sort\"\n        }\n    }\n    tool assign_task\n}\n";

        let formatted_source = format_workflow_source(source_text).expect("batch import workflow should format successfully");

        assert_eq!(formatted_source, expected_output);
    }

    #[test]
    fn formatter_preserves_blockless_tool_calls() {
        let source_text = workflow_source! {
            dynamic {
                data: call tool.list_all_participants_who_has_answered_given_task
            }
        };
        let expected_output = "dynamic {\n    data: call tool.list_all_participants_who_has_answered_given_task\n}\n";

        let formatted_source = format_workflow_source(source_text).expect("blockless tool call workflow should format successfully");

        assert_eq!(formatted_source, expected_output);
    }

    #[test]
    fn formatter_keeps_tool_call_block_when_content_exists() {
        let source_text = workflow_source! {
            dynamic {
                data: call tool.fetch_participant_answer {
                    input {
                        participant_id: input.participant_id
                    }
                }
            }
        };
        let expected_output = "dynamic {\n    data: call tool.fetch_participant_answer {\n        input {\n            participant_id: input.participant_id\n        }\n    }\n}\n";

        let formatted_source = format_workflow_source(source_text).expect("tool call workflow should format successfully");

        assert_eq!(formatted_source, expected_output);
    }

    fn discover_workflow_examples() -> Vec<PathBuf> {
        let workflows_directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("workflows");
        let mut workflow_paths = Vec::new();

        collect_paths_by_extension(&workflows_directory, "ai", &mut workflow_paths);
        workflow_paths.sort();

        workflow_paths
    }

    struct FormatterFixtureCase {
        fixture_name: String,
        before_source: String,
    }

    impl FormatterFixtureCase {
        fn discover_all() -> Vec<Self> {
            let formatter_fixture_directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("../superwire-cli/tests/fixtures/formatter");
            let directory_entries = fs::read_dir(&formatter_fixture_directory).unwrap_or_else(|read_error| {
                panic!(
                    "failed to read formatter fixture directory {}: {read_error}",
                    formatter_fixture_directory.display()
                )
            });

            let mut fixture_cases = Vec::new();

            for directory_entry_result in directory_entries {
                let directory_entry = directory_entry_result.unwrap_or_else(|read_error| {
                    panic!(
                        "failed to read formatter fixture entry in {}: {read_error}",
                        formatter_fixture_directory.display()
                    )
                });
                let fixture_path = directory_entry.path();

                if !fixture_path.is_file() {
                    continue;
                }

                fixture_cases.push(Self::from_path(&fixture_path));
            }

            assert!(
                !fixture_cases.is_empty(),
                "formatter fixture directory {} should contain at least one fixture file",
                formatter_fixture_directory.display()
            );

            fixture_cases.sort_by(|left_case, right_case| left_case.fixture_name.cmp(&right_case.fixture_name));
            fixture_cases
        }

        fn from_path(fixture_path: &Path) -> Self {
            let fixture_contents = fs::read_to_string(fixture_path)
                .unwrap_or_else(|read_error| panic!("failed to read fixture {}: {read_error}", fixture_path.display()));

            let wire_code_blocks = Self::extract_wire_code_blocks(&fixture_contents, fixture_path);

            assert_eq!(
                wire_code_blocks.len(),
                2,
                "fixture {} must contain exactly two ```wire blocks",
                fixture_path.display()
            );

            Self {
                fixture_name: fixture_path
                    .file_stem()
                    .and_then(|file_stem| file_stem.to_str())
                    .expect("fixture file name should have valid UTF-8 stem")
                    .to_owned(),
                before_source: wire_code_blocks[0].clone(),
            }
        }

        fn assert_original_parses(&self) {
            parse_workflow(&self.before_source)
                .unwrap_or_else(|parse_error| panic!("failed to parse original fixture {}: {parse_error}", self.fixture_name));
        }

        fn format_original(&self) -> String {
            self.format_source(&self.before_source, "original")
        }

        fn assert_formatted_parses(&self, formatted_source: &str) {
            parse_workflow(formatted_source)
                .unwrap_or_else(|parse_error| panic!("failed to parse formatted fixture {}: {parse_error}", self.fixture_name));
        }

        fn format_source(&self, source_text: &str, source_label: &str) -> String {
            format_workflow_source(source_text).unwrap_or_else(|format_error| {
                panic!(
                    "failed to format {source_label} source for fixture {}: {format_error}",
                    self.fixture_name
                )
            })
        }

        fn extract_wire_code_blocks(markdown_text: &str, fixture_path: &Path) -> Vec<String> {
            let mut extracted_blocks = Vec::new();
            let mut current_block_lines = Vec::new();
            let mut is_inside_wire_block = false;

            for markdown_line in markdown_text.lines() {
                let line_without_carriage_return = markdown_line.trim_end_matches('\r');
                let trimmed_line = line_without_carriage_return.trim();

                if !is_inside_wire_block && trimmed_line == "```wire" {
                    is_inside_wire_block = true;
                    current_block_lines.clear();

                    continue;
                }

                if is_inside_wire_block && trimmed_line == "```" {
                    extracted_blocks.push(Self::normalize_block_contents(&current_block_lines));
                    is_inside_wire_block = false;

                    continue;
                }

                if is_inside_wire_block {
                    current_block_lines.push(line_without_carriage_return.to_owned());
                }
            }

            assert!(
                !is_inside_wire_block,
                "unclosed ```wire block in fixture {}",
                fixture_path.display()
            );
            extracted_blocks
        }

        fn normalize_block_contents(block_lines: &[String]) -> String {
            let normalized_lines = block_lines
                .iter()
                .map(|line_text| line_text.trim_end().to_owned())
                .collect::<Vec<_>>();

            let mut normalized_contents = normalized_lines.join("\n");

            if !normalized_contents.ends_with('\n') {
                normalized_contents.push('\n');
            }

            normalized_contents
        }
    }

    fn collect_paths_by_extension(current_directory: &Path, extension: &str, collected_paths: &mut Vec<PathBuf>) {
        let directory_entries = fs::read_dir(current_directory)
            .unwrap_or_else(|read_error| panic!("failed to read directory {}: {read_error}", current_directory.display()));

        for directory_entry_result in directory_entries {
            let directory_entry = directory_entry_result
                .unwrap_or_else(|read_error| panic!("failed to read entry in {}: {read_error}", current_directory.display()));

            let entry_path = directory_entry.path();

            if entry_path.is_dir() {
                collect_paths_by_extension(&entry_path, extension, collected_paths);

                continue;
            }

            if entry_path.extension().and_then(|path_extension| path_extension.to_str()) != Some(extension) {
                continue;
            }

            collected_paths.push(entry_path);
        }
    }
}
