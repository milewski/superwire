use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn build_command_succeeds_for_sample_workflow() {
    let _build_lock = build_test_lock().lock().expect("build test lock should be available");

    let workflow_source = r"
input {
    topic: string
}

output {
    message: input.topic
}
";
    let temporary_workflow_file = TemporaryWorkflowFile::new("build-succeeds.ai", workflow_source);
    let output_binary_path = temporary_workflow_file
        .directory_path()
        .join(format!("built-workflow{}", std::env::consts::EXE_SUFFIX));
    let command_output = run_build_command(temporary_workflow_file.path(), &output_binary_path);
    let stderr = String::from_utf8_lossy(&command_output.stderr);

    assert!(command_output.status.success(), "unexpected stderr: {stderr}");
    assert!(output_binary_path.exists());
}

#[test]
fn generated_binary_help_includes_expected_input_flags() {
    let _build_lock = build_test_lock().lock().expect("build test lock should be available");

    let workflow_source = r"
input {
    topic: string
}

output {
    message: input.topic
}
";
    let temporary_workflow_file = TemporaryWorkflowFile::new("build-help.ai", workflow_source);
    let output_binary_path = temporary_workflow_file
        .directory_path()
        .join(format!("built-help{}", std::env::consts::EXE_SUFFIX));
    let build_output = run_build_command(temporary_workflow_file.path(), &output_binary_path);
    let build_stderr = String::from_utf8_lossy(&build_output.stderr);

    assert!(build_output.status.success(), "unexpected build stderr: {build_stderr}");

    let help_output = Command::new(&output_binary_path)
        .arg("--help")
        .output()
        .expect("generated executable should run for --help");
    let help_stdout = String::from_utf8_lossy(&help_output.stdout);

    assert!(help_output.status.success());
    assert!(help_stdout.contains("--topic"));
}

#[test]
fn generated_binary_executes_and_outputs_expected_json() {
    let _build_lock = build_test_lock().lock().expect("build test lock should be available");

    let workflow_source = r"
input {
    topic: string
}

output {
    message: input.topic
}
";
    let temporary_workflow_file = TemporaryWorkflowFile::new("build-run.ai", workflow_source);
    let output_binary_path = temporary_workflow_file
        .directory_path()
        .join(format!("built-run{}", std::env::consts::EXE_SUFFIX));
    let build_output = run_build_command(temporary_workflow_file.path(), &output_binary_path);
    let build_stderr = String::from_utf8_lossy(&build_output.stderr);

    assert!(build_output.status.success(), "unexpected build stderr: {build_stderr}");

    let execution_output = Command::new(&output_binary_path)
        .arg("--topic")
        .arg("hello-from-built-binary")
        .output()
        .expect("generated executable should run");
    let execution_stdout = String::from_utf8_lossy(&execution_output.stdout);

    assert!(execution_output.status.success());
    assert!(execution_stdout.contains("\"message\": \"hello-from-built-binary\""));
}

fn run_build_command(workflow_path: &Path, output_path: &Path) -> Output {
    Command::new(cli_binary_path())
        .arg("build")
        .arg(workflow_path)
        .arg("--output")
        .arg(output_path)
        .output()
        .expect("build command should run")
}

fn cli_binary_path() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_engine-ai"))
}

struct TemporaryWorkflowFile {
    directory_path: PathBuf,
    file_path: PathBuf,
}

impl TemporaryWorkflowFile {
    fn new(file_name: impl AsRef<Path>, file_contents: &str) -> Self {
        let unique_suffix = unique_suffix();
        let directory_path = std::env::temp_dir().join(format!("engine-ai-cli-build-{unique_suffix}"));
        let file_path = directory_path.join(file_name);

        fs::create_dir_all(&directory_path).expect("temporary directory should be created");
        fs::write(&file_path, file_contents).expect("temporary workflow file should be written");

        Self { directory_path, file_path }
    }

    fn path(&self) -> &Path {
        &self.file_path
    }

    fn directory_path(&self) -> &Path {
        &self.directory_path
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

fn build_test_lock() -> &'static Mutex<()> {
    static BUILD_TEST_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

    BUILD_TEST_LOCK.get_or_init(|| Mutex::new(()))
}
