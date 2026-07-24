use super::{read_project_mcp_lock, CompletionSuggestion, LanguageServer};
use crate::document::DocumentState;
use serde_json::Value;
use std::collections::BTreeMap;
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;
use superwire_macros::workflow_source;
use superwire_mcp::{
    McpClientBackend, McpClientFactory, McpError, McpLock, McpLockResolutionContext, McpNetworkPolicy, McpServerConfig, McpServerLock,
    PolicyMcpClientFactory, ProjectMcpLock,
};
use superwire_test_support::{FakeMcpClientFactory, FakeMcpServerBuilder};

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
        &superwire_dsl::parse_workflow(workflow_source).expect("workflow should parse"),
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
fn automatic_mcp_discovery_is_disabled_without_explicit_workspace_trust() {
    let mcp_client_factory = Arc::new(fake_mcp_client_factory());
    let workflow_source = workflow_source! {
        secrets {
            authorization: string
        }

        mcp local {
            endpoint: "mcp://local"
            headers {
                Authorization: secrets.authorization
            }
        }
    };
    let changed_workflow_source = workflow_source! {
        secrets {
            authorization: string
        }

        mcp local {
            endpoint: "mcp://local"
            headers {
                Authorization: secrets.authorization
            }
        }

        output {
            value: null
        }
    };
    let mut language_server = LanguageServer::with_mcp_client_factory(mcp_client_factory.clone());

    open_document(&mut language_server, PLAYGROUND_DOCUMENT_URI, 1, workflow_source);
    change_document(&mut language_server, PLAYGROUND_DOCUMENT_URI, 2, changed_workflow_source);
    send_runtime_values(
        &mut language_server,
        PLAYGROUND_DOCUMENT_URI,
        Value::Null,
        serde_json::json!({ "authorization": "Bearer private-token" }),
    );

    assert!(language_server.pending_mcp_discoveries.is_empty());
    assert!(mcp_client_factory.requests("local").is_empty());
}

#[test]
fn trusted_initialization_option_enables_async_mcp_discovery() {
    let mcp_client_factory = Arc::new(fake_mcp_client_factory());
    let workflow_source = workflow_source! {
        mcp local {
            endpoint: "mcp://local"
        }
    };
    let mut language_server = LanguageServer::with_mcp_client_factory(mcp_client_factory.clone());
    let initialize_messages = initialize_with_network_mcp_discovery_trust(&mut language_server);

    assert_eq!(
        initialize_messages[0]
            .pointer("/result/capabilities/experimental/superwire/initializationOptions/workspaceTrust/networkMcpDiscovery/default"),
        Some(&Value::String("disabled".to_string()))
    );
    assert_eq!(
        language_server.network_mcp_discovery_trust,
        super::NetworkMcpDiscoveryTrust::Trusted
    );

    open_document(&mut language_server, PLAYGROUND_DOCUMENT_URI, 1, workflow_source);

    assert!(language_server
        .receive_and_apply_mcp_discovery_result(Duration::from_secs(1))
        .is_some());
    assert!(!mcp_client_factory.requests("local").is_empty());
}

#[test]
fn default_language_server_factory_remains_offline_when_trust_is_requested() {
    let workflow_source = workflow_source! {
        mcp local {
            endpoint: "http://127.0.0.1:9/mcp"
        }
    };
    let mut language_server = LanguageServer::default();

    initialize_with_network_mcp_discovery_trust(&mut language_server);

    assert!(language_server
        .resolve_mcp_lock(PLAYGROUND_DOCUMENT_URI, workflow_source, None)
        .is_none());
}

