use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

macro_rules! workflow_template {
    ($($workflow_tokens:tt)*) => {{
        stringify!($($workflow_tokens)*)
    }};
}

#[test]
fn validates_workflow_file_when_check_command_succeeds() {
    let temporary_workspace = TemporaryWorkspace::new();
    let workflow_source = workflow_template! {
        output {
            ok: true
        }
    };

    let workflow_file_path = temporary_workspace.write_file("valid.wire", &workflow_source);

    let command_output = run_workflow_check_command(workflow_file_path.as_path());
    let standard_output = String::from_utf8_lossy(&command_output.stdout);

    assert!(command_output.status.success(), "workflow check command should succeed");
    assert!(standard_output.contains("workflow is valid"));
}

#[test]
fn rejects_workflow_file_with_invalid_reference_types() {
    let temporary_workspace = TemporaryWorkspace::new();
    let workflow_source = workflow_template! {
        input {
            title: string
        }

        output {
            summary: input.missing
        }
    };

    let workflow_file_path = temporary_workspace.write_file("invalid.wire", &workflow_source);

    let command_output = run_workflow_check_command(workflow_file_path.as_path());
    let standard_error = String::from_utf8_lossy(&command_output.stderr);

    assert!(!command_output.status.success(), "workflow check command should fail");
    assert_eq!(command_output.status.code(), Some(2));
    assert!(
        standard_error.contains("missing") || standard_error.contains("unknown"),
        "expected validation error details in stderr, received: {standard_error}"
    );
}

fn run_workflow_check_command(workflow_file_path: &Path) -> Output {
    Command::new(cli_binary_path())
        .arg("workflow")
        .arg("check")
        .arg(workflow_file_path)
        .output()
        .expect("workflow check command should run")
}

fn cli_binary_path() -> PathBuf {
    if let Some(configured_binary_path) = option_env!("CARGO_BIN_EXE_superwire-cli") {
        return PathBuf::from(configured_binary_path);
    }

    if let Some(configured_binary_path) = option_env!("CARGO_BIN_EXE_superwire_cli") {
        return PathBuf::from(configured_binary_path);
    }

    let current_executable_path = std::env::current_exe()
        .unwrap_or_else(|current_executable_error| panic!("failed to resolve current test executable path: {current_executable_error}"));

    let target_profile_directory = current_executable_path.parent().and_then(Path::parent).unwrap_or_else(|| {
        panic!(
            "failed to derive target profile directory from {}",
            current_executable_path.display()
        )
    });

    let executable_file_name = format!("superwire-cli{}", std::env::consts::EXE_SUFFIX);
    let inferred_binary_path = target_profile_directory.join(executable_file_name);

    if inferred_binary_path.exists() {
        return inferred_binary_path;
    }

    panic!(
        "failed to locate superwire-cli binary; looked for compile-time cargo bin vars and {}",
        inferred_binary_path.display()
    );
}

struct TemporaryWorkspace {
    root_directory: PathBuf,
}

impl TemporaryWorkspace {
    fn new() -> Self {
        let unique_suffix = unique_suffix();
        let root_directory = std::env::temp_dir().join(format!("superwire-workflow-check-tests-{unique_suffix}"));

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
}

impl Drop for TemporaryWorkspace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root_directory);
    }
}

fn unique_suffix() -> String {
    let timestamp_millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after unix epoch")
        .as_millis();

    let process_identifier = std::process::id();

    format!("{timestamp_millis}-{process_identifier}")
}
