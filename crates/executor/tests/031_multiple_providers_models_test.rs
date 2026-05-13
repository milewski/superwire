#[macro_use]
mod support;

use serde_json::json;
use support::fixtures;
use support::runner::TestRunner;

#[tokio::test]
async fn executes_workflow_with_multiple_providers_and_models() {
    let output = TestRunner::workflow(fixtures::MULTIPLE_PROVIDERS_MODELS)
        .input(json!({ "topic": "incident response" }))
        .provider("primary", |provider| {
            provider.api_key("test-primary-key");
            provider.model("model-a", |model| {
                model
                    .turn()
                    .expect_prompt("Draft summary for incident response")
                    .respond_json(json!({ "value": "draft from primary" }));
            });

            provider.model("model-c", |model| {
                model
                    .turn()
                    .expect_prompt("Finalize using review: {\"value\":\"reviewed by backup\"}")
                    .respond_json(json!({ "value": "final from primary" }));
            });
        })
        .provider("backup", |provider| {
            provider.api_key("test-backup-key");
            provider.model("model-b", |model| {
                model
                    .turn()
                    .expect_prompt("Review this draft: {\"value\":\"draft from primary\"}")
                    .respond_json(json!({ "value": "reviewed by backup" }));
            });
        })
        .run()
        .await
        .expect("fixture runner should execute workflow across providers and models");

    assert_eq!(
        output.output,
        json!({
            "draft": { "value": "draft from primary" },
            "review": { "value": "reviewed by backup" },
            "final": { "value": "final from primary" },
        })
    );

    assert_eq!(output.provider_requests["primary"].len(), 2);
    assert_eq!(output.provider_requests["backup"].len(), 1);
}
