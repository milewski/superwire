#[macro_use]
mod support;

use serde_json::json;
use support::fixtures;
use support::runner::TestRunner;

#[tokio::test]
async fn executes_dynamic_tool_call_before_agent() {
    let tool_response = json!({ "task_title": "Survey", "participants": 10 });
    let output = TestRunner::workflow(fixtures::DYNAMIC_TOOL_CALL)
        .input(json!({ "project_id": 42, "task_id": 7 }))
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
                tool.input_schema(schema! { project_id: i64, task_id: i64 })
                    .output_schema(schema! { task_title: String, participants: i64 })
                    .respond_json(tool_response.clone());
            });
        })
        .run()
        .await
        .expect("fixture runner should execute dynamic tool call workflow");

    assert_eq!(output.output, json!({ "data": tool_response, "summary": "done" }));
}

#[tokio::test]
async fn fails_when_mcp_tool_output_does_not_match_schema() {
    let verbose_tool_value = "too-long-to-repeat-".repeat(32);
    let execution_error = TestRunner::workflow(fixtures::DYNAMIC_TOOL_CALL)
        .input(json!({ "project_id": 42, "task_id": 7 }))
        .provider("openai", |provider| {
            provider.api_key("test-api-key").model("model-a", |model| {
                model.turn().expect_prompt("Summarize:").respond_json(json!({ "summary": "done" }));
            });
        })
        .mcp("local", |mcp_builder| {
            mcp_builder.tool("fetch_task_data", |tool_builder| {
                tool_builder
                    .input_schema(schema! { project_id: i64, task_id: i64 })
                    .output_schema(schema! { task_title: String, participants: i64 })
                    .respond_json(json!({ "task_title": "Survey", "participants": verbose_tool_value }));
            });
        })
        .run()
        .await
        .expect_err("execution should fail when MCP tool output violates its schema");

    let error_message = execution_error.to_string();

    assert!(error_message.contains("$.data.participants"), "{error_message}");
    assert!(error_message.contains("value is not of type"), "{error_message}");
    assert!(!error_message.contains("too-long-to-repeat"), "{error_message}");
}

#[tokio::test]
async fn fails_when_mcp_server_returns_tool_error() {
    let execution_error = TestRunner::workflow(fixtures::DYNAMIC_TOOL_CALL)
        .input(json!({ "project_id": 42, "task_id": 7 }))
        .provider("openai", |provider| {
            provider.api_key("test-api-key").model("model-a", |model| {
                model.turn().expect_prompt("Summarize:").respond_json(json!({ "summary": "done" }));
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
        .expect_err("execution should fail when MCP server returns an error");

    assert!(execution_error.to_string().contains("fetch_task_data"));
}
