#[macro_use]
mod support;

use serde_json::json;
use support::fixtures;
use support::runner::TestRunner;

#[tokio::test]
async fn supports_variant_match_projection_fixture() {
    let output = TestRunner::workflow(fixtures::VARIANT_MATCH_PROJECTION)
        .provider("openai", |provider| {
            provider.api_key("test-api-key");
            provider.model("model-a", |model| {
                model.turn().expect_prompt("Generate an event result.").respond_json(json!({
                    "event": { "type": "created", "id": "event-1" },
                    "maybe_event": null
                }));
            });
        })
        .run()
        .await
        .expect("fixture runner should execute variant match workflow");

    assert_eq!(
        output.output,
        json!({
            "event_id": "event-1",
            "created_id": "event-1",
            "nullable_event_id": "none"
        })
    );
}
