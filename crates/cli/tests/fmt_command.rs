use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

use engine_ai_core::dsl::format_workflow_source;

macro_rules! inline_cli_workflow {
    ($($workflow_tokens:tt)*) => {{
        normalize_inline_workflow_source(stringify!($($workflow_tokens)*))
    }};
}

#[test]
fn formats_single_workflow_file_in_place() {
    let unformatted_source = inline_cli_workflow! {
        provider openai   {driver:"openai" models:["gpt-4o-mini",]}

        output { result: "ok" }
    };

    let expected_source = format_workflow_source(&unformatted_source).expect("formatter should build expected canonical source");

    let temporary_workspace = TemporaryWorkspace::new();
    let workflow_path = temporary_workspace.write_file("single.ai", &unformatted_source);

    let command_output = run_fmt_command(workflow_path.as_path());

    assert!(command_output.status.success(), "fmt command should succeed");

    let formatted_source = fs::read_to_string(&workflow_path).expect("formatted workflow should be readable");

    assert_eq!(formatted_source, expected_source);
}

#[test]
fn formats_all_workflow_files_inside_directory() {
    let temporary_workspace = TemporaryWorkspace::new();

    let source_directory = temporary_workspace.create_directory("workflows");
    let nested_directory = temporary_workspace.create_directory("workflows/nested");

    let first_source = inline_cli_workflow! {
        provider openai   {driver:"openai" models:["gpt-4o-mini",]}

        output { greeting: "hello" }
    };

    let second_source = inline_cli_workflow! {
        output { value: 1 }
    };

    let expected_first_source = format_workflow_source(&first_source).expect("first formatter output should be canonical");
    let expected_second_source = format_workflow_source(&second_source).expect("second formatter output should be canonical");

    let first_workflow_path = source_directory.join("first.ai");
    let second_workflow_path = nested_directory.join("second.ai");

    fs::write(&first_workflow_path, first_source).expect("first workflow should be created");
    fs::write(&second_workflow_path, second_source).expect("second workflow should be created");

    let command_output = run_fmt_command(source_directory.as_path());

    assert!(command_output.status.success(), "fmt command should succeed");

    let formatted_first_source = fs::read_to_string(&first_workflow_path).expect("first formatted workflow should be readable");
    let formatted_second_source = fs::read_to_string(&second_workflow_path).expect("second formatted workflow should be readable");

    assert_eq!(formatted_first_source, expected_first_source);
    assert_eq!(formatted_second_source, expected_second_source);
}

#[test]
fn preserves_comments_while_formatting() {
    let source_with_comments = include_str!("fixtures/comments_workflow.ai");
    let expected_source =
        format_workflow_source(source_with_comments).expect("formatter should preserve comments when building expected source");

    let temporary_workspace = TemporaryWorkspace::new();
    let workflow_path = temporary_workspace.write_file("comments.ai", source_with_comments);

    let command_output = run_fmt_command(workflow_path.as_path());

    assert!(command_output.status.success(), "fmt command should succeed");

    let formatted_source = fs::read_to_string(&workflow_path).expect("formatted workflow should be readable");

    assert_eq!(formatted_source, expected_source);
    assert!(formatted_source.contains("// provider declaration"));
    assert!(formatted_source.contains("// provider driver"));
    assert!(formatted_source.contains("// inline comment"));
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

fn normalize_inline_workflow_source(source_template: &str) -> String {
    let mut normalized_source = source_template.to_owned();

    if !normalized_source.ends_with('\n') {
        normalized_source.push('\n');
    }

    normalized_source
}
