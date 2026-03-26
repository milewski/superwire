use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn fmt_check_fails_with_non_zero_when_formatting_is_needed() {
    let unformatted_workflow = r#"
provider openai   {driver:"openai" endpoint:"https://api.openai.com/v1"  models:["gpt-4o-mini","gpt-4o",]}

agent planner {model:openai("gpt-4o-mini") prompt:"Plan for {{input.topic}}" output:string}

output {plan:agent.planner}
"#;

    let temporary_workflow_file = TemporaryWorkflowFile::new("needs-formatting.ai", unformatted_workflow);

    let command_output = run_fmt_command(temporary_workflow_file.path(), true);
    let stderr = String::from_utf8_lossy(&command_output.stderr);

    assert!(!command_output.status.success());
    assert_eq!(command_output.status.code(), Some(2));
    assert!(stderr.contains("workflow formatting differs from canonical style"));
}

#[test]
fn fmt_rejects_comments_and_leaves_file_unchanged() {
    let workflow_with_comment = r#"
provider openai {
    driver: "openai" // this comment is rejected
}

agent planner {
    model: openai("gpt-4o-mini")
    prompt: "hello"
    output: string
}

output {
    plan: agent.planner
}
"#;

    let temporary_workflow_file = TemporaryWorkflowFile::new("commented-workflow.ai", workflow_with_comment);

    let command_output = run_fmt_command(temporary_workflow_file.path(), false);
    let stderr = String::from_utf8_lossy(&command_output.stderr);
    let file_contents_after_failure =
        fs::read_to_string(temporary_workflow_file.path()).expect("workflow file should still be readable after fmt failure");

    assert!(!command_output.status.success());
    assert_eq!(command_output.status.code(), Some(2));
    assert!(stderr.contains("line comments (`//`) are not allowed"));
    assert_eq!(file_contents_after_failure, workflow_with_comment);
}

fn run_fmt_command(workflow_path: &Path, check_mode: bool) -> Output {
    let mut command = Command::new(cli_binary_path());
    command.arg("fmt");

    if check_mode {
        command.arg("--check");
    }

    command.arg(workflow_path).output().expect("fmt command should run")
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
        let directory_path = std::env::temp_dir().join(format!("engine-ai-cli-fmt-{unique_suffix}"));
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
