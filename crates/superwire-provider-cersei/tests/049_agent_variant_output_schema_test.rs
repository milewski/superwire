#[macro_use]
mod support;

use serde_json::json;
use support::fixtures;
use support::runner::TestRunner;

#[tokio::test]
async fn supports_inline_variant_agent_output_schema() {
    let output = TestRunner::workflow(fixtures::AGENT_VARIANT_OUTPUT_SCHEMA)
        .provider("openai", |provider| {
            provider.api_key("test-api-key");
            provider.model("model-a", |model| {
                model
                    .turn()
                    .expect_prompt("Route the billing support request to the best next action.")
                    .respond_json(json!({
                        "routing": {
                            "action": "team_escalation",
                            "queue": "billing",
                            "priority": "urgent",
                            "reason": "Refund requires manual approval",
                            "details": {
                                "customer_summary": "Customer was double charged for annual billing."
                            }
                        }
                    }));
            });
        })
        .run()
        .await
        .expect("fixture runner should execute inline variant agent output workflow");

    assert_eq!(
        output.output,
        json!({
            "routing": {
                "routing": {
                    "action": "team_escalation",
                    "queue": "billing",
                    "priority": "urgent",
                    "reason": "Refund requires manual approval",
                    "details": {
                        "customer_summary": "Customer was double charged for annual billing."
                    }
                }
            },
            "action": "team_escalation",
            "selected_summary": "Customer was double charged for annual billing.",
            "escalation_reason": "Refund requires manual approval",
            "self_service_article": "none"
        })
    );
}
