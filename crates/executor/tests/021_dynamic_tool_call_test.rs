#[macro_use]
mod support;

use serde_json::json;
use support::fixtures;
use support::runner::TestRunner;

#[tokio::test]
async fn executes_dynamic_tool_call_before_agent() {
    let tool_response = json!({ "task_title": "Survey", "participants": 10 });
    let run_output = TestRunner::workflow(fixtures::DYNAMIC_TOOL_CALL)
        .input(input!({ "project_id": 42, "task_id": 7 }))
        .provider("openai", |provider| {
            provider.api_key("test-api-key").model("model-a", |model| {
                model
                    .turn()
                    .expect_prompt("Summarize:")
                    .expect_prompt("Survey")
                    .respond_json(json!({ "summary": "done" }));
            });
        })
        .mcp("local", |mcp| {
            mcp.tool("fetch_task_data", |tool| {
                tool.input_schema(schema!({ project_id: number, task_id: number }))
                    .output_schema(schema!({ task_title: string, participants: number }))
                    .respond_json(tool_response.clone());
            });
        })
        .run()
        .await
        .expect("fixture runner should execute dynamic tool call workflow");

    assert_eq!(run_output.output, json!({ "data": tool_response, "summary": "done" }));
}
