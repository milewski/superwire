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

    let read_resource_request = output.mcp_requests["local"]
        .iter()
        .find(|request| request.get("method") == Some(&json!("resources/read")))
        .expect("resources/read request should be present");

    assert_eq!(
        read_resource_request.pointer("/params"),
        Some(&json!({
            "uri": "file://resources/project-readme"
        }))
    );

    let render_prompt_request = output.mcp_requests["local"]
        .iter()
        .find(|request| request.get("method") == Some(&json!("prompts/get")))
        .expect("prompts/get request should be present");

    assert_eq!(
        render_prompt_request.pointer("/params"),
        Some(&json!({
            "name": "system-prompt",
            "arguments": {
                "readme": "# Project README\nUse stable sorting."
            }
        }))
    );
}
