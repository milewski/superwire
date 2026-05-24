#![allow(dead_code)]

use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;
use superwire_dsl::testing::WorkflowSourceTemplate;

pub struct CliCommand {
    arguments: Vec<String>,
    current_directory: Option<PathBuf>,
}

impl CliCommand {
    pub fn new(arguments: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            arguments: arguments.into_iter().map(Into::into).collect(),
            current_directory: None,
        }
    }

    pub fn workflow_check(workflow_file_path: impl AsRef<Path>) -> Self {
        Self::new([
            "workflow".to_owned(),
            "check".to_owned(),
            workflow_file_path.as_ref().to_string_lossy().into_owned(),
        ])
    }

    pub fn workflow_lock(arguments: impl IntoIterator<Item = impl AsRef<OsStr>>) -> Self {
        Self::workflow_command("lock", arguments)
    }

    pub fn workflow_vars(arguments: impl IntoIterator<Item = impl AsRef<OsStr>>) -> Self {
        Self::workflow_command("vars", arguments)
    }

    pub fn format_command(target_path: impl AsRef<Path>) -> Self {
        Self::new(["fmt".to_owned(), target_path.as_ref().to_string_lossy().into_owned()])
    }

    #[must_use]
    pub fn current_directory(mut self, current_directory: impl AsRef<Path>) -> Self {
        self.current_directory = Some(current_directory.as_ref().to_path_buf());
        self
    }

    #[must_use]
    pub fn output(&self) -> Output {
        let mut command = Command::new(Self::binary_path());
        command.args(&self.arguments);

        if let Some(current_directory) = &self.current_directory {
            command.current_dir(current_directory);
        }

        command.output().unwrap_or_else(|command_error| {
            panic!(
                "cli command `superwire-cli {}` should run: {command_error}",
                self.arguments.join(" ")
            )
        })
    }

    fn workflow_command(subcommand: &str, arguments: impl IntoIterator<Item = impl AsRef<OsStr>>) -> Self {
        let mut command_arguments = vec!["workflow".to_owned(), subcommand.to_owned()];

        command_arguments.extend(
            arguments
                .into_iter()
                .map(|argument| argument.as_ref().to_string_lossy().into_owned()),
        );

        Self::new(command_arguments)
    }

    fn binary_path() -> PathBuf {
        if let Some(configured_binary_path) = option_env!("CARGO_BIN_EXE_superwire-cli") {
            return PathBuf::from(configured_binary_path);
        }

        if let Some(configured_binary_path) = option_env!("CARGO_BIN_EXE_superwire_cli") {
            return PathBuf::from(configured_binary_path);
        }

        let current_executable_path = std::env::current_exe().unwrap_or_else(|current_executable_error| {
            panic!("failed to resolve current test executable path: {current_executable_error}")
        });

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
}

pub trait CommandOutputAssertions {
    fn assert_success(&self, message: &str);
    fn assert_failure(&self, message: &str);
    fn assert_failure_code(&self, expected_code: i32, message: &str);
    fn assert_stdout_contains(&self, expected_text: &str, message: &str);
    fn assert_stderr_contains(&self, expected_text: &str, message: &str);
    fn assert_stderr_not_contains(&self, unexpected_text: &str, message: &str);
    fn stdout_text(&self) -> String;
    fn stderr_text(&self) -> String;
}

impl CommandOutputAssertions for Output {
    fn assert_success(&self, message: &str) {
        assert!(self.status.success(), "{message}: {}", self.stderr_text());
    }

    fn assert_failure(&self, message: &str) {
        assert!(!self.status.success(), "{message}");
    }

    fn assert_failure_code(&self, expected_code: i32, message: &str) {
        assert!(!self.status.success(), "{message}");
        assert_eq!(self.status.code(), Some(expected_code));
    }

    fn assert_stdout_contains(&self, expected_text: &str, message: &str) {
        let standard_output = self.stdout_text();

        assert!(
            standard_output.contains(expected_text),
            "{message}; expected stdout to contain `{expected_text}`, received: {standard_output}"
        );
    }