#[test]
fn initialization_trust_values_are_exact_and_unsupported_values_fail_closed() {
    let mcp_client_factory = Arc::new(fake_mcp_client_factory());
    let mut language_server = LanguageServer::with_mcp_client_factory(mcp_client_factory);
    let disabled_messages = initialize_with_network_mcp_discovery_value(&mut language_server, Value::String("disabled".to_string()));

    assert!(disabled_messages[0]["result"].is_object());
    assert_eq!(
        language_server.network_mcp_discovery_trust,
        super::NetworkMcpDiscoveryTrust::Disabled
    );

    let trusted_messages = initialize_with_network_mcp_discovery_value(&mut language_server, Value::String("trusted".to_string()));

    assert!(trusted_messages[0]["result"].is_object());
    assert_eq!(
        language_server.network_mcp_discovery_trust,
        super::NetworkMcpDiscoveryTrust::Trusted
    );

    for unsupported_value in [
        Value::String("public-only".to_string()),
        Value::String("future-policy".to_string()),
        Value::Bool(true),
    ] {
        let invalid_messages = initialize_with_network_mcp_discovery_value(&mut language_server, unsupported_value);
        let invalid_response = &invalid_messages[0];

        assert_eq!(invalid_response["error"]["code"], -32602);
        assert!(invalid_response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("initializationOptions.workspaceTrust.networkMcpDiscovery")));
        assert_eq!(
            invalid_response["error"]["data"]["supportedValues"],
            serde_json::json!(["disabled", "trusted"])
        );
        assert_eq!(
            language_server.network_mcp_discovery_trust,
            super::NetworkMcpDiscoveryTrust::Disabled
        );
    }
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

    let language_server = LanguageServer::with_mcp_client_factory(mcp_client_factory);
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
    let language_server = LanguageServer::with_mcp_client_factory(mcp_client_factory.clone());

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
fn discovery_cache_debug_and_keys_do_not_retain_plaintext_runtime_secrets() {
    let mcp_client_factory = Arc::new(fake_mcp_client_factory());
    let workflow_source = workflow_source! {
        secrets {
            authorization: string
        }

        mcp local {
            endpoint: "mcp://local"
            headers {
                Authorization: secrets.authorization
            }
        }
    };
    let runtime_values = super::RuntimeValues {
        input: Value::Null,
        secrets: serde_json::json!({ "authorization": "Bearer private-token" }),
    };
    let mut discovery_cache = super::McpDiscoveryCache::with_limits(mcp_client_factory, 4, Duration::from_secs(60));

    discovery_cache
        .resolve_mcp_lock(PLAYGROUND_DOCUMENT_URI, workflow_source, None, Some(&runtime_values))
        .expect("runtime MCP discovery should populate the cache");

    let cache_debug = format!("{discovery_cache:?}");
    let runtime_values_debug = format!("{runtime_values:?}");

    assert!(cache_debug.contains("sha256:"));
    assert!(!cache_debug.contains("private-token"));
    assert!(!cache_debug.contains("Bearer"));
    assert!(!runtime_values_debug.contains("private-token"));
    assert!(!runtime_values_debug.contains("Bearer"));
}

#[test]
fn endpoint_policy_approval_precedes_runtime_header_evaluation() {
    let workflow_source = workflow_source! {
        secrets {
            authorization: string
        }

        mcp local {
            endpoint: "http://127.0.0.1:9/mcp"
            headers {
                Authorization: secrets.authorization
            }
        }
    };
    let workflow = superwire_dsl::parse_workflow(workflow_source).expect("workflow should parse");
    let lock_resolution_context = McpLockResolutionContext::default();
    let mut discovery_cache = super::McpDiscoveryCache::with_limits(
        Arc::new(PolicyMcpClientFactory::new(McpNetworkPolicy::Disabled)),
        1,
        Duration::from_secs(60),
    );
    let discovery_error = discovery_cache
        .discover_from_workflow_with_context(&workflow, &lock_resolution_context)
        .expect_err("disabled endpoint policy should reject before the missing header secret is evaluated");

    assert!(matches!(discovery_error, McpError::NetworkPolicyViolation { .. }));
}

