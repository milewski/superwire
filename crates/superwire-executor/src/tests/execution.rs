use super::fixtures;
use super::support::{request, TrackingModelProvider};
use crate::service::ExecutorService;
use serde_json::json;

#[tokio::test]
async fn minimum_workflow_produces_output() {
    assert_eq!(
        execute!(fixtures::MINIMUM, output: { "value": "hello world" }).await,
        json!({ "greeting": "hello world" })
    );
}

#[tokio::test]
async fn string_output_workflow() {
    assert_eq!(
        execute!(fixtures::STRING_OUTPUT, output: { "value": "This is a summary." }).await,
        json!({ "summary": "This is a summary." })
    );
}

#[tokio::test]
async fn object_output_workflow() {
    let output = execute! (
        fixtures::OBJECT_OUTPUT,
        output: {
            "name": "Alice",
            "age": 30,
            "role": "engineer"
        }
    )
    .await;

    assert_eq!(
        output,
        json!({
            "profile": {
                "name": "Alice",
                "age": 30,
                "role": "engineer"
            }
        })
    );
}

#[tokio::test]
async fn linear_chain_executes_in_order() {
    let output = execute!(
        fixtures::LINEAR_CHAIN,
        input: { "topic": "testing" },
        output: { "value": "first" },
        output: { "value": "second" },
    )
    .await;
    assert_eq!(output, json!({ "result": "second" }));
}

#[tokio::test]
async fn multiline_prompt_workflow() {
    let output = execute!(fixtures::MULTILINE_PROMPT, output: { "value": "Welcome!" }).await;
    assert_eq!(output, json!({ "message": "Welcome!" }));
}

#[tokio::test]
async fn inference_settings_workflow() {
    let output = execute!(fixtures::INFERENCE_SETTINGS, output: { "value": "All systems go." }).await;
    assert_eq!(output, json!({ "analysis": "All systems go." }));
}

#[tokio::test]
async fn inference_settings_are_sent_with_model_request() {
    let model_provider = TrackingModelProvider::new(vec![json!({ "value": "All systems go." })]);
    let service = ExecutorService::new(model_provider.clone());

    service
        .execute(request(fixtures::INFERENCE_SETTINGS))
        .await
        .expect("workflow should execute");

    let recorded_requests = model_provider
        .recorded_requests
        .lock()
        .expect("recorded requests lock should not be poisoned");
    let request = recorded_requests.first().expect("agent request should be recorded");

    assert_eq!(request.inference.get("temperature"), Some(&json!(0.2)));
    assert_eq!(request.inference.get("max_tokens"), Some(&json!(4000)));
}

#[tokio::test]
async fn direct_agent_context_is_sent_without_rewriting_history() {
    let model_provider = TrackingModelProvider::new(vec![json!({ "value": "research" }), json!({ "value": "continued" })]);
    let service = ExecutorService::new(model_provider.clone());

    service
        .execute(request(fixtures::AGENT_CONTEXT_SHARING))
        .await
        .expect("workflow should execute");

    let recorded_requests = model_provider
        .recorded_requests
        .lock()
        .expect("recorded requests lock should not be poisoned");

    assert_eq!(recorded_requests.len(), 2);
    assert_eq!(recorded_requests[0].context, None);
    assert_eq!(recorded_requests[1].context, Some(json!({ "agent": "research" })));
}

#[tokio::test]
async fn compact_agent_context_runs_compaction_before_target_agent() {
    let model_provider = TrackingModelProvider::new(vec![
        json!({ "value": "research" }),
        json!("compact summary"),
        json!({ "value": "summary" }),
    ]);
    let service = ExecutorService::new(model_provider.clone());

    service
        .execute(request(fixtures::AGENT_CONTEXT_COMPACTION))
        .await
        .expect("workflow should execute");

    let recorded_requests = model_provider
        .recorded_requests
        .lock()
        .expect("recorded requests lock should not be poisoned");

    assert_eq!(recorded_requests.len(), 3);
    assert_eq!(recorded_requests[1].agent_name, "summarize__context_compaction");
    assert_eq!(recorded_requests[1].context, Some(json!({ "agent": "research" })));
    assert_eq!(recorded_requests[1].prompt, "Compact this prior context for a short summary.");
    assert_eq!(
        recorded_requests[2].context,
        Some(json!({ "agent": "summarize__context_compaction" }))
    );
}

#[tokio::test]
async fn context_expression_serializes_agent_context_in_workflow_output() {
    let output = execute!(fixtures::AGENT_CONTEXT_EXPRESSION_OUTPUT, output: { "result": "cat joke" }).await;

    assert_eq!(
        output,
        json!({
            "context_value": { "agent": "analyzer" },
            "context_text": "stored {\"agent\":\"analyzer\"}",
        })
    );
}

#[tokio::test]
async fn compact_context_expression_runs_compaction_for_workflow_output() {
    let model_provider = TrackingModelProvider::new(vec![json!({ "result": "cat joke" }), json!("compact joke context")]);
    let service = ExecutorService::new(model_provider.clone());

    let output = service
        .execute(request(fixtures::AGENT_CONTEXT_EXPRESSION_COMPACTION))
        .await
        .expect("workflow should execute")
        .output;

    assert_eq!(
        output,
        json!({
            "compacted": { "agent": "analyzer__context_compaction" },
        })
    );

    let recorded_requests = model_provider
        .recorded_requests
        .lock()
        .expect("recorded requests lock should not be poisoned");

    assert_eq!(recorded_requests.len(), 2);
    assert_eq!(recorded_requests[1].agent_name, "analyzer__context_compaction");
    assert_eq!(recorded_requests[1].context, Some(json!({ "agent": "analyzer" })));
    assert_eq!(recorded_requests[1].prompt, "Compact this context for final output.");
}

#[tokio::test]
async fn context_expression_renders_inside_agent_instruction_template() {
    let model_provider = TrackingModelProvider::new(vec![json!({ "result": "cat joke" }), json!({ "result": "used context" })]);
    let service = ExecutorService::new(model_provider.clone());

    let output = service
        .execute(request(fixtures::AGENT_CONTEXT_EXPRESSION_PROMPT))
        .await
        .expect("workflow should execute")
        .output;

    assert_eq!(output, json!({ "result": "used context" }));

    let recorded_requests = model_provider
        .recorded_requests
        .lock()
        .expect("recorded requests lock should not be poisoned");

    assert_eq!(recorded_requests.len(), 2);
    assert_eq!(recorded_requests[1].prompt, "Use {\"agent\":\"analyzer\"}");
}
