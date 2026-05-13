#[macro_use]
mod support;

use serde_json::json;
use support::fixtures;
use support::runner::TestRunner;

#[tokio::test]
async fn exposes_agent_scoped_tool_call_limits() {
    let output = TestRunner::workflow(fixtures::TOOL_MAX_CALLS_SCOPES)
        .provider("openai", |provider| {
            provider.api_key("test-api-key");
            provider.model("model-a", |model| {
                model
                    .turn()
                    .expect_prompt("First")
                    .expect_tools(["fetch_shared"])
                    .respond_json(json!({ "value": "first" }));

                model
                    .turn()
                    .expect_prompt("Second")
                    .expect_tools(["fetch_shared"])
                    .respond_json(json!({ "value": "second" }));
            });
        })
        .mcp("local", |mcp| {
            mcp.tool("fetch_task_data", |tool| {
                tool.input_schema(schema! { project_id: i64, task_id: i64 })
                    .output_schema(schema! { task_title: String, participants: i64 });
            });
        })
        .run()
        .await
        .expect("fixture runner should execute tool max calls scopes workflow");

    assert_eq!(output.output, json!({ "first": { "value": "first" }, "second": { "value": "second" } }));
}