#[test]
fn discovery_cache_evicts_least_recently_used_entries_at_capacity() {
    let mcp_client_factory = Arc::new(
        fake_mcp_client_factory()
            .with_server("newest", newest_mcp_server)
            .with_server("third", standard_mcp_server),
    );
    let local_source = workflow_source! {
        mcp local {
            endpoint: "mcp://local"
        }
    };
    let newest_source = workflow_source! {
        mcp newest {
            endpoint: "mcp://newest"
        }
    };
    let third_source = workflow_source! {
        mcp third {
            endpoint: "mcp://third"
        }
    };
    let mut discovery_cache = super::McpDiscoveryCache::with_limits(mcp_client_factory.clone(), 2, Duration::from_secs(60));

    discovery_cache
        .resolve_mcp_lock(PLAYGROUND_DOCUMENT_URI, local_source, None, None)
        .expect("local MCP discovery should succeed");
    discovery_cache
        .resolve_mcp_lock(PLAYGROUND_DOCUMENT_URI, newest_source, None, None)
        .expect("newest MCP discovery should succeed");
    let local_request_count = mcp_client_factory.requests("local").len();
    let newest_request_count = mcp_client_factory.requests("newest").len();

    discovery_cache
        .resolve_mcp_lock(PLAYGROUND_DOCUMENT_URI, local_source, None, None)
        .expect("local MCP cache hit should succeed");
    discovery_cache
        .resolve_mcp_lock(PLAYGROUND_DOCUMENT_URI, third_source, None, None)
        .expect("third MCP discovery should evict the least recently used entry");
    discovery_cache
        .resolve_mcp_lock(PLAYGROUND_DOCUMENT_URI, newest_source, None, None)
        .expect("evicted newest MCP entry should be rediscovered");

    assert_eq!(mcp_client_factory.requests("local").len(), local_request_count);
    assert!(mcp_client_factory.requests("newest").len() > newest_request_count);
    assert_eq!(discovery_cache.server_locks_by_config_key.len(), 2);
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
    enable_network_mcp_discovery(&mut language_server);

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
    let mut accepted_discovery_notification = None;

    for _ in 0..2 {
        if let Some(diagnostics_notification) = language_server.receive_and_apply_mcp_discovery_result(Duration::from_secs(1)) {
            accepted_discovery_notification = Some(diagnostics_notification);

            break;
        }
    }

    assert!(accepted_discovery_notification.is_some());
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
    let completion_labels = completion_labels(&completion_messages);

    assert!(
        completion_labels.contains(&"list_all_participants_who_has_answered_given_task"),
        "expected runtime MCP completion labels to include participant listing tool; got {completion_labels:?} from messages {completion_messages:?}"
    );
    assert!(
        !completion_labels.contains(&"fetch_participant_answer"),
        "expected existing imported tool to be excluded; got {completion_labels:?}"
    );
}

#[test]
fn did_open_and_feature_requests_remain_responsive_during_mcp_discovery() {
    let (mcp_client_factory, discovery_gate) = delayed_mcp_client_factory();
    let workflow_source = workflow_source! {
        mcp local {
            endpoint: "mcp://local"
        }
    };
    let document_uri = "file:///tmp/non-blocking-mcp.wire";
    let mut language_server = LanguageServer::with_mcp_client_factory(mcp_client_factory);
    enable_network_mcp_discovery(&mut language_server);

    open_document(&mut language_server, document_uri, 1, workflow_source);

    assert_eq!(discovery_gate.wait_until_started(), "local");
    assert_eq!(language_server.pending_mcp_discoveries.len(), 1);

    let (source_text, previous_mcp_lock) = {
        let document_state = language_server.documents.get(document_uri).expect("opened document should exist");

        (Arc::<str>::from(document_state.source_text()), document_state.mcp_lock())
    };
    let duplicate_scheduled = language_server.schedule_mcp_discovery(document_uri.to_string(), 1, source_text, previous_mcp_lock);

    assert!(!duplicate_scheduled);

    let feature_messages = completion_messages(&mut language_server, document_uri, serde_json::json!({ "line": 0, "character": 0 }));

    assert!(feature_messages.iter().any(|message| message.get("id") == Some(&Value::from(10))));

    discovery_gate.release();

    assert!(language_server
        .receive_and_apply_mcp_discovery_result(Duration::from_secs(1))
        .is_some());
}

