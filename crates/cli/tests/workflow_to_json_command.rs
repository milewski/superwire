use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};
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
fn exports_tool_input_and_binding_schemas() {
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

            bindings {
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
        exported_json.pointer("/tools/0/binding_schema/required"),
        Some(&json!(["project", "status"]))
    );

    assert_eq!(
        exported_json.pointer("/tools/0/binding_schema/properties/project/type"),
        Some(&json!("string"))
    );

    assert_eq!(
        exported_json.pointer("/tools/0/binding_schema/properties/status/enum"),
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
            bindings {
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

#[test]
fn exports_workflow_and_agent_dynamic_blocks() {
    let temporary_workspace = TemporaryWorkspace::new();
    let workflow_source = workflow_template! {
        provider openai {
            driver: "openai"
            endpoint: "https://api.openai.com/v1"
            api_key: "test-api-key"
            models: ["gpt-4.1-mini"]
        }

        dynamic {
            prompt_seed: "hello"
        }

        agent assistant {
            model: openai("gpt-4.1-mini")

            dynamic {
                rendered_prompt: dynamic.prompt_seed
            }

            prompt: dynamic.rendered_prompt
            output: string
        }

        output {
            result: agent.assistant
        }
    };

    let workflow_file_path = temporary_workspace.write_file("dynamic-to-json.wire", workflow_source);
    let command_output = run_workflow_to_json_command(&[workflow_file_path.as_os_str()]);

    assert!(command_output.status.success(), "workflow to-json command should succeed");

    let exported_json: Value = serde_json::from_slice(&command_output.stdout).expect("workflow to-json output should be valid json");

    assert_eq!(exported_json.pointer("/dynamic/prompt_seed"), Some(&json!("hello")));
    assert_eq!(
        exported_json.pointer("/agents/0/dynamic/rendered_prompt/$ref"),
        Some(&json!("dynamic.prompt_seed"))
    );
    assert_eq!(
        exported_json.pointer("/agents/0/prompt/$ref"),
        Some(&json!("dynamic.rendered_prompt"))
    );
}

#[test]
fn exports_workflow_output_fields_that_reference_dynamic_values() {
    let temporary_workspace = TemporaryWorkspace::new();
    let workflow_source = workflow_template! {
        provider openai {
            driver: "openai"
            endpoint: "https://api.openai.com/v1"
            api_key: "test-api-key"
            models: ["gpt-4.1-mini"]
        }

        input {
            topic: string
        }

        dynamic {
            max_bullets: 3
            audience: "engineering"
        }

        dynamic {
            metadata: {
                workflow: "dynamic_values"
            }
        }

        agent writer {
            model: openai("gpt-4.1-mini")
            prompt: "Write about {{ input.topic }} for {{ dynamic.audience }}"
            output: {
                summary: string
            }
        }

        output {
            topic: input.topic
            audience: dynamic.audience
            max_bullets: dynamic.max_bullets
            workflow_name: dynamic.metadata.workflow
            summary: agent.writer.summary
        }
    };

    let workflow_file_path = temporary_workspace.write_file("dynamic-output-to-json.wire", workflow_source);
    let command_output = run_workflow_to_json_command(&[workflow_file_path.as_os_str()]);

    assert!(command_output.status.success(), "workflow to-json command should succeed");

    let exported_json: Value = serde_json::from_slice(&command_output.stdout).expect("workflow to-json output should be valid json");

    assert_eq!(
        exported_json.pointer("/output/fields/audience/$ref"),
        Some(&json!("dynamic.audience"))
    );
    assert_eq!(
        exported_json.pointer("/output/fields/max_bullets/$ref"),
        Some(&json!("dynamic.max_bullets"))
    );
    assert_eq!(
        exported_json.pointer("/output/fields/workflow_name/$ref"),
        Some(&json!("dynamic.metadata.workflow"))
    );
    assert_eq!(
        exported_json.pointer("/output/contract/workflow_type/fields/audience/kind"),
        Some(&json!("string"))
    );
    assert_eq!(
        exported_json.pointer("/output/contract/workflow_type/fields/max_bullets/kind"),
        Some(&json!("integer"))
    );
    assert_eq!(
        exported_json.pointer("/output/contract/workflow_type/fields/workflow_name/kind"),
        Some(&json!("string"))
    );
}

#[test]
fn exports_workflow_dynamic_tool_calls_used_by_output_fields() {
    let temporary_workspace = TemporaryWorkspace::new();
    let workflow_source = workflow_template! {
        input {
            query: string
        }

        tool searchable_web {
            input {
                query: string
            }

            output {
                title: string
            }
        }

        dynamic {
            search_result: call tool.searchable_web {
                input {
                    query: input.query
                }
            }
        }

        output {
            title: dynamic.search_result.title
        }
    };

    let workflow_file_path = temporary_workspace.write_file("dynamic-tool-output-to-json.wire", workflow_source);
    let command_output = run_workflow_to_json_command(&[workflow_file_path.as_os_str()]);

    assert!(command_output.status.success(), "workflow to-json command should succeed");

    let exported_json: Value = serde_json::from_slice(&command_output.stdout).expect("workflow to-json output should be valid json");

    assert_eq!(
        exported_json.pointer("/dynamic/search_result/$tool_call"),
        Some(&json!("tool.searchable_web"))
    );
    assert_eq!(
        exported_json.pointer("/output/contract/workflow_type/fields/title/kind"),
        Some(&json!("string"))
    );
}

#[test]
fn exports_fixed_tool_bindings_without_requiring_them_in_calls() {
    let temporary_workspace = TemporaryWorkspace::new();
    let workflow_source = workflow_template! {
        tool example {
            bindings {
                property_a: number
                property_b: "direct assignment"
            }

            output {
                value: string
            }
        }

        dynamic {
            example: call tool.example {
                bindings {
                    property_a: 123
                }
            }
        }

        output {
            value: dynamic.example.value
        }
    };

    let workflow_file_path = temporary_workspace.write_file("fixed-tool-bindings.wire", workflow_source);
    let command_output = run_workflow_to_json_command(&[workflow_file_path.as_os_str()]);

    assert!(command_output.status.success(), "workflow to-json command should succeed");

    let exported_json: Value = serde_json::from_slice(&command_output.stdout).expect("workflow to-json output should be valid json");

    assert_eq!(exported_json.pointer("/tools/0/bindings/0/name"), Some(&json!("property_a")));
    assert_eq!(
        exported_json.pointer("/tools/0/fixed_bindings/property_b"),
        Some(&json!("direct assignment"))
    );
    assert_eq!(
        exported_json.pointer("/tools/0/binding_schema/required"),
        Some(&json!(["property_a"]))
    );
}

#[test]
fn exports_fixed_tool_bindings_from_references_and_literals() {
    let temporary_workspace = TemporaryWorkspace::new();
    let workflow_source = workflow_template! {
        input {
            project_id: number
            task_id: number
        }

        tool example {
            bindings {
                project_id: input.project_id
                retry_count: 123
                task_id: input.task_id
            }

            output {
                value: string
            }
        }

        dynamic {
            example: call tool.example
        }

        output {
            value: dynamic.example.value
        }
    };

    let workflow_file_path = temporary_workspace.write_file("fixed-tool-reference-bindings.wire", workflow_source);
    let command_output = run_workflow_to_json_command(&[workflow_file_path.as_os_str()]);

    assert!(command_output.status.success(), "workflow to-json command should succeed");

    let exported_json: Value = serde_json::from_slice(&command_output.stdout).expect("workflow to-json output should be valid json");

    assert_eq!(
        exported_json.pointer("/tools/0/fixed_bindings/project_id/$ref"),
        Some(&json!("input.project_id"))
    );
    assert_eq!(exported_json.pointer("/tools/0/fixed_bindings/retry_count"), Some(&json!(123)));
    assert_eq!(
        exported_json.pointer("/tools/0/fixed_bindings/task_id/$ref"),
        Some(&json!("input.task_id"))
    );
}

#[test]
fn rejects_dynamic_tool_calls_missing_required_typed_bindings() {
    let temporary_workspace = TemporaryWorkspace::new();
    let workflow_source = workflow_template! {
        tool example {
            bindings {
                property_a: number
                property_b: "direct assignment"
            }

            output {
                value: string
            }
        }

        dynamic {
            example: call tool.example {
            }
        }

        output {
            value: dynamic.example.value
        }
    };

    let workflow_file_path = temporary_workspace.write_file("missing-tool-binding.wire", workflow_source);
    let command_output = run_workflow_to_json_command(&[workflow_file_path.as_os_str()]);

    assert!(!command_output.status.success(), "workflow to-json command should fail");

    let stderr = String::from_utf8_lossy(&command_output.stderr);
    assert!(
        stderr.contains("missing required `bindings` field `property_a`"),
        "unexpected stderr: {stderr}"
    );
}

#[test]
fn rejects_dynamic_tool_calls_with_wrong_typed_binding_type() {
    let temporary_workspace = TemporaryWorkspace::new();
    let workflow_source = workflow_template! {
        tool example {
            bindings {
                property_a: number
            }

            output {
                value: string
            }
        }

        dynamic {
            example: call tool.example {
                bindings {
                    property_a: "wrong"
                }
            }
        }

        output {
            value: dynamic.example.value
        }
    };

    let workflow_file_path = temporary_workspace.write_file("wrong-tool-binding-type.wire", workflow_source);
    let command_output = run_workflow_to_json_command(&[workflow_file_path.as_os_str()]);

    assert!(!command_output.status.success(), "workflow to-json command should fail");

    let stderr = String::from_utf8_lossy(&command_output.stderr);
    assert!(stderr.contains("expects number, found string"), "unexpected stderr: {stderr}");
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
    static NEXT_UNIQUE_SUFFIX: AtomicU64 = AtomicU64::new(0);

    let timestamp_millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after unix epoch")
        .as_nanos();

    let process_identifier = std::process::id();
    let sequence = NEXT_UNIQUE_SUFFIX.fetch_add(1, Ordering::Relaxed);

    format!("{timestamp_millis}-{process_identifier}-{sequence}")
}
