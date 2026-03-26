use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn check_command_succeeds_for_valid_workflow() {
    let minimum_workflow_path = core_workflow_sample_path("minimum.ai");
    let minimum_workflow_source = fs::read_to_string(&minimum_workflow_path).expect("minimum sample workflow should be readable");
    let valid_workflow_source = minimum_workflow_source.replace(
        "driver: \"ollama\"\n    models:",
        "driver: \"ollama\"\n    endpoint: \"http://127.0.0.1:11434\"\n    models:",
    );
    let temporary_workflow_file = TemporaryWorkflowFile::new("valid-workflow.ai", &valid_workflow_source);

    let command_output = run_check_command(temporary_workflow_file.path());
    let stdout = String::from_utf8_lossy(&command_output.stdout);
    let stderr = String::from_utf8_lossy(&command_output.stderr);

    assert!(command_output.status.success());
    assert!(stdout.contains("workflow is valid:"));
    assert!(stderr.is_empty(), "unexpected stderr: {stderr}");
}

#[test]
fn check_command_fails_with_non_zero_exit_and_useful_error() {
    let minimum_workflow_path = core_workflow_sample_path("minimum.ai");
    let command_output = run_check_command(&minimum_workflow_path);
    let stderr = String::from_utf8_lossy(&command_output.stderr);

    assert!(!command_output.status.success());
    assert_eq!(command_output.status.code(), Some(2));
    assert!(stderr.contains("workflow is invalid:"));
    assert!(stderr.contains("missing `endpoint` property"));
}

fn run_check_command(workflow_path: &Path) -> Output {
    Command::new(cli_binary_path())
        .arg("check")
        .arg(workflow_path)
        .output()
        .expect("check command should run")
}

fn cli_binary_path() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_engine-ai"))
}

fn core_workflow_sample_path(file_name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../core/workflows").join(file_name)
}

struct TemporaryWorkflowFile {
    directory_path: PathBuf,
    file_path: PathBuf,
}

impl TemporaryWorkflowFile {
    fn new(file_name: impl AsRef<Path>, file_contents: &str) -> Self {
        let unique_suffix = unique_suffix();
        let directory_path = std::env::temp_dir().join(format!("engine-ai-cli-check-{unique_suffix}"));
        let file_path = directory_path.join(file_name);

        fs::create_dir_all(&directory_path).expect("temporary directory should be created");
        fs::write(&file_path, file_contents).expect("temporary workflow file should be written");

        Self { directory_path, file_path }
    }

    fn path(&self) -> &Path {
        &self.file_path
    }
}

impl Drop for TemporaryWorkflowFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.file_path);
        let _ = fs::remove_dir(&self.directory_path);
    }
}

fn unique_suffix() -> String {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after unix epoch")
        .as_nanos();

    format!("{}-{timestamp}", std::process::id())
}