#[test]
fn retains_previous_mcp_snapshot_while_new_version_discovers() {
    let previous_source = workflow_source! {
        mcp local {
            endpoint: "mcp://local"
        }
    };
    let preparation_server = LanguageServer::with_mcp_client_factory(Arc::new(fake_mcp_client_factory()));
    let previous_mcp_lock = preparation_server
        .resolve_mcp_lock("file:///tmp/previous-mcp.wire", previous_source, None)
        .expect("previous source should resolve an MCP lock");
    let (mcp_client_factory, discovery_gate) = delayed_mcp_client_factory();
    let workflow_template = workflow_source! {
        mcp local {
            endpoint: "mcp://changed"
        }

        from mcp.local.tool {
            tool <cursor>
        }
    };
    let (current_source, cursor_position) = source_and_cursor(workflow_template);
    let document_uri = "file:///tmp/previous-mcp.wire";
    let mut language_server = LanguageServer::with_mcp_client_factory(mcp_client_factory);
    enable_network_mcp_discovery(&mut language_server);

    language_server.documents.insert(
        document_uri.to_string(),
        DocumentState::from_versioned_text(
            previous_source.to_string(),
            Some(previous_mcp_lock.clone()),
            Some(1),
            crate::document::PositionEncoding::default(),
        ),
    );
    change_document(&mut language_server, document_uri, 2, &current_source);

    assert_eq!(discovery_gate.wait_until_started(), "local");
    assert_eq!(
        language_server.documents.get(document_uri).and_then(DocumentState::mcp_lock),
        Some(previous_mcp_lock)
    );

    let completion_messages = completion_messages(&mut language_server, document_uri, cursor_position);
    let completion_labels = completion_labels(&completion_messages);

    assert!(completion_labels.contains(&"update_user_name"));

    discovery_gate.release();

    assert!(language_server
        .receive_and_apply_mcp_discovery_result(Duration::from_secs(1))
        .is_some());
}

#[test]
fn coalesces_burst_versions_and_dispatches_only_first_and_latest() {
    let (mcp_client_factory, discovery_gate) = delayed_mcp_client_factory();
    let first_source = workflow_source! {
        mcp local {
            endpoint: "mcp://local"
        }
    };
    let superseded_source = workflow_source! {
        mcp newest {
            endpoint: "mcp://newest"
        }
    };
    let latest_source = workflow_source! {
        mcp third {
            endpoint: "mcp://third"
        }
    };
    let document_uri = "file:///tmp/latest-wins-mcp.wire";
    let mut language_server = LanguageServer::with_mcp_client_factory(mcp_client_factory.clone());
    enable_network_mcp_discovery(&mut language_server);

    open_document(&mut language_server, document_uri, 1, first_source);

    assert_eq!(discovery_gate.wait_until_started(), "local");

    change_document(&mut language_server, document_uri, 2, superseded_source);
    change_document(&mut language_server, document_uri, 3, latest_source);
    discovery_gate.release();

    assert!(
        language_server
            .receive_and_apply_mcp_discovery_result(Duration::from_secs(1))
            .is_none(),
        "version one discovery should be discarded"
    );
    assert_eq!(discovery_gate.wait_until_started(), "third");
    assert!(mcp_client_factory.delegate.requests("newest").is_empty());

    discovery_gate.release();

    let diagnostics_notification = language_server
        .receive_and_apply_mcp_discovery_result(Duration::from_secs(1))
        .expect("latest discovery should be accepted");
    let document_state = language_server
        .documents
        .get(document_uri)
        .expect("latest document should remain open");
    let latest_mcp_lock = document_state.mcp_lock().expect("latest MCP lock should apply");

    assert_eq!(diagnostics_notification.params["version"], 3);
    assert_eq!(document_state.version(), Some(3));
    assert_eq!(document_state.source_text(), latest_source);
    assert!(latest_mcp_lock.servers.contains_key("third"));
    assert!(!latest_mcp_lock.servers.contains_key("local"));
    assert!(!latest_mcp_lock.servers.contains_key("newest"));
}

