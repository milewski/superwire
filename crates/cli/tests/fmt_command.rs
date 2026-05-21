mod harness;

use std::fs;
use std::path::{Path, PathBuf};

use harness::{CliCommand, CommandOutputAssertions, TemporaryWorkspace};

#[test]
fn formats_all_markdown_fixtures_as_individual_files() {
    for fixture_case in discover_formatter_fixture_cases() {
        let temporary_workspace = TemporaryWorkspace::new("superwire-cli-fmt-tests");
        let workflow_file_path = temporary_workspace.write_file("single.wire", &fixture_case.before_source);

        let command_output = CliCommand::format_command(workflow_file_path.as_path()).output();

        command_output.assert_success(&format!("fmt command should succeed for fixture {}", fixture_case.fixture_name));

        let formatted_source = fs::read_to_string(&workflow_file_path).expect("formatted workflow source should be readable after fmt");

        assert_eq!(
            formatted_source, fixture_case.expected_after_source,
            "formatted output mismatch for fixture {}",
            fixture_case.fixture_name
        );
    }
}

#[test]
fn formats_all_workflow_files_inside_directory_from_markdown_fixtures() {
    let temporary_workspace = TemporaryWorkspace::new("superwire-cli-fmt-tests");
    let source_directory = temporary_workspace.create_directory("workflows");
    let nested_directory = temporary_workspace.create_directory("workflows/nested");

    let mut created_workflow_cases = Vec::new();

    for (fixture_index, fixture_case) in discover_formatter_fixture_cases().into_iter().enumerate() {
        let target_directory = if fixture_index % 2 == 0 {
            &source_directory
        } else {
            &nested_directory
        };

        let workflow_file_name = format!("{}.wire", fixture_case.fixture_name);
        let workflow_file_path = target_directory.join(workflow_file_name);

        fs::write(&workflow_file_path, &fixture_case.before_source).expect("workflow source should be written");

        created_workflow_cases.push((workflow_file_path, fixture_case.expected_after_source));
    }

    let command_output = CliCommand::format_command(source_directory.as_path()).output();

    command_output.assert_success("fmt command should succeed");

    for (workflow_file_path, expected_after_source) in created_workflow_cases {
        let formatted_source = fs::read_to_string(&workflow_file_path).expect("formatted workflow source should be readable after fmt");

        assert_eq!(formatted_source, expected_after_source);
    }
}

#[test]
fn preserves_comments_for_fixture_cases_that_contain_comments() {
    for fixture_case in discover_formatter_fixture_cases() {
        if !fixture_case.before_source.contains("//") {
            continue;
        }

        let temporary_workspace = TemporaryWorkspace::new("superwire-cli-fmt-tests");
        let workflow_file_path = temporary_workspace.write_file("comments.wire", &fixture_case.before_source);

        let command_output = CliCommand::format_command(workflow_file_path.as_path()).output();

        command_output.assert_success(&format!("fmt command should succeed for fixture {}", fixture_case.fixture_name));

        let formatted_source = fs::read_to_string(&workflow_file_path).expect("formatted workflow source should be readable after fmt");

        assert_eq!(
            formatted_source, fixture_case.expected_after_source,
            "formatted output mismatch for fixture {}",
            fixture_case.fixture_name
        );

        assert!(
            formatted_source.contains("//"),
            "expected comments to be preserved for fixture {}",
            fixture_case.fixture_name
        );
    }
}

#[test]
fn rejects_non_workflow_file_target() {
    let temporary_workspace = TemporaryWorkspace::new("superwire-cli-fmt-tests");
    let non_workflow_file_path = temporary_workspace.write_file("notes.txt", "not a workflow\n");

    let command_output = CliCommand::format_command(non_workflow_file_path.as_path()).output();
    let stderr_text = command_output.stderr_text();

    command_output.assert_failure_code(2, "fmt command should fail for non-.wire files");
    assert!(stderr_text.contains("expected a .wire workflow file"));
}

#[test]
fn rejects_directory_without_workflow_files() {
    let temporary_workspace = TemporaryWorkspace::new("superwire-cli-fmt-tests");
    let empty_directory_path = temporary_workspace.create_directory("empty");

    let command_output = CliCommand::format_command(empty_directory_path.as_path()).output();
    let stderr_text = command_output.stderr_text();

    command_output.assert_failure_code(2, "fmt command should fail when no workflow files are found");
    assert!(stderr_text.contains("no workflow files (.wire) found"));
}

fn discover_formatter_fixture_paths() -> Vec<PathBuf> {
    let formatter_fixture_directory = formatter_fixture_directory();
    let directory_entries = fs::read_dir(&formatter_fixture_directory).unwrap_or_else(|read_error| {
        panic!(
            "failed to read formatter fixture directory {}: {read_error}",
            formatter_fixture_directory.display()
        )
    });
    let mut fixture_paths = Vec::new();

    for directory_entry_result in directory_entries {
        let directory_entry =
            directory_entry_result.unwrap_or_else(|read_error| panic!("failed to read formatter fixture entry: {read_error}"));
        let fixture_path = directory_entry.path();

        if !fixture_path.is_file() {
            continue;
        }

        fixture_paths.push(fixture_path);
    }

    assert!(
        !fixture_paths.is_empty(),
        "formatter fixture directory {} should contain at least one fixture file",
        formatter_fixture_directory.display()
    );

    fixture_paths.sort();
    fixture_paths
}

fn discover_formatter_fixture_cases() -> Vec<FormatterFixtureCase> {
    discover_formatter_fixture_paths()
        .into_iter()
        .map(|fixture_path| FormatterFixtureCase::from_path(&fixture_path))
        .collect::<Vec<_>>()
}

fn formatter_fixture_directory() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/formatter")
}

struct FormatterFixtureCase {
    fixture_name: String,
    before_source: String,
    expected_after_source: String,
}

impl FormatterFixtureCase {
    fn from_path(fixture_path: &Path) -> Self {
        let fixture_contents = fs::read_to_string(fixture_path)
            .unwrap_or_else(|read_error| panic!("failed to read fixture {}: {read_error}", fixture_path.display()));
        let wire_code_blocks = Self::extract_wire_code_blocks(&fixture_contents);

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
            expected_after_source: wire_code_blocks[1].clone(),
        }
    }

    fn extract_wire_code_blocks(markdown_text: &str) -> Vec<String> {
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

        assert!(!is_inside_wire_block, "unclosed ```wire block in fixture markdown");
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
