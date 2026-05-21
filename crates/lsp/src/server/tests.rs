use super::{read_project_mcp_lock, LanguageServer};
use crate::document::DocumentState;
use serde_json::Value;
use std::collections::BTreeMap;
use std::sync::Arc;
use superwire_core::mcp::{McpLock, McpLockResolutionContext, ProjectMcpLock};
use superwire_core::testing::FakeMcpClientFactory;
use superwire_core::workflow_source;

const PLAYGROUND_DOCUMENT_URI: &str = "file:///playground/document.wire";

#[test]
fn reads_mcp_lock_from_project_lock_without_refreshing() {
    let mcp_client_factory = fake_mcp_client_factory();
    let workflow_source = workflow_source! {
        secrets {
            mcp_endpoint: string
        }

        mcp local {
            endpoint: secrets.mcp_endpoint
            headers {
                Accept: "application/json"
            }
        }

        tool update_user_name from mcp.local.tool.update_user_name
    };
    let temp_directory_path = std::env::temp_dir().join(format!(
        "superwire_lsp_lock_test_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("current time should be after unix epoch")
            .as_nanos()
    ));
    std::fs::create_dir_all(&temp_directory_path).expect("temporary directory should be created");
    let temp_file_path = temp_directory_path.join("dynamic.wire");
    std::fs::write(&temp_file_path, workflow_source).expect("temporary workflow should write");
    let document_uri = format!("file://{}", temp_file_path.display());
    let lock_path = temp_directory_path.join("superwire.lock");
    let lock_context = McpLockResolutionContext {
        input: BTreeMap::new(),
        secrets: [("mcp_endpoint".to_string(), Value::String("mcp://local".to_string()))]
            .into_iter()
            .collect(),
        dynamic: BTreeMap::new(),
        agent_outputs: BTreeMap::new(),
        agent_contexts: BTreeMap::new(),
    };
    let discovered_lock = McpLock::discover_from_workflow_with_lock_context_and_client_factory(
        &superwire_core::dsl::parse_workflow(workflow_source).expect("workflow should parse"),
        Some(&lock_context),
        &mcp_client_factory,
    )
    .expect("MCP metadata should discover using lock context");
    let mut project_lock = ProjectMcpLock::empty();

    project_lock.insert_workflow_lock(
        temp_file_path.parent().expect("temporary workflow should have parent"),
        &temp_file_path,
        discovered_lock,
    );
    project_lock.write_to_path(&lock_path).expect("project lock should write");

    let read_lock = read_project_mcp_lock(&document_uri).expect("project lock should read");

    assert!(read_lock.servers.contains_key("local"));
    assert!(!temp_file_path.with_extension("wire.lock").exists());

    let _ = std::fs::remove_dir_all(&temp_directory_path);
}

#[test]
fn discovers_mcp_lock_from_document_when_project_lock_is_missing() {
    let mcp_client_factory = Arc::new(fake_mcp_client_factory());
    let workflow_source = workflow_source! {
        mcp local {
            endpoint: "mcp://local"
        }

        output {
            value: null
        }
    };

    let mut language_server = LanguageServer::with_mcp_client_factory(mcp_client_factory);
    let discovered_lock = language_server
        .resolve_mcp_lock("file:///playground/document.wire", workflow_source, None)
        .expect("MCP metadata should discover from document source");

    assert!(discovered_lock.servers.contains_key("local"));
    assert!(discovered_lock.servers["local"].find_tool_with_name("update_user_name").is_some());
}

#[test]
fn caches_document_mcp_discovery_until_server_settings_change() {
    let mcp_client_factory = Arc::new(fake_mcp_client_factory().with_server("renamed_local", standard_mcp_server));
    let workflow_source = workflow_source! {
        mcp local {
            endpoint: "mcp://local"
        }

        output {
            value: null
        }
    };
    let changed_workflow_source = workflow_source.replace("local", "renamed_local");
    let mut language_server = LanguageServer::with_mcp_client_factory(mcp_client_factory.clone());

    let first_lock = language_server
        .resolve_mcp_lock("file:///playground/document.wire", workflow_source, None)
        .expect("first discovery should succeed");
    let first_request_count = mcp_client_factory.requests("local").len();
    let second_lock = language_server
        .resolve_mcp_lock("file:///playground/document.wire", workflow_source, Some(first_lock))
        .expect("second discovery should reuse cache");

    assert!(second_lock.servers.contains_key("local"));
    assert_eq!(mcp_client_factory.requests("local").len(), first_request_count);

    let changed_lock = language_server
        .resolve_mcp_lock("file:///playground/document.wire", &changed_workflow_source, Some(second_lock))
        .expect("changed settings should trigger discovery");

    assert!(changed_lock.servers.contains_key("renamed_local"));
    assert!(!mcp_client_factory.requests("renamed_local").is_empty());
}