#[test]
fn discovery_scheduler_bounds_unique_pending_documents() {
    let scheduler = super::McpDiscoveryScheduler::new(1);
    let source_text = Arc::<str>::from(workflow_source! {
        mcp local {
            endpoint: "mcp://local"
        }
    });
    let request_for_document = |document_uri: &str, request_id: u64| super::McpDiscoveryRequest {
        pending_discovery: super::PendingMcpDiscovery {
            request_id,
            document_version: 1,
            source_text: Arc::clone(&source_text),
            runtime_values: None,
        },
        document_uri: document_uri.to_string(),
        previous_mcp_lock: None,
    };
    let first_document_uri = "file:///tmp/first-queued.wire";
    let latest_document_uri = "file:///tmp/latest-queued.wire";

    let first_outcome = scheduler.schedule(request_for_document(first_document_uri, 1));
    let latest_outcome = scheduler.schedule(request_for_document(latest_document_uri, 2));
    let scheduler_state = scheduler.state.lock().expect("discovery scheduler lock should not be poisoned");

    assert!(first_outcome.accepted);
    assert!(latest_outcome.accepted);
    assert_eq!(latest_outcome.evicted_document_uri.as_deref(), Some(first_document_uri));
    assert_eq!(scheduler_state.pending_requests_by_document_uri.len(), 1);
    assert!(scheduler_state.pending_requests_by_document_uri.contains_key(latest_document_uri));
}

#[test]
fn slow_document_discovery_does_not_starve_other_documents() {
    let (mcp_client_factory, discovery_gate) = delayed_mcp_client_factory();
    let slow_source = workflow_source! {
        mcp local {
            endpoint: "mcp://local"
        }
    };
    let independent_source = workflow_source! {
        mcp newest {
            endpoint: "mcp://newest"
        }
    };
    let slow_document_uri = "file:///tmp/slow-mcp.wire";
    let independent_document_uri = "file:///tmp/independent-mcp.wire";
    let mut language_server = LanguageServer::with_mcp_client_factory(mcp_client_factory);
    enable_network_mcp_discovery(&mut language_server);

    open_document(&mut language_server, slow_document_uri, 1, slow_source);

    assert_eq!(discovery_gate.wait_until_started(), "local");

    open_document(&mut language_server, independent_document_uri, 1, independent_source);

    assert_eq!(
        discovery_gate.wait_until_started(),
        "newest",
        "an independent document should begin discovery before the slow document is released"
    );

    discovery_gate.release();
    discovery_gate.release();

    for _ in 0..2 {
        assert!(language_server
            .receive_and_apply_mcp_discovery_result(Duration::from_secs(1))
            .is_some());
    }

    assert!(language_server
        .documents
        .get(slow_document_uri)
        .and_then(DocumentState::mcp_lock)
        .is_some_and(|mcp_lock| mcp_lock.servers.contains_key("local")));
    assert!(language_server
        .documents
        .get(independent_document_uri)
        .and_then(DocumentState::mcp_lock)
        .is_some_and(|mcp_lock| mcp_lock.servers.contains_key("newest")));
}

