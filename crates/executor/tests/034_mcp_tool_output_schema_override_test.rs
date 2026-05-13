#[macro_use]
mod support;

use serde_json::json;
use support::fixtures;
use support::runner::TestRunner;

#[tokio::test]
async fn uses_wire_output_schema_when_mcp_tool_output_schema_is_not_defined() {
    let output = TestRunner::workflow(fixtures::MCP_TOOL_OUTPUT_SCHEMA_OVERRIDE)
        .mcp("local", |mcp| {
            mcp.tool("fetch_numbers", |tool| {
                tool.input_schema(schema! {}).respond_json(json!({ "values": [1, 2, 3] }));
            });
        })
        .provider("openai", |provider| {
            provider.api_key("test-api-key");
            provider.model("model-a", |model| {
                model.turn().expect_prompt("Write a note for 1.").respond_json(json!({ "value": "one" }));
                model.turn().expect_prompt("Write a note for 2.").respond_json(json!({ "value": "two" }));
                model.turn().expect_prompt("Write a note for 3.").respond_json(json!({ "value": "three" }));
            });
        })
        .run()
        .await
        .expect("wire-defined output schema should be used when MCP output schema is omitted");

    assert_eq!(output.output, json!({ "notes": [{ "value": "one" }, { "value": "two" }, { "value": "three" }] }));
}