#[test]
fn discovers_mcp_lock_from_playground_runtime_values() {
    let mcp_client_factory = Arc::new(fake_mcp_client_factory());
    let workflow_source = workflow_source! {
        provider openai from openai {
            endpoint: "https://api.openai.com/v1"
            api_key: "test-api-key"
        }

        model openai_model from openai {
            id: "gpt-4.1-mini"
        }

        secrets {
            mcp_endpoint: string
        }

        mcp local {
            endpoint: secrets.mcp_endpoint
        }

        from mcp.local {
            tool list_all_participants_who_has_answered_given_task
        }

        dynamic {
            data: call tool.list_all_participants_who_has_answered_given_task {}
        }

        agent participant_answer_analyzer for participant in dynamic.data.participants {
            model: model.openai_model
            instruction: "Analyze the participant answer"
            output {
                value: string
            }
        }

        output {
            value: agent.participant_answer_analyzer.value
        }
    };
    let mut language_server = LanguageServer::with_mcp_client_factory(mcp_client_factory);

    language_server.runtime_values_by_document_uri.insert(
        PLAYGROUND_DOCUMENT_URI.to_string(),
        super::RuntimeValues {
            input: Value::Null,
            secrets: serde_json::json!({ "mcp_endpoint": "mcp://local" }),
        },
    );

    let discovered_lock = language_server
        .resolve_mcp_lock(PLAYGROUND_DOCUMENT_URI, workflow_source, None)
        .expect("MCP metadata should discover from playground runtime values");
    let document_state = DocumentState::new(workflow_source.to_string(), Some(discovered_lock));
    let diagnostic_messages = document_state
        .diagnostics()
        .into_iter()
        .map(|diagnostic| diagnostic.message)
        .collect::<Vec<_>>();

    assert!(
        diagnostic_messages
            .iter()
            .all(|message| !message.contains("dynamic.data.participants")),
        "expected participant field reference to validate from MCP output schema; got {diagnostic_messages:?}"
    );
}

#[test]
#[allow(clippy::too_many_lines)]
fn handles_playground_runtime_values_notification_and_completes_mcp_tools() {
    let mcp_client_factory = Arc::new(fake_mcp_client_factory());
    let workflow_template = workflow_source! {
        secrets {
            mcp_endpoint: string
        }

        mcp local {
            endpoint: secrets.mcp_endpoint
        }

        from mcp.local {
            bindings {
                project_id: input.project_id
                task_id: input.task_id
            }

            tool <cursor>
            tool fetch_participant_answer
        }
    };
    let cursor_offset = workflow_template
        .find("<cursor>")
        .expect("source template should contain cursor marker");
    let source_before_cursor = &workflow_template[..cursor_offset];
    let cursor_line = source_before_cursor.lines().count().saturating_sub(1);
    let cursor_character = source_before_cursor
        .rsplit('\n')
        .next()
        .expect("source before cursor should contain current line")
        .chars()
        .count();
    let workflow_source = workflow_template.replace("<cursor>", "");
    let cursor_position = serde_json::json!({
        "line": cursor_line,
        "character": cursor_character
    });
    let mut language_server = LanguageServer::with_mcp_client_factory(mcp_client_factory.clone());

    language_server
        .handle_json_rpc_message(
            serde_json::json!({
                "jsonrpc": "2.0",
                "method": "textDocument/didOpen",
                "params": {
                    "textDocument": {
                        "uri": PLAYGROUND_DOCUMENT_URI,
                        "languageId": "wire",
                        "version": 1,
                        "text": workflow_source
                    }
                }
            })
            .to_string()
            .as_bytes(),
        )
        .expect("didOpen should be accepted");

    language_server
        .handle_json_rpc_message(
            serde_json::json!({
                "jsonrpc": "2.0",
                "method": "superwire/runtimeValues",
                "params": {
                    "textDocument": { "uri": PLAYGROUND_DOCUMENT_URI },
                    "input": {
                        "project_id": 7,
                        "task_id": 11
                    },
                    "secrets": {
                        "mcp_endpoint": "mcp://local"
                    }
                }
            })
            .to_string()
            .as_bytes(),
        )
        .expect("runtime values notification should be accepted");
    assert!(
        !mcp_client_factory.requests("local").is_empty(),
        "runtime values should trigger MCP discovery"
    );

    let completion_messages = language_server
        .handle_json_rpc_message(
            serde_json::json!({
                "jsonrpc": "2.0",
                "id": 10,
                "method": "textDocument/completion",
                "params": {
                    "textDocument": { "uri": PLAYGROUND_DOCUMENT_URI },
                    "position": cursor_position
                }
            })
            .to_string()
            .as_bytes(),
        )
        .expect("completion request should be accepted")
        .messages;
    let completion_labels = completion_messages[0]
        .pointer("/result/items")
        .and_then(Value::as_array)
        .expect("completion response should contain items")
        .iter()
        .filter_map(|completion_item| completion_item.get("label").and_then(Value::as_str))
        .collect::<Vec<_>>();

    assert!(
        completion_labels.contains(&"list_all_participants_who_has_answered_given_task"),
        "expected runtime MCP completion labels to include participant listing tool; got {completion_labels:?} from messages {completion_messages:?}"
    );
    assert!(
        !completion_labels.contains(&"fetch_participant_answer"),
        "expected existing imported tool to be excluded; got {completion_labels:?}"
    );
}

fn fake_mcp_client_factory() -> FakeMcpClientFactory {
    FakeMcpClientFactory::new().with_server("local", standard_mcp_server)
}

fn standard_mcp_server(server_builder: &mut superwire_core::testing::FakeMcpServerBuilder) {
    server_builder.tool("update-user-name", |tool_builder| {
        tool_builder
            .description("Update user name")
            .input_schema(serde_json::json!({
                "type": "object",
                "properties": {
                    "user_name": { "type": "string" }
                },
                "required": ["user_name"]
            }))
            .output_schema(serde_json::json!({
                "type": "object",
                "properties": {
                    "success": { "type": "boolean" }
                },
                "required": ["success"]
            }));
    });

    server_builder.tool("list_all_participants_who_has_answered_given_task", |tool_builder| {
        tool_builder
            .description("List all participants who answered a task")
            .input_schema(serde_json::json!({
                "type": "object",
                "properties": {}
            }))
            .output_schema(serde_json::json!({
                "type": "object",
                "properties": {
                    "participants": {
                        "type": "array",
                        "items": { "type": "object" }
                    }
                },
                "required": ["participants"]
            }));
    });
}
