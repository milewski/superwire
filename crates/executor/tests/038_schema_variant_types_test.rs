#[macro_use]
mod support;

use serde_json::json;
use support::fixtures;
use support::runner::TestRunner;

#[tokio::test]
async fn supports_schema_variant_types_fixture() {
    let output = TestRunner::workflow(fixtures::SCHEMA_VARIANT_TYPES)
        .provider("openai", |provider| {
            provider.api_key("test-api-key");
            provider.model("model-a", |model| {
                model.turn().expect_prompt("Generate an event wrapper.").respond_json(json!({
                    "value": {
                        "event": {
                            "type": "created",
                            "id": "event-1",
                            "actor": { "name": "Ada" }
                        },
                        "nullable_event": {
                            "type": "deleted",
                            "id": "event-2",
                            "reason": "cleanup"
                        }
                    }
                }));
            });
        })
        .run()
        .await
        .expect("fixture runner should execute schema variant types workflow");

    assert_eq!(
        output.output,
        json!({
            "event_id": "event-1",
            "created_actor": "Ada",
            "deleted_reason": "not-deleted",
            "nullable_event_id": "event-2",
            "nullable_deleted_reason": "cleanup"
        })
    );
}