#[test]
fn trust_downgrade_invalidates_in_flight_discovery_and_runtime_values() {
    let (mcp_client_factory, discovery_gate) = delayed_mcp_client_factory();
    let workflow_source = workflow_source! {
        mcp local {
            endpoint: "mcp://local"
        }
    };
    let document_uri = "file:///tmp/trust-downgrade-mcp.wire";
    let mut language_server = LanguageServer::with_mcp_client_factory(mcp_client_factory);

    initialize_with_network_mcp_discovery_trust(&mut language_server);
    open_document(&mut language_server, document_uri, 1, workflow_source);

    assert_eq!(discovery_gate.wait_until_started(), "local");

    send_runtime_values(
        &mut language_server,
        document_uri,
        Value::Null,
        serde_json::json!({ "authorization": "Bearer private-token" }),
    );
    let disabled_messages = initialize_with_network_mcp_discovery_value(&mut language_server, Value::String("disabled".to_string()));

    assert!(disabled_messages[0]["result"].is_object());
    assert_eq!(
        language_server.network_mcp_discovery_trust,
        super::NetworkMcpDiscoveryTrust::Disabled
    );
    assert!(language_server.pending_mcp_discoveries.is_empty());
    assert!(language_server.runtime_values_by_document_uri.is_empty());

    discovery_gate.release();

    assert!(language_server
        .receive_and_apply_mcp_discovery_result(Duration::from_secs(1))
        .is_none());
    assert!(language_server
        .documents
        .get(document_uri)
        .and_then(DocumentState::mcp_lock)
        .is_none());
}

#[test]
fn close_invalidates_in_flight_mcp_discovery() {
    let (mcp_client_factory, discovery_gate) = delayed_mcp_client_factory();
    let workflow_source = workflow_source! {
        mcp local {
            endpoint: "mcp://local"
        }
    };
    let document_uri = "file:///tmp/closed-mcp.wire";
    let mut language_server = LanguageServer::with_mcp_client_factory(mcp_client_factory);
    enable_network_mcp_discovery(&mut language_server);

    open_document(&mut language_server, document_uri, 1, workflow_source);

    assert_eq!(discovery_gate.wait_until_started(), "local");
    send_runtime_values(
        &mut language_server,
        document_uri,
        Value::Null,
        serde_json::json!({ "authorization": "Bearer queued-private-token" }),
    );

    let queued_scheduler_debug = {
        let scheduler_state = language_server
            .mcp_discovery_worker
            .scheduler
            .state
            .lock()
            .expect("discovery scheduler lock should not be poisoned");

        assert!(scheduler_state.pending_requests_by_document_uri.contains_key(document_uri));

        format!("{scheduler_state:?}")
    };

    assert!(!queued_scheduler_debug.contains("queued-private-token"));

    close_document(&mut language_server, document_uri);

    assert!(!language_server.documents.contains_key(document_uri));
    assert!(!language_server.pending_mcp_discoveries.contains_key(document_uri));
    {
        let scheduler_state = language_server
            .mcp_discovery_worker
            .scheduler
            .state
            .lock()
            .expect("discovery scheduler lock should not be poisoned");

        assert!(!scheduler_state.pending_requests_by_document_uri.contains_key(document_uri));
    }

    discovery_gate.release();

    assert!(language_server
        .receive_and_apply_mcp_discovery_result(Duration::from_secs(1))
        .is_none());
    assert!(!language_server.documents.contains_key(document_uri));
}

fn initialize_with_network_mcp_discovery_trust(language_server: &mut LanguageServer) -> Vec<Value> {
    initialize_with_network_mcp_discovery_value(language_server, Value::String("trusted".to_string()))
}

fn initialize_with_network_mcp_discovery_value(language_server: &mut LanguageServer, network_mcp_discovery_value: Value) -> Vec<Value> {
    language_server
        .handle_json_rpc_message(
            serde_json::json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "initialize",
                "params": {
                    "processId": null,
                    "capabilities": {},
                    "initializationOptions": {
                        "workspaceTrust": {
                            "networkMcpDiscovery": network_mcp_discovery_value
                        }
                    }
                }
            })
            .to_string()
            .as_bytes(),
        )
        .expect("initialize request should be accepted")
        .messages
}

