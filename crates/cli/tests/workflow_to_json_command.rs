use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};

macro_rules! workflow_template {
    ($($workflow_tokens:tt)*) => {{
        stringify!($($workflow_tokens)*)
    }};
}

#[test]
fn exports_execution_graph_with_parallel_batches() {
    let temporary_workspace = TemporaryWorkspace::new();
    let workflow_source = workflow_template! {
        provider openai {
            driver: "openai"
            endpoint: "https://api.openai.com/v1"
            api_key: "test-api-key"
            models: ["gpt-4.1-mini"]
        }

        input {
            product_name: string
            release_highlights: [string]
        }

        agent changelog {
            model: openai("gpt-4.1-mini")
            prompt: "Write release notes for {{ input.product_name }} with {{ input.release_highlights }}"
            output: {
                markdown: string
            }
        }

        agent social_thread {
            model: openai("gpt-4.1-mini")
            prompt: "Create posts for {{ input.product_name }} with {{ input.release_highlights }}"
            output: {
                posts: [string; 5]
            }
        }

        agent customer_email {
            model: openai("gpt-4.1-mini")
            prompt: "Write email for {{ input.product_name }} with {{ input.release_highlights }}"
            output: {
                subject: string
                body: string
            }
        }

        agent consistency_review {
            model: openai("gpt-4.1-mini")
            prompt: "Check {{ agent.changelog.markdown }} {{ agent.social_thread.posts }} {{ agent.customer_email.subject }}"
            output: {
                approved: boolean
            }
        }

        output {
            changelog_markdown: agent.changelog.markdown
            social_posts: agent.social_thread.posts
            email_subject: agent.customer_email.subject
            approved: agent.consistency_review.approved
        }
    };

    let workflow_file_path = temporary_workspace.write_file("parallel.wire", workflow_source);

    let command_output = run_workflow_to_json_command(&[workflow_file_path.as_os_str()]);

    assert!(command_output.status.success(), "workflow to-json command should succeed");

    let exported_json: Value = serde_json::from_slice(&command_output.stdout).expect("workflow to-json output should be valid json");

    assert_eq!(
        exported_json.get("format").and_then(Value::as_str),
        Some("superwire_workflow_compact_v1")
    );

    assert_eq!(exported_json.pointer("/providers/0/models"), Some(&json!(["gpt-4.1-mini"])));

    assert_eq!(
        exported_json.pointer("/execution/batches"),
        Some(&json!([["changelog", "social_thread", "customer_email"], ["consistency_review"]]))
    );

    assert_eq!(exported_json.pointer("/agents/0/dependents"), Some(&json!(["consistency_review"])));

    assert_eq!(exported_json.pointer("/agents/1/dependents"), Some(&json!(["consistency_review"])));

    assert_eq!(exported_json.pointer("/agents/2/dependents"), Some(&json!(["consistency_review"])));
}

#[test]
fn writes_json_to_output_file_when_requested() {
    let temporary_workspace = TemporaryWorkspace::new();
    let workflow_source = workflow_template! {
        provider openai {
            driver: "openai"
            endpoint: "https://api.openai.com/v1"
            api_key: "test-api-key"
            models: ["gpt-4.1-mini"]
        }

        input {
            numbers: [number]
        }

        agent collect_note for number_item in input.numbers {
            model: openai("gpt-4.1-mini")
            prompt: "Write note for {{ number_item }}"
            output: string
        }

        output {
            notes: agent.collect_note
        }
    };

    let workflow_file_path = temporary_workspace.write_file("for-loop.wire", workflow_source);
    let output_json_path = temporary_workspace.root_directory.join("export.json");

    let command_output = run_workflow_to_json_command(&[
        workflow_file_path.as_os_str(),
        std::ffi::OsStr::new("--output"),
        output_json_path.as_os_str(),
        std::ffi::OsStr::new("--compact"),
    ]);

    assert!(command_output.status.success(), "workflow to-json command should succeed");
    assert!(command_output.stdout.is_empty(), "stdout should be empty when --output is provided");

    let output_contents = fs::read_to_string(&output_json_path).expect("json output file should be written");
    let exported_json: Value = serde_json::from_str(&output_contents).expect("output file should contain valid json");

    assert_eq!(exported_json.pointer("/execution/batches"), Some(&json!([["collect_note"]])));

    assert_eq!(
        exported_json.pointer("/agents/0/for_each/pattern/identifier"),
        Some(&json!("number_item"))
    );

    assert_eq!(
        exported_json.pointer("/agents/0/output/final_output/workflow_type/kind"),
        Some(&json!("array"))
    );

    assert_eq!(
        exported_json.pointer("/output/fields/notes/$ref"),
        Some(&json!("agent.collect_note"))
    );
}

