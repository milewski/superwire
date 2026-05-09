#[macro_use]
mod support;

use serde_json::json;
use support::fixtures;
use support::runner::TestRunner;

#[tokio::test]
async fn batches_prompt_and_resource_imports_with_bindings() {
    let output = TestRunner::workflow(fixtures::MCP_PROMPT_RESOURCE_BATCH_IMPORTS)
        .input(json!({ "project_id": 14, "task_id": 7 }))
        .mcp("local", |mcp| {
            mcp.prompt("create_task_instructions", |prompt| {
                prompt.description("Task instructions").text("Create the requested task.");
            });

            mcp.resource("all_tasks", |resource| {
                resource
                    .uri("file://resources/all_tasks")
                    .mime_type("text/markdown")
                    .text("# Tasks");
            });
        })
        .run()
        .await
        .expect("fixture runner should execute prompt and resource batch imports");

    assert!(output.output["instructions"]
        .as_str()
        .is_some_and(|instructions| instructions.contains("Create the requested task.")));

    assert!(output.output["tasks"].as_str().is_some_and(|tasks| tasks.contains("# Tasks")));

    let expected_prompt_parameters = json!({
            "name": "create_task_instructions",
            "arguments": {
                "audience": "maintainers"
            }
    });

    assert!(
        output.mcp_requests["local"].iter().any(|request| {
            request.get("method") == Some(&json!("prompts/get")) && request.pointer("/params") == Some(&expected_prompt_parameters)
        }),
        "prompts/get request with batch bindings should be present: {:?}",
        output.mcp_requests["local"]
    );

    let read_resource_request = output.mcp_requests["local"]
        .iter()
        .find(|request| request.get("method") == Some(&json!("resources/read")))
        .expect("resources/read request should be present");

    assert_eq!(
        read_resource_request.pointer("/params"),
        Some(&json!({
            "uri": "file://resources/all_tasks"
        }))
    );
}
