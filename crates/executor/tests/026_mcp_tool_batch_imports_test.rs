#[macro_use]
mod support;

use serde_json::{json, Value};
use support::fixtures;
use support::runner::TestRunner;

#[tokio::test]
async fn scripts_provider_tool_calls_and_mcp_tool_responses() {
    let task_schema = schema!({ title: string });
    let tool_output_schema = schema!({ task_id: number });
    let run_output = TestRunner::workflow(fixtures::MCP_TOOL_BATCH_IMPORTS)
        .input(input!({ "project_id": 14, "task_id": 7 }))
        .provider("openai", |provider| {
            provider.api_key("test-api-key").model("gpt-4.1-mini", |model| {
                model
                    .turn()
                    .expect_prompt("Manage project tasks")
                    .expect_tools(["assign_task", "create_sorting_task", "update_task_status"])
                    .expect_tool_with_schema("create_sorting_task", task_schema.clone())
                    .respond_tool_calls([call!("create_sorting_task", { "title": "first" })]);

                model
                    .turn()
                    .with_messages(|messages| {
                        let tool_message_count = messages
                            .iter()
                            .filter(|message| message.get("role") == Some(&json!("tool")))
                            .count();

                        assert_eq!(tool_message_count, 1);
                    })
                    .expect_prompt("Manage project tasks")
                    .expect_tools(["assign_task", "create_sorting_task", "update_task_status"])
                    .respond_json(json!("created"));
            });
        })
        .mcp("local", |mcp| {
            mcp.tool("create-sorting-task", |tool| {
                tool.input_schema(task_schema)
                    .output_schema(tool_output_schema)
                    .respond_json(json!({ "task_id": 100 }));
            });

            mcp.tool("update-task-status", |tool| {
                tool.input_schema(schema!({ status: string }))
                    .output_schema(schema!({ success: boolean }));
            });

            mcp.tool("assign-task", |tool| {
                tool.input_schema(schema!({ user_id: number }))
                    .output_schema(schema!({ success: boolean }));
            });
        })
        .run()
        .await
        .expect("fixture runner should execute workflow with tools");

    assert_eq!(run_output.output, json!({ "value": "created" }));
    assert_mcp_tool_was_called_with(
        &run_output.mcp_requests["local"],
        "create-sorting-task",
        json!({
            "project_id": 14,
            "task_id": 7,
            "title": "first",
        }),
    );
}

fn assert_mcp_tool_was_called_with(requests: &[Value], tool_name: &str, expected_arguments: Value) {
    let tool_call = requests
        .iter()
        .find(|request| request.get("method") == Some(&json!("tools/call")) && request.pointer("/params/name") == Some(&json!(tool_name)))
        .expect("expected MCP tool call request");

    assert_eq!(tool_call.pointer("/params/arguments"), Some(&expected_arguments));
}

#[tokio::test]
async fn sends_model_tool_call_argument_errors_back_to_model() {
    let task_schema = schema!({ title: string });
    let run_output = TestRunner::workflow(fixtures::MCP_TOOL_BATCH_IMPORTS)
        .input(input!({ "project_id": 14, "task_id": 7 }))
        .provider("openai", |provider| {
            provider.api_key("test-api-key").model("gpt-4.1-mini", |model| {
                model
                    .turn()
                    .expect_prompt("Manage project tasks")
                    .expect_tools(["assign_task", "create_sorting_task", "update_task_status"])
                    .expect_tool_with_schema("create_sorting_task", task_schema.clone())
                    .respond_tool_calls([call!("create_sorting_task", { "title": 123 })]);

                model
                    .turn()
                    .with_messages(|messages| {
                        let tool_message = messages
                            .iter()
                            .find(|message| message.get("role") == Some(&json!("tool")))
                            .expect("tool error message should be replayed to model");
                        let tool_content = tool_message.get("content").and_then(Value::as_str).unwrap_or_default();

                        assert!(tool_content.contains("schema"), "{tool_content}");
                    })
                    .respond_json(json!("recovered"));
            });
        })
        .mcp("local", |mcp| {
            mcp.tool("create-sorting-task", |tool| {
                tool.input_schema(task_schema).output_schema(schema!({ task_id: number }));
            });

            mcp.tool("update-task-status", |tool| {
                tool.input_schema(schema!({ status: string }))
                    .output_schema(schema!({ success: boolean }));
            });

            mcp.tool("assign-task", |tool| {
                tool.input_schema(schema!({ user_id: number }))
                    .output_schema(schema!({ success: boolean }));
            });
        })
        .run()
        .await
        .expect("execution should let model recover from invalid tool arguments");

    assert_eq!(run_output.output, json!({ "value": "recovered" }));
}

