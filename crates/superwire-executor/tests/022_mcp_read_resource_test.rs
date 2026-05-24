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
                    .uri("file://resources/project_readme")
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

    let read_resource_request = output.mcp_requests["local"]
        .iter()
        .find(|request| request.method == "resources/read")
        .expect("resources/read request should be present");

    assert_eq!(read_resource_request.name.as_deref(), Some("project-readme"));
    assert_eq!(
        read_resource_request.arguments,
        json!({
            "workspace_id": "workspace-1",
            "section": "setup"
        })
    );
}
