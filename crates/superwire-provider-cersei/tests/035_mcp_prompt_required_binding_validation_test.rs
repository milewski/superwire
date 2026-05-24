#[macro_use]
mod support;

use serde_json::json;
use support::fixtures;
use support::runner::TestRunner;

#[tokio::test]
async fn rejects_prompt_import_missing_required_mcp_argument_binding() {
    let output = TestRunner::workflow(fixtures::MCP_PROMPT_REQUIRED_BINDING_VALIDATION)
        .input(json!({ "project_id": 14, "task_id": 7 }))
        .mcp("local", |mcp| {
            mcp.prompt("summarize-task-prompt", |prompt| {
                prompt
                    .description("Task summary prompt")
                    .text("Summarize the requested task.")
                    .argument("project_id", true)
                    .argument("type_id", true)
                    .argument("type", true);
            });
        })
        .run_expect_error()
        .await;

    let error_message = output.error.to_string();

    assert!(error_message.contains("MCP prompt `summarize_task_prompt` requires binding `type`"));
    assert!(!output.mcp_requests["local"].iter().any(|request| request.method == "prompts/get"));
}
