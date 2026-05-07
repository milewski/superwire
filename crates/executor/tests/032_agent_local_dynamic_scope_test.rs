#[macro_use]
mod support;

use serde_json::{json, Value};
use support::fixtures;
use support::runner::TestRunner;

#[tokio::test]
async fn agent_local_dynamic_tool_call_waits_for_agent_dependency() {
    let output = TestRunner::workflow(fixtures::AGENT_LOCAL_DYNAMIC_TOOL_SCOPE)
        .provider("openai", |provider| {
            provider.api_key("test-api-key");
            provider.model("model-a", |model| {
                model.turn().expect_prompt("Prepare dependency value").respond_string("ready");

                model
                    .turn()
                    .expect_prompt("Use dependency from agent_a")
                    .respond_string("done");
            });
        })
        .mcp("local", |mcp| {
            mcp.tool("fetch_task_data", |tool| {
                tool.input_schema(schema! { project_id: i64, depends_on: String })
                    .output_schema(schema! { task_title: String, participants: i64 })
                    .respond_json(json!({ "task_title": "Survey", "participants": 10 }));
            });
        })
        .run()
        .await
        .expect("agent-local dynamic tool call should wait for dependency");

    let tool_call_arguments = find_mcp_tool_call_arguments(&output.mcp_requests["local"], "fetch_task_data");

    assert_eq!(tool_call_arguments, json!({ "project_id": 7, "depends_on": "ready" }));
    assert_eq!(output.output, json!({ "first": "ready", "second": "done" }));
}

fn find_mcp_tool_call_arguments(requests: &[Value], tool_name: &str) -> Value {
    let tool_call_request = requests
        .iter()
        .find(|request| {
            request.get("method") == Some(&json!("tools/call")) && request.pointer("/params/name") == Some(&json!(tool_name))
        })
        .expect("expected MCP tools/call request");

    tool_call_request
        .pointer("/params/arguments")
        .cloned()
        .expect("MCP tools/call should include params.arguments")
}
