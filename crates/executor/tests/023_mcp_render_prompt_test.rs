#[macro_use]
mod support;

use serde_json::json;
use support::fixtures;
use support::runner::TestRunner;

#[tokio::test]
async fn renders_mcp_prompt_as_dynamic_value() {
    let output = TestRunner::workflow(fixtures::MCP_RENDER_PROMPT)
        .input(json!({ "workspace_id": "workspace-1" }))
        .mcp("local", |mcp| {
            mcp.prompt("system-prompt", |prompt| {
                prompt.description("System prompt").text("Follow project conventions.");
            });
        })
        .run()
        .await
        .expect("fixture runner should execute MCP prompt render workflow");

    assert!(output.output["instructions"]
        .as_str()
        .is_some_and(|instructions| instructions.contains("Follow project conventions.")));
}