#[tokio::test]
async fn fails_after_model_repeats_same_tool_call_too_many_times() {
    let task_schema = schema!({ title: string });
    let tool_output_schema = schema!({ task_id: number });
    let execution_error = TestRunner::workflow(fixtures::MCP_TOOL_BATCH_IMPORTS)
        .input(input!({ "project_id": 14, "task_id": 7 }))
        .provider("openai", |provider| {
            provider.api_key("test-api-key").model("gpt-4.1-mini", |model| {
                for _turn_index in 0..24 {
                    model
                        .turn()
                        .expect_prompt("Manage project tasks")
                        .expect_tools(["assign_task", "create_sorting_task", "update_task_status"])
                        .respond_tool_calls([call!("create_sorting_task", { "title": "repeat" })]);
                }
            });
        })
        .mcp("local", |mcp| {
            mcp.tool("create-sorting-task", |tool| {
                tool.input_schema(task_schema).output_schema(tool_output_schema);

                for response_index in 0..24 {
                    tool.respond_json(json!({ "task_id": response_index + 1 }));
                }
            });

            mcp.tool("update-task-status", |tool| {
                tool.input_schema(schema!({ status: string }))
                    .output_schema(schema!({ success: boolean }));
            });

            mcp.tool("assign-task", |tool| {
                tool.input_schema(schema!({ user_id: number }))
                    .output_schema(schema!({ success: boolean }));
            });
        })
        .run()
        .await
        .expect_err("execution should fail when model only emits tool calls");

    let error_message = execution_error.to_string();

    assert!(
        error_message.contains("tool") || error_message.contains("valid JSON output"),
        "{error_message}"
    );
}

#[tokio::test]
async fn sends_error_back_when_model_receives_incorrect_tool_schema() {
    let run_output = TestRunner::workflow(fixtures::MCP_TOOL_BATCH_IMPORTS)
        .input(input!({ "project_id": 14, "task_id": 7 }))
        .provider("openai", |provider| {
            provider.api_key("test-api-key").model("gpt-4.1-mini", |model| {
                model
                    .turn()
                    .expect_prompt("Manage project tasks")
                    .expect_tool_with_schema("create_sorting_task", schema!({ title: number }))
                    .respond_tool_calls([call!("create_sorting_task", { "title": "not a number" })]);

                model
                    .turn()
                    .with_messages(|messages| {
                        let tool_message = messages
                            .iter()
                            .find(|message| message.get("role") == Some(&json!("tool")))
                            .expect("tool schema error should be replayed to model");
                        let tool_content = tool_message.get("content").and_then(Value::as_str).unwrap_or_default();

                        assert!(tool_content.contains("tool_argument_schema_mismatch"), "{tool_content}");
                    })
                    .respond_json(json!("recovered from bad schema"));
            });
        })
        .mcp("local", |mcp| {
            mcp.tool("create-sorting-task", |tool| {
                tool.input_schema(schema!({ title: number }))
                    .output_schema(schema!({ task_id: number }));
            });

            mcp.tool("update-task-status", |tool| {
                tool.input_schema(schema!({ status: string }))
                    .output_schema(schema!({ success: boolean }));
            });

            mcp.tool("assign-task", |tool| {
                tool.input_schema(schema!({ user_id: number }))
                    .output_schema(schema!({ success: boolean }));
            });
        })
        .run()
        .await
        .expect("execution should let model recover from incorrect tool schema");

    assert_eq!(run_output.output, json!({ "value": "recovered from bad schema" }));
}

#[tokio::test]
async fn sends_incorrect_mcp_tool_output_back_to_model() {
    let task_schema = schema!({ title: string });
    let run_output = TestRunner::workflow(fixtures::MCP_TOOL_BATCH_IMPORTS)
        .input(input!({ "project_id": 14, "task_id": 7 }))
        .provider("openai", |provider| {
            provider.api_key("test-api-key").model("gpt-4.1-mini", |model| {
                model
                    .turn()
                    .expect_prompt("Manage project tasks")
                    .expect_tools(["assign_task", "create_sorting_task", "update_task_status"])
                    .respond_tool_calls([call!("create_sorting_task", { "title": "first" })]);

                model
                    .turn()
                    .with_messages(|messages| {
                        let tool_message = messages
                            .iter()
                            .find(|message| message.get("role") == Some(&json!("tool")))
                            .expect("incorrect MCP output should be replayed to model");
                        let tool_content = tool_message.get("content").and_then(Value::as_str).unwrap_or_default();

                        assert!(tool_content.contains("wrong"), "{tool_content}");
                    })
                    .respond_json(json!("recovered from bad tool output"));
            });
        })
        .mcp("local", |mcp| {
            mcp.tool("create-sorting-task", |tool| {
                tool.input_schema(task_schema)
                    .output_schema(schema!({ task_id: number }))
                    .respond_json(json!({ "task_id": "wrong" }));
            });

            mcp.tool("update-task-status", |tool| {
                tool.input_schema(schema!({ status: string }))
                    .output_schema(schema!({ success: boolean }));
            });

            mcp.tool("assign-task", |tool| {
                tool.input_schema(schema!({ user_id: number }))
                    .output_schema(schema!({ success: boolean }));
            });
        })
        .run()
        .await
        .expect("execution should let model recover from incorrect MCP output");

    assert_eq!(run_output.output, json!({ "value": "recovered from bad tool output" }));
}
