#[macro_use]
mod support;

use serde_json::json;
use support::fixtures;
use support::runner::{Format, TestRunner};

#[tokio::test]
async fn executes_fixture_through_scripted_provider_server() {
    let run_output = TestRunner::workflow(fixtures::MINIMUM)
        .provider("openai", |provider| {
            provider.api_key("test-api-key").model("model-a", |model| {
                model
                    .turn()
                    .with_messages(|messages| {
                        assert_eq!(messages.len(), 2);
                        assert_eq!(messages.last().and_then(|message| message.get("role")), Some(&json!("user")));
                    })
                    .expect_prompt("Write a short welcome message.")
                    .respond_string("hello from fixture runner");
            });
        })
        .run()
        .await
        .expect("fixture runner should execute workflow");

    assert_eq!(run_output.output, json!({ "greeting": "hello from fixture runner" }));
    assert_eq!(run_output.provider_requests["openai"].len(), 1);
}

#[tokio::test]
async fn falls_back_when_model_does_not_support_json_schema() {
    let run_output = TestRunner::workflow(fixtures::MINIMUM)
        .provider("openai", |provider| {
            provider.api_key("test-api-key").model("model-a", |model| {
                model
                    .turn()
                    .with_response_format(Format::Auto)
                    .respond_error("response_format json_schema is not supported by this model");

                model
                    .turn()
                    .with_response_format(Format::JsonObject)
                    .respond_string("hello after fallback");
            });
        })
        .run()
        .await
        .expect("fixture runner should fall back after json_schema provider error");

    assert_eq!(run_output.output, json!({ "greeting": "hello after fallback" }));
    assert_eq!(run_output.provider_requests["openai"].len(), 2);
}