    fn assert_stderr_contains(&self, expected_text: &str, message: &str) {
        let standard_error = self.stderr_text();

        assert!(
            standard_error.contains(expected_text),
            "{message}; expected stderr to contain `{expected_text}`, received: {standard_error}"
        );
    }

    fn assert_stderr_not_contains(&self, unexpected_text: &str, message: &str) {
        let standard_error = self.stderr_text();

        assert!(
            !standard_error.contains(unexpected_text),
            "{message}; expected stderr to omit `{unexpected_text}`, received: {standard_error}"
        );
    }

    fn stdout_text(&self) -> String {
        String::from_utf8_lossy(&self.stdout).into_owned()
    }

    fn stderr_text(&self) -> String {
        String::from_utf8_lossy(&self.stderr).into_owned()
    }
}

pub struct TemporaryWorkspace {
    pub root_directory: PathBuf,
}

impl TemporaryWorkspace {
    #[must_use]
    pub fn new(name_prefix: &str) -> Self {
        let root_directory = std::env::temp_dir().join(format!("{name_prefix}-{}", Self::unique_suffix()));

        fs::create_dir_all(&root_directory).expect("temporary root directory should be created");

        Self { root_directory }
    }

    pub fn path(&self, relative_path: impl AsRef<Path>) -> PathBuf {
        self.root_directory.join(relative_path)
    }

    pub fn write_file(&self, relative_path: impl AsRef<Path>, contents: &str) -> PathBuf {
        let absolute_path = self.resolve_path(relative_path);

        if let Some(parent_directory) = absolute_path.parent() {
            fs::create_dir_all(parent_directory).expect("parent directory should be created");
        }

        fs::write(&absolute_path, contents).expect("temporary file should be written");

        absolute_path
    }

    pub fn write_workflow(&self, relative_path: impl AsRef<Path>, source_template: &WorkflowSourceTemplate) -> PathBuf {
        self.write_file(relative_path, source_template.source())
    }

    pub fn write_json_file(&self, relative_path: impl AsRef<Path>, value: &Value) -> PathBuf {
        let contents = serde_json::to_string_pretty(value).expect("json should serialize");

        self.write_file(relative_path, &contents)
    }

    pub fn read_file(&self, path: impl AsRef<Path>) -> String {
        let absolute_path = self.resolve_path(path);

        fs::read_to_string(&absolute_path)
            .unwrap_or_else(|read_error| panic!("temporary file {} should read: {read_error}", absolute_path.display()))
    }

    pub fn read_json_file(&self, path: impl AsRef<Path>) -> Value {
        let absolute_path = self.resolve_path(path);
        let file_contents = self.read_file(&absolute_path);

        serde_json::from_str(&file_contents).unwrap_or_else(|parse_error| {
            panic!(
                "temporary file {} should contain valid json: {parse_error}",
                absolute_path.display()
            )
        })
    }

    pub fn assert_file_exists(&self, path: impl AsRef<Path>, message: &str) {
        let absolute_path = self.resolve_path(path);

        assert!(absolute_path.exists(), "{message}: {}", absolute_path.display());
    }

    pub fn assert_file_missing(&self, path: impl AsRef<Path>, message: &str) {
        let absolute_path = self.resolve_path(path);

        assert!(!absolute_path.exists(), "{message}: {}", absolute_path.display());
    }

    pub fn create_directory(&self, relative_path: impl AsRef<Path>) -> PathBuf {
        let absolute_path = self.resolve_path(relative_path);

        fs::create_dir_all(&absolute_path).expect("temporary directory should be created");

        absolute_path
    }

    fn resolve_path(&self, path: impl AsRef<Path>) -> PathBuf {
        let path = path.as_ref();

        if path.is_absolute() {
            return path.to_path_buf();
        }

        self.root_directory.join(path)
    }

    fn unique_suffix() -> String {
        static UNIQUE_COUNTER: AtomicU64 = AtomicU64::new(0);

        let timestamp_nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be after unix epoch")
            .as_nanos();
        let process_identifier = std::process::id();
        let counter = UNIQUE_COUNTER.fetch_add(1, Ordering::Relaxed);

        format!("{process_identifier}-{timestamp_nanos}-{counter}")
    }
}

impl Drop for TemporaryWorkspace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root_directory);
    }
}