fn send_runtime_values(language_server: &mut LanguageServer, document_uri: &str, input: Value, secrets: Value) -> Vec<Value> {
    language_server
        .handle_json_rpc_message(
            serde_json::json!({
                "jsonrpc": "2.0",
                "method": "superwire/runtimeValues",
                "params": {
                    "textDocument": { "uri": document_uri },
                    "input": input,
                    "secrets": secrets
                }
            })
            .to_string()
            .as_bytes(),
        )
        .expect("runtime values notification should be accepted")
        .messages
}

fn enable_network_mcp_discovery(language_server: &mut LanguageServer) {
    language_server.network_mcp_discovery_trust = super::NetworkMcpDiscoveryTrust::Trusted;
}

fn open_document(language_server: &mut LanguageServer, document_uri: &str, version: i32, source_text: &str) -> Vec<Value> {
    language_server
        .handle_json_rpc_message(
            serde_json::json!({
                "jsonrpc": "2.0",
                "method": "textDocument/didOpen",
                "params": {
                    "textDocument": {
                        "uri": document_uri,
                        "languageId": "wire",
                        "version": version,
                        "text": source_text
                    }
                }
            })
            .to_string()
            .as_bytes(),
        )
        .expect("didOpen should be accepted")
        .messages
}

fn change_document(language_server: &mut LanguageServer, document_uri: &str, version: i32, source_text: &str) -> Vec<Value> {
    language_server
        .handle_json_rpc_message(
            serde_json::json!({
                "jsonrpc": "2.0",
                "method": "textDocument/didChange",
                "params": {
                    "textDocument": {
                        "uri": document_uri,
                        "version": version
                    },
                    "contentChanges": [{
                        "text": source_text
                    }]
                }
            })
            .to_string()
            .as_bytes(),
        )
        .expect("didChange should be accepted")
        .messages
}

fn close_document(language_server: &mut LanguageServer, document_uri: &str) -> Vec<Value> {
    language_server
        .handle_json_rpc_message(
            serde_json::json!({
                "jsonrpc": "2.0",
                "method": "textDocument/didClose",
                "params": {
                    "textDocument": {
                        "uri": document_uri
                    }
                }
            })
            .to_string()
            .as_bytes(),
        )
        .expect("didClose should be accepted")
        .messages
}

fn completion_messages(language_server: &mut LanguageServer, document_uri: &str, position: Value) -> Vec<Value> {
    language_server
        .handle_json_rpc_message(
            serde_json::json!({
                "jsonrpc": "2.0",
                "id": 10,
                "method": "textDocument/completion",
                "params": {
                    "textDocument": {
                        "uri": document_uri
                    },
                    "position": position
                }
            })
            .to_string()
            .as_bytes(),
        )
        .expect("completion request should be accepted")
        .messages
}

fn completion_labels(messages: &[Value]) -> Vec<&str> {
    messages
        .iter()
        .find_map(|message| message.pointer("/result/items").and_then(Value::as_array))
        .expect("completion response should contain items")
        .iter()
        .filter_map(|completion_item| completion_item.get("label").and_then(Value::as_str))
        .collect()
}

fn source_and_cursor(source_template: &str) -> (String, Value) {
    let cursor_marker = "<cursor>";
    let cursor_offset = source_template
        .find(cursor_marker)
        .expect("source template should contain cursor marker");
    let source_before_cursor = &source_template[..cursor_offset];
    let cursor_line = source_before_cursor.lines().count().saturating_sub(1);
    let cursor_character = source_before_cursor
        .rsplit('\n')
        .next()
        .expect("source before cursor should contain a current line")
        .chars()
        .count();
    let source_text = source_template.replacen(cursor_marker, "", 1);

    (
        source_text,
        serde_json::json!({
            "line": cursor_line,
            "character": cursor_character
        }),
    )
}

#[derive(Debug)]
struct DelayedMcpClientFactory {
    delegate: FakeMcpClientFactory,
    started_sender: mpsc::Sender<String>,
    release_receiver: Arc<Mutex<mpsc::Receiver<()>>>,
}