#[test]
fn exports_tool_input_and_bounded_schemas() {
    let temporary_workspace = TemporaryWorkspace::new();
    let workflow_source = workflow_template! {
        provider openai {
            driver: "openai"
            endpoint: "https://api.openai.com/v1"
            api_key: "test-api-key"
            models: ["gpt-4.1-mini"]
        }

        tool issue_tracker_lookup {
            input {
                issue_id: number
            }

            bounded {
                project: string
                status: "open" | "closed"
            }
        }

        agent assistant {
            model: openai("gpt-4.1-mini")
            tools: [tool.issue_tracker_lookup(project: "superwire", status: "open")]
            prompt: "lookup issue"
            output: string
        }

        output {
            result: agent.assistant
        }
    };

    let workflow_file_path = temporary_workspace.write_file("tool-schemas.wire", workflow_source);
    let command_output = run_workflow_to_json_command(&[workflow_file_path.as_os_str()]);

    assert!(command_output.status.success(), "workflow to-json command should succeed");

    let exported_json: Value = serde_json::from_slice(&command_output.stdout).expect("workflow to-json output should be valid json");

    assert_eq!(
        exported_json.pointer("/tools/0/input_schema/properties/issue_id/type"),
        Some(&json!("integer"))
    );

    assert_eq!(exported_json.pointer("/tools/0/input_schema/required"), Some(&json!(["issue_id"])));

    assert_eq!(
        exported_json.pointer("/tools/0/bounded_schema/required"),
        Some(&json!(["project", "status"]))
    );

    assert_eq!(
        exported_json.pointer("/tools/0/bounded_schema/properties/project/type"),
        Some(&json!("string"))
    );

    assert_eq!(
        exported_json.pointer("/tools/0/bounded_schema/properties/status/enum"),
        Some(&json!(["closed", "open"]))
    );
}

#[test]
fn omits_empty_required_array_for_tool_without_agent_input() {
    let temporary_workspace = TemporaryWorkspace::new();
    let workflow_source = workflow_template! {
        provider openai {
            driver: "openai"
            endpoint: "https://api.openai.com/v1"
            api_key: "test-api-key"
            models: ["gpt-4.1-mini"]
        }

        tool list_all_participants {
            bounded {
                project_id: number
            }
        }

        agent assistant {
            model: openai("gpt-4.1-mini")
            tools: [tool.list_all_participants(project_id: 1)]
            prompt: "list participants"
            output: string
        }

        output {
            result: agent.assistant
        }
    };

    let workflow_file_path = temporary_workspace.write_file("tool-empty-input-schema.wire", workflow_source);
    let command_output = run_workflow_to_json_command(&[workflow_file_path.as_os_str()]);

    assert!(command_output.status.success(), "workflow to-json command should succeed");

    let exported_json: Value = serde_json::from_slice(&command_output.stdout).expect("workflow to-json output should be valid json");

    assert_eq!(exported_json.pointer("/tools/0/input_schema/type"), Some(&json!("object")));
    assert_eq!(exported_json.pointer("/tools/0/input_schema/required"), None);
}

fn run_workflow_to_json_command(arguments: &[&std::ffi::OsStr]) -> Output {
    let mut command = Command::new(cli_binary_path());
    command.arg("workflow").arg("to-json");

    for argument in arguments {
        command.arg(argument);
    }

    command.output().expect("workflow to-json command should run")
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
        let root_directory = std::env::temp_dir().join(format!("superwire-workflow-to-json-tests-{unique_suffix}"));

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
