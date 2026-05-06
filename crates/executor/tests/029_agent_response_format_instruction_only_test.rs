#[macro_use]
mod support;

use serde_json::json;
use support::fixtures;
use support::runner::{Format, TestRunner};

#[tokio::test]
async fn asserts_instruction_only_response_format() {
    let run_output = TestRunner::workflow(fixtures::AGENT_RESPONSE_FORMAT_INSTRUCTION_ONLY)
        .provider("openai", |provider| {
            provider.api_key("test-api-key").model("model-a", |model| {
                model.turn().with_response_format(Format::InstructionOnly).respond_string("hello");
            });
        })
        .run()
        .await
        .expect("fixture runner should assert instruction_only response format");

    assert_eq!(run_output.output, json!({ "greeting": "hello" }));
}
