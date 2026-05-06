#[macro_use]
mod support;

use serde_json::json;
use support::fixtures;
use support::runner::TestRunner;

#[tokio::test]
async fn reads_mcp_resource_as_dynamic_value() {
    let output = TestRunner::workflow(fixtures::MCP_READ_RESOURCE)
        .input(json!({ "workspace_id": "workspace-1" }))
        .mcp("local", |mcp| {
            mcp.resource("project-readme", |resource| {
                resource
                    .uri("file://resources/project-readme")
                    .mime_type("text/markdown")
                    .text("# Project README\nUse stable sorting.");
            });
        })
        .run()
        .await
        .expect("fixture runner should execute MCP resource read workflow");

    assert!(output.output["readme"]
        .as_str()
        .is_some_and(|readme| readme.contains("# Project README")));
}
