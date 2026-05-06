#[macro_use]
mod support;

use serde_json::json;
use support::fixtures;
use support::runner::TestRunner;

#[tokio::test]
async fn exposes_agent_scoped_tool_call_limits() {
    let run_output = TestRunner::workflow(fixtures::TOOL_MAX_CALLS_SCOPES)
        .provider("openai", |provider| {
            provider.api_key("test-api-key").model("model-a", |model| {
                model
                    .turn()
                    .expect_prompt("First")
                    .expect_tools(["fetch_shared"])
                    .respond_string("first");

                model
                    .turn()
                    .expect_prompt("Second")
                    .expect_tools(["fetch_shared"])
                    .respond_string("second");
            });
        })
        .mcp("local", |mcp| {
            mcp.tool("fetch_task_data", |tool| {
                tool.input_schema(schema!({ project_id: number, task_id: number }))
                    .output_schema(schema!({ task_title: string, participants: number }));
            });
        })
        .run()
        .await
        .expect("fixture runner should execute tool max calls scopes workflow");

    assert_eq!(run_output.output, json!({ "first": "first", "second": "second" }));
}
