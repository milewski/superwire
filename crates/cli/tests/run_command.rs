use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn run_command_succeeds_for_workflow_without_input() {
    let workflow_source = r#"
output {
    message: "hello"
}
"#;
    let temporary_workflow_file = TemporaryWorkflowFile::new("run-no-input.ai", workflow_source);

    let command_output = run_workflow_command(temporary_workflow_file.path(), &[]);
    let stdout = String::from_utf8_lossy(&command_output.stdout);
    let stderr = String::from_utf8_lossy(&command_output.stderr);

    assert!(command_output.status.success());
    assert!(stdout.contains("\"message\": \"hello\""));
    assert!(stderr.is_empty(), "unexpected stderr: {stderr}");
}

#[test]
fn run_command_succeeds_for_typed_input_flag_binding() {
    let workflow_source = r"
input {
    topic: string
}

output {
    message: input.topic
}
";
    let temporary_workflow_file = TemporaryWorkflowFile::new("run-input.ai", workflow_source);
    let invocation_arguments = ["--topic", "hello-from-flag"];
    let command_output = run_workflow_command(temporary_workflow_file.path(), &invocation_arguments);
    let stdout = String::from_utf8_lossy(&command_output.stdout);

    assert!(command_output.status.success());
    assert!(stdout.contains("\"message\": \"hello-from-flag\""));
}

#[test]
fn run_command_fails_with_runtime_exit_when_tools_are_used() {
    let workflow_source = r#"
provider ollama {
    driver: "ollama"
    endpoint: "http://127.0.0.1:11434"
    models: ["test-model"]
}

agent planner {
    model: ollama("test-model")
    prompt: "hello"
    tools: ["search"]
    output: string
}

output {
    message: agent.planner
}
"#;
    let temporary_workflow_file = TemporaryWorkflowFile::new("run-tools.ai", workflow_source);
    let command_output = run_workflow_command(temporary_workflow_file.path(), &[]);
    let stderr = String::from_utf8_lossy(&command_output.stderr);

    assert!(!command_output.status.success());
    assert_eq!(command_output.status.code(), Some(3));
    assert!(stderr.contains("runtime error:"));
    assert!(stderr.contains("uses `tools`, which is not supported yet"));
}

fn run_workflow_command(workflow_path: &Path, invocation_arguments: &[&str]) -> Output {
    let mut command = Command::new(cli_binary_path());
    command.arg("run").arg(workflow_path);

    for invocation_argument in invocation_arguments {
        command.arg(invocation_argument);
    }

    command.output().expect("run command should run")
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
        let directory_path = std::env::temp_dir().join(format!("engine-ai-cli-run-{unique_suffix}"));
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
