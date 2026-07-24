#[macro_use]
mod support;

use serde_json::{json, Value};
use support::fixtures;
use support::runner::{FakeMcpRequest, TestRunner};

#[tokio::test]
async fn scripts_provider_tool_calls_and_mcp_tool_responses() {
    let task_schema = schema! { title: String };
    let tool_output_schema = schema! { task_id: i64 };
    let output = TestRunner::workflow(fixtures::MCP_TOOL_BATCH_IMPORTS)
        .input(json!({ "project_id": 14, "task_id": 7 }))
        .provider("openai", |provider| {
            provider.api_key("test-api-key");
            provider.model("gpt-4.1-mini", |model| {
                model
                    .turn()
                    .expect_prompt("Manage project tasks")
                    .expect_tools(["assign_task", "create_sorting_task", "update_task_status"])
                    .expect_tool_with_schema("create_sorting_task", schema! { title: String })
                    .respond_tool_calls([call!("create_sorting_task", { "title": "first" })]);

                model
                    .turn()
                    .expect_prompt("Manage project tasks")
                    .expect_tools(["assign_task", "create_sorting_task", "update_task_status"])
                    .respond_json(json!({ "value": "created" }));
            });
        })
        .mcp("local", |mcp| {
            mcp.tool("create-sorting-task", |tool| {
                tool.input_schema(task_schema)
                    .output_schema(tool_output_schema)
                    .respond_json(json!({ "task_id": 100 }));
            });

            mcp.tool("update-task-status", |tool| {
                tool.input_schema(schema! { status: String })
                    .output_schema(schema! { success: bool });
            });

            mcp.tool("assign-task", |tool| {
                tool.input_schema(schema! { user_id: i64 }).output_schema(schema! { success: bool });
            });
        })
        .run()
        .await
        .expect("fixture runner should execute workflow with tools");

    assert_eq!(output.output, json!({ "value": { "value": "created" } }));
    assert_mcp_tool_was_called_with(
        &output.mcp_requests["local"],
        "create-sorting-task",
        json!({
            "project_id": 14,
            "task_id": 7,
            "title": "first",
        }),
    );
}

fn assert_mcp_tool_was_called_with(requests: &[FakeMcpRequest], tool_name: &str, expected_arguments: Value) {
    let tool_call = requests
        .iter()
        .find(|request| request.method == "tools/call" && request.name.as_deref() == Some(tool_name))
        .expect("expected MCP tool call request");

    assert_eq!(tool_call.arguments, expected_arguments);
}

#[tokio::test]
async fn sends_model_tool_call_argument_errors_back_to_model() {
    let task_schema = schema! { title: String };

    let output = TestRunner::workflow(fixtures::MCP_TOOL_BATCH_IMPORTS)
        .input(json!({ "project_id": 14, "task_id": 7 }))
        .provider("openai", |provider| {
            provider.api_key("test-api-key");
            provider.model("gpt-4.1-mini", |model| {
                model
                    .turn()
                    .expect_prompt("Manage project tasks")
                    .expect_tools(["assign_task", "create_sorting_task", "update_task_status"])
                    .expect_tool_with_schema("create_sorting_task", schema! { title: String })
                    .respond_tool_calls([call!("create_sorting_task", { "title": 123 })]);

                model.turn().respond_json(json!({ "value": "recovered" }));
            });
        })
        .mcp("local", |mcp| {
            mcp.tool("create-sorting-task", |tool| {
                tool.input_schema(task_schema).output_schema(schema! { task_id: i64 });
            });

            mcp.tool("update-task-status", |tool| {
                tool.input_schema(schema! { status: String })
                    .output_schema(schema! { success: bool });
            });

            mcp.tool("assign-task", |tool| {
                tool.input_schema(schema! { user_id: i64 }).output_schema(schema! { success: bool });
            });
        })
        .run()
        .await
        .expect("execution should let model recover from invalid tool arguments");

    assert_eq!(output.output, json!({ "value": { "value": "recovered" } }));
}

#[tokio::test]
async fn fails_after_model_repeats_same_tool_call_too_many_times() {
    let task_schema = schema! { title: String };
    let tool_output_schema = schema! { task_id: i64 };
    let execution_error = TestRunner::workflow(fixtures::MCP_TOOL_BATCH_IMPORTS)
        .input(json!({ "project_id": 14, "task_id": 7 }))
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
                tool.input_schema(schema! { status: String })
                    .output_schema(schema! { success: bool });
            });

            mcp.tool("assign-task", |tool| {
                tool.input_schema(schema! { user_id: i64 }).output_schema(schema! { success: bool });
            });
        })
        .run()
        .await
        .expect_err("execution should fail when model only emits tool calls");

    let error_message = execution_error.to_string();

    assert!(
        error_message.contains("tool") || error_message.contains("finalize"),
        "{error_message}"
    );
}

#[tokio::test]
async fn sends_error_back_when_model_receives_incorrect_tool_schema() {
    let output = TestRunner::workflow(fixtures::MCP_TOOL_BATCH_IMPORTS)
        .input(json!({ "project_id": 14, "task_id": 7 }))
        .provider("openai", |provider| {
            provider.api_key("test-api-key");
            provider.model("gpt-4.1-mini", |model| {
                model
                    .turn()
                    .expect_prompt("Manage project tasks")
                    .expect_tool_with_schema("create_sorting_task", schema! { title: i64 })
                    .respond_tool_calls([call!("create_sorting_task", { "title": "not a number" })]);

                model.turn().respond_json(json!({ "value": "recovered from bad schema" }));
            });
        })
        .mcp("local", |mcp| {
            mcp.tool("create-sorting-task", |tool| {
                tool.input_schema(schema! { title: i64 }).output_schema(schema! { task_id: i64 });
            });

            mcp.tool("update-task-status", |tool| {
                tool.input_schema(schema! { status: String })
                    .output_schema(schema! { success: bool });
            });

            mcp.tool("assign-task", |tool| {
                tool.input_schema(schema! { user_id: i64 }).output_schema(schema! { success: bool });
            });
        })
        .run()
        .await
        .expect("execution should let model recover from incorrect tool schema");

    assert_eq!(output.output, json!({ "value": { "value": "recovered from bad schema" } }));
}

#[tokio::test]
async fn sends_incorrect_mcp_tool_output_back_to_model() {
    let task_schema = schema! { title: String };
    let output = TestRunner::workflow(fixtures::MCP_TOOL_BATCH_IMPORTS)
        .input(json!({ "project_id": 14, "task_id": 7 }))
        .provider("openai", |provider| {
            provider.api_key("test-api-key");
            provider.model("gpt-4.1-mini", |model| {
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
                    .respond_json(json!({ "value": "recovered from bad tool output" }));
            });
        })
        .mcp("local", |mcp| {
            mcp.tool("create-sorting-task", |tool| {
                tool.input_schema(task_schema)
                    .output_schema(schema! { task_id: i64 })
                    .respond_json(json!({ "task_id": "wrong" }));
            });

            mcp.tool("update-task-status", |tool| {
                tool.input_schema(schema! { status: String })
                    .output_schema(schema! { success: bool });
            });

            mcp.tool("assign-task", |tool| {
                tool.input_schema(schema! { user_id: i64 }).output_schema(schema! { success: bool });
            });
        })
        .run()
        .await
        .expect("execution should let model recover from incorrect MCP output");

    assert_eq!(output.output, json!({ "value": { "value": "recovered from bad tool output" } }));
}
