#[macro_use]
mod support;

use serde_json::{json, Value};
use support::fixtures;
use support::runner::TestRunner;

#[tokio::test]
async fn agent_local_dynamic_tool_call_overrides_and_appends_bindings() {
    let output = TestRunner::workflow(fixtures::AGENT_LOCAL_DYNAMIC_BINDINGS_OVERRIDE_APPEND)
        .provider("openai", |provider| {
            provider.api_key("test-api-key");
            provider.model("model-a", |model| {
                model.turn().expect_prompt("Process fetched values").respond_json(json!({ "value": "ok" }));
            });
        })
        .mcp("local", |mcp| {
            mcp.tool("fetch_numbers", |tool| {
                tool.input_schema(schema! { r#override: i64, base: String, append: String })
                    .output_schema(schema! { values: Vec<u64> })
                    .respond_json(json!({ "values": [1, 2, 3] }));
            });
        })
        .run()
        .await
        .expect("agent-local dynamic bindings should override and append values");

    let tool_call_arguments = find_mcp_tool_call_arguments(&output.mcp_requests["local"], "fetch_numbers");

    assert_eq!(tool_call_arguments, json!({ "override": 123, "base": "keep", "append": "xyz" }));
    assert_eq!(output.output, json!({ "result": { "value": "ok" } }));
}

fn find_mcp_tool_call_arguments(requests: &[Value], tool_name: &str) -> Value {
    let tool_call_request = requests
        .iter()
        .find(|request| request.get("method") == Some(&json!("tools/call")) && request.pointer("/params/name") == Some(&json!(tool_name)))
        .expect("expected MCP tools/call request");

    tool_call_request
        .pointer("/params/arguments")
        .cloned()
        .expect("MCP tools/call should include params.arguments")
}
