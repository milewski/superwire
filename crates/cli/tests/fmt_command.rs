use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn formats_single_workflow_file_from_markdown_fixture() {
    let fixture_case = FormatterFixtureCase::from_path(&formatter_fixture_path("basic_spacing.md"));

    let temporary_workspace = TemporaryWorkspace::new();
    let workflow_path = temporary_workspace.write_file("single.ai", &fixture_case.before_source);

    let command_output = run_fmt_command(workflow_path.as_path());

    assert!(command_output.status.success(), "fmt command should succeed");

    let formatted_source = fs::read_to_string(&workflow_path).expect("formatted workflow should be readable");

    assert_eq!(formatted_source, fixture_case.expected_after_source);
}

#[test]
fn formats_all_workflow_files_inside_directory_from_markdown_fixtures() {
    let temporary_workspace = TemporaryWorkspace::new();
    let source_directory = temporary_workspace.create_directory("workflows");
    let nested_directory = temporary_workspace.create_directory("workflows/nested");

    let mut created_workflow_cases = Vec::new();

    for (fixture_index, fixture_path) in discover_formatter_fixture_paths().into_iter().enumerate() {
        let fixture_case = FormatterFixtureCase::from_path(&fixture_path);
        let target_directory = if fixture_index % 2 == 0 {
            &source_directory
        } else {
            &nested_directory
        };

        let workflow_file_name = format!("{}.ai", fixture_case.fixture_name);
        let workflow_file_path = target_directory.join(workflow_file_name);

        fs::write(&workflow_file_path, &fixture_case.before_source).expect("workflow source should be written");

        created_workflow_cases.push((workflow_file_path, fixture_case.expected_after_source));
    }

    let command_output = run_fmt_command(source_directory.as_path());

    assert!(command_output.status.success(), "fmt command should succeed");

    for (workflow_file_path, expected_after_source) in created_workflow_cases {
        let formatted_source = fs::read_to_string(&workflow_file_path).expect("formatted workflow source should be readable after fmt");

        assert_eq!(formatted_source, expected_after_source);
    }
}

#[test]
fn preserves_comments_while_formatting() {
    let fixture_case = FormatterFixtureCase::from_path(&formatter_fixture_path("comments_preserved.md"));

    let temporary_workspace = TemporaryWorkspace::new();
    let workflow_path = temporary_workspace.write_file("comments.ai", &fixture_case.before_source);

    let command_output = run_fmt_command(workflow_path.as_path());

    assert!(command_output.status.success(), "fmt command should succeed");

    let formatted_source = fs::read_to_string(&workflow_path).expect("formatted workflow should be readable");

    assert_eq!(formatted_source, fixture_case.expected_after_source);
    assert!(formatted_source.contains("// provider declaration"));
    assert!(formatted_source.contains("// provider driver"));
    assert!(formatted_source.contains("// inline driver comment"));
}

#[test]
fn rejects_non_workflow_file_target() {
    let temporary_workspace = TemporaryWorkspace::new();
    let non_workflow_file_path = temporary_workspace.write_file("notes.txt", "not a workflow\n");

    let command_output = run_fmt_command(non_workflow_file_path.as_path());
    let stderr_text = String::from_utf8_lossy(&command_output.stderr);

    assert!(!command_output.status.success(), "fmt command should fail for non-.ai files");
    assert_eq!(command_output.status.code(), Some(2));
    assert!(stderr_text.contains("expected a .ai workflow file"));
}

#[test]
fn rejects_directory_without_workflow_files() {
    let temporary_workspace = TemporaryWorkspace::new();
    let empty_directory_path = temporary_workspace.create_directory("empty");

    let command_output = run_fmt_command(empty_directory_path.as_path());
    let stderr_text = String::from_utf8_lossy(&command_output.stderr);

    assert!(
        !command_output.status.success(),
        "fmt command should fail when no workflow files are found"
    );

    assert_eq!(command_output.status.code(), Some(2));
    assert!(stderr_text.contains("no workflow files (.ai) found"));
}

fn run_fmt_command(target_path: &Path) -> Output {
    Command::new(cli_binary_path())
        .arg("fmt")
        .arg(target_path)
        .output()
        .expect("fmt command should run")
}

fn cli_binary_path() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_cli"))
}

fn formatter_fixture_path(fixture_file_name: &str) -> PathBuf {
    formatter_fixture_directory().join(fixture_file_name)
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

        if fixture_path.extension().and_then(|extension| extension.to_str()) != Some("md") {
            continue;
        }

        fixture_paths.push(fixture_path);
    }

    fixture_paths.sort();
    fixture_paths
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
        let ai_code_blocks = Self::extract_ai_code_blocks(&fixture_contents);

        assert_eq!(
            ai_code_blocks.len(),
            2,
            "fixture {} must contain exactly two ```ai blocks",
            fixture_path.display()
        );

        Self {
            fixture_name: fixture_path
                .file_stem()
                .and_then(|file_stem| file_stem.to_str())
                .expect("fixture file name should have valid UTF-8 stem")
                .to_owned(),
            before_source: ai_code_blocks[0].clone(),
            expected_after_source: ai_code_blocks[1].clone(),
        }
    }

    fn extract_ai_code_blocks(markdown_text: &str) -> Vec<String> {
        let mut extracted_blocks = Vec::new();
        let mut current_block_lines = Vec::new();
        let mut is_inside_ai_block = false;

        for markdown_line in markdown_text.lines() {
            let line_without_carriage_return = markdown_line.trim_end_matches('\r');
            let trimmed_line = line_without_carriage_return.trim();

            if !is_inside_ai_block && trimmed_line == "```ai" {
                is_inside_ai_block = true;
                current_block_lines.clear();

                continue;
            }

            if is_inside_ai_block && trimmed_line == "```" {
                extracted_blocks.push(Self::normalize_block_contents(&current_block_lines));
                is_inside_ai_block = false;

                continue;
            }

            if is_inside_ai_block {
                current_block_lines.push(line_without_carriage_return.to_owned());
            }
        }

        assert!(!is_inside_ai_block, "unclosed ```ai block in fixture markdown");
        extracted_blocks
    }

    fn normalize_block_contents(block_lines: &[String]) -> String {
        let mut normalized_contents = block_lines.join("\n");

        if !normalized_contents.ends_with('\n') {
            normalized_contents.push('\n');
        }

        normalized_contents
    }
}

struct TemporaryWorkspace {
    root_directory: PathBuf,
}

impl TemporaryWorkspace {
    fn new() -> Self {
        let unique_suffix = unique_suffix();
        let root_directory = std::env::temp_dir().join(format!("engine-ai-cli-tests-{unique_suffix}"));

        fs::create_dir_all(&root_directory).expect("temporary root directory should be created");

        Self { root_directory }
    }

    fn write_file(&self, relative_path: &str, contents: &str) -> PathBuf {
        let absolute_path = self.root_directory.join(relative_path);

        if let Some(parent_directory) = absolute_path.parent() {
            fs::create_dir_all(parent_directory).expect("parent directory should be created");
        }

        fs::write(&absolute_path, contents).expect("temporary file should be written");

        absolute_path
    }

    fn create_directory(&self, relative_path: &str) -> PathBuf {
        let absolute_path = self.root_directory.join(relative_path);
        fs::create_dir_all(&absolute_path).expect("directory should be created");
        absolute_path
    }
}

impl Drop for TemporaryWorkspace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root_directory);
    }
}

fn unique_suffix() -> String {
    let unix_timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after unix epoch")
        .as_nanos();

    format!("{}-{unix_timestamp}", std::process::id())
}