impl McpClientFactory for DelayedMcpClientFactory {
    fn client_for_config(&self, server_config: McpServerConfig) -> Result<Arc<dyn McpClientBackend>, McpError> {
        let server_name = server_config.name.clone();
        let delegate = self.delegate.client_for_config(server_config)?;

        Ok(Arc::new(DelayedMcpClient {
            server_name,
            delegate,
            started_sender: self.started_sender.clone(),
            release_receiver: Arc::clone(&self.release_receiver),
        }))
    }
}

#[derive(Debug)]
struct DelayedMcpClient {
    server_name: String,
    delegate: Arc<dyn McpClientBackend>,
    started_sender: mpsc::Sender<String>,
    release_receiver: Arc<Mutex<mpsc::Receiver<()>>>,
}

impl McpClientBackend for DelayedMcpClient {
    fn list_tools(&self) -> Result<McpServerLock, McpError> {
        self.started_sender
            .send(self.server_name.clone())
            .expect("test should receive discovery start");
        self.release_receiver
            .lock()
            .expect("discovery release receiver lock poisoned")
            .recv_timeout(Duration::from_secs(5))
            .expect("test should release delayed discovery");

        self.delegate.list_tools()
    }

    fn call_tool(&self, tool_name: &str, arguments: Value) -> Result<Value, McpError> {
        self.delegate.call_tool(tool_name, arguments)
    }

    fn read_resource(&self, resource_name: &str, arguments: Value) -> Result<Value, McpError> {
        self.delegate.read_resource(resource_name, arguments)
    }

    fn get_prompt(&self, prompt_name: &str, arguments: Value) -> Result<Value, McpError> {
        self.delegate.get_prompt(prompt_name, arguments)
    }
}

#[derive(Debug)]
struct DiscoveryGate {
    started_receiver: mpsc::Receiver<String>,
    release_sender: mpsc::Sender<()>,
}

impl DiscoveryGate {
    fn wait_until_started(&self) -> String {
        self.started_receiver
            .recv_timeout(Duration::from_secs(1))
            .expect("MCP discovery should start")
    }

    fn release(&self) {
        self.release_sender.send(()).expect("MCP discovery should still be waiting");
    }
}

fn delayed_mcp_client_factory() -> (Arc<DelayedMcpClientFactory>, DiscoveryGate) {
    let (started_sender, started_receiver) = mpsc::channel();
    let (release_sender, release_receiver) = mpsc::channel();
    let delegate = fake_mcp_client_factory()
        .with_server("newest", newest_mcp_server)
        .with_server("third", standard_mcp_server);
    let client_factory = Arc::new(DelayedMcpClientFactory {
        delegate,
        started_sender,
        release_receiver: Arc::new(Mutex::new(release_receiver)),
    });
    let discovery_gate = DiscoveryGate {
        started_receiver,
        release_sender,
    };

    (client_factory, discovery_gate)
}

fn fake_mcp_client_factory() -> FakeMcpClientFactory {
    FakeMcpClientFactory::new().with_server("local", standard_mcp_server)
}

fn standard_mcp_server(server_builder: &mut FakeMcpServerBuilder) {
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

fn newest_mcp_server(server_builder: &mut FakeMcpServerBuilder) {
    server_builder.tool("newest-tool", |tool_builder| {
        tool_builder
            .description("Newest discovered tool")
            .input_schema(serde_json::json!({
                "type": "object",
                "properties": {}
            }))
            .output_schema(serde_json::json!({
                "type": "object",
                "properties": {
                    "accepted": { "type": "boolean" }
                },
                "required": ["accepted"]
            }));
    });
}

#[test]
fn plain_completion_text_preserves_braced_placeholder_defaults() {
    let snippet_text = "before.${1:name}.after $2";

    assert!(CompletionSuggestion::uses_snippet_format(snippet_text));
    assert_eq!(CompletionSuggestion::plain_insert_text(snippet_text), "before.name.after ");
}
