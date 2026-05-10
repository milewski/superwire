#[macro_use]
mod support;

use serde_json::{json, Value};
use support::fixtures;
use support::runner::TestRunner;

#[tokio::test]
async fn batches_mixed_mcp_imports_with_bindings() {
    let output = TestRunner::workflow(fixtures::MCP_MIXED_BATCH_IMPORTS)
        .input(json!({ "project_id": 14, "task_id": 7 }))
        .provider("openai", |provider| {
            provider.api_key("test-api-key");
            provider.model("gpt-4.1-mini", |model| {
                model
                    .turn()
                    .expect_prompt("Create a task")
                    .expect_tools(["create_task", "read_all_tasks", "render_create_task_instructions"])
                    .respond_tool_calls([
                        call!("render_create_task_instructions", { "audience": "maintainers" }),
                        call!("read_all_tasks", {}),
                    ]);

                model
                    .turn()
                    .expect_prompt("Create a task")
                    .expect_tools(["create_task", "read_all_tasks", "render_create_task_instructions"])
                    .respond_tool_calls([call!("create_task", { "title": "first" })]);

                model
                    .turn()
                    .expect_prompt("Create a task")
                    .expect_tools(["create_task", "read_all_tasks", "render_create_task_instructions"])
                    .respond_json(json!("created"));
            });
        })
        .mcp("local", |mcp| {
            mcp.prompt("create-task-instructions", |prompt| {
                prompt
                    .description("Task instructions")
                    .text("Create the requested task.")
                    .argument("project_id", true)
                    .argument("audience", false);
            });

            mcp.resource("all-tasks", |resource| {
                resource
                    .uri("file://resources/all_tasks")
                    .mime_type("text/markdown")
                    .text("# Tasks");
            });

            mcp.tool("create-task", |tool| {
                tool.input_schema(schema! { title: String })
                    .output_schema(schema! { task_id: i64 })
                    .respond_json(json!({ "task_id": 100 }));
            });
        })
        .run()
        .await
        .expect("fixture runner should execute mixed MCP batch imports");

    assert_eq!(output.output, json!({ "value": "created" }));

    let tool_call = output.mcp_requests["local"]
        .iter()
        .find(|request| request.get("method") == Some(&json!("tools/call")))
        .expect("tools/call request should be present");

    assert_eq!(tool_call.pointer("/params/name"), Some(&json!("create-task")));
    assert_eq!(
        tool_call.pointer("/params/arguments"),
        Some(&json!({
            "project_id": 14,
            "task_id": 7,
            "title": "first"
        }))
    );

    let prompt_request = output.mcp_requests["local"]
        .iter()
        .find(|request| request.get("method") == Some(&json!("prompts/get")))
        .expect("prompts/get request should be present");

    assert_eq!(prompt_request.pointer("/params/name"), Some(&json!("create-task-instructions")));
    assert_eq!(
        prompt_request.pointer("/params/arguments"),
        Some(&json!({
            "project_id": "14"
        }))
    );

    assert_mcp_request_was_sent(&output.mcp_requests["local"], "resources/read");
}

fn assert_mcp_request_was_sent(requests: &[Value], method: &str) {
    assert!(
        requests.iter().any(|request| request.get("method") == Some(&json!(method))),
        "expected MCP request `{method}`"
    );
}
