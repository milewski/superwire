#[macro_use]
mod support;

use serde_json::json;
use support::fixtures;
use support::runner::TestRunner;

#[tokio::test]
async fn resolves_mcp_read_render_dependencies() {
    let output = TestRunner::workflow(fixtures::MCP_READ_RENDER_DEPENDENCIES)
        .input(json!({ "workspace_id": "workspace-1" }))
        .mcp("local", |mcp| {
            mcp.resource("project-readme", |resource| {
                resource
                    .uri("file://resources/project-readme")
                    .mime_type("text/markdown")
                    .text("# Project README\nUse stable sorting.");
            });

            mcp.prompt("system-prompt", |prompt| {
                prompt.description("System prompt").text("Follow project conventions.");
            });
        })
        .run()
        .await
        .expect("fixture runner should execute MCP read/render dependency workflow");

    assert!(output.output["readme"]
        .as_str()
        .is_some_and(|readme| readme.contains("# Project README")));

    assert!(output.output["instructions"]
        .as_str()
        .is_some_and(|instructions| instructions.contains("Follow project conventions.")));
}
