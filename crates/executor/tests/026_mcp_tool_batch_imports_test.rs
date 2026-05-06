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
