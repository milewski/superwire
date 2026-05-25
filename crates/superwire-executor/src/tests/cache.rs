use super::fixtures;
use super::support::{request, TrackingModelProvider};
use crate::runtime::AgentCacheSession;
use crate::service::ExecutorService;
use serde_json::json;

#[tokio::test]
async fn service_reuses_cached_agent_output_for_same_session() {
    let model_provider = TrackingModelProvider::new(vec![json!({ "value": "first" }), json!({ "value": "second" })]);
    let service = ExecutorService::new(model_provider.clone());
    let cache_session = AgentCacheSession::new("browser-a");
    let first_response = service
        .execute_for_session(request(fixtures::MINIMUM), cache_session.clone())
        .await
        .expect("first execution should succeed");
    let second_response = service
        .execute_for_session(request(fixtures::MINIMUM), cache_session)
        .await
        .expect("second execution should succeed");

    assert_eq!(first_response.output, json!({ "greeting": "first" }));
    assert_eq!(second_response.output, json!({ "greeting": "first" }));
    assert_eq!(model_provider.recorded_count(), 1);
}

#[tokio::test]
async fn service_reuses_cached_agent_output_for_same_client_cache_key() {
    let model_provider = TrackingModelProvider::new(vec![json!({ "value": "first" }), json!({ "value": "second" })]);
    let service = ExecutorService::new(model_provider.clone());
    let mut first_request = request(fixtures::MINIMUM);
    let mut second_request = request(fixtures::MINIMUM);
    first_request.options.cache_key = Some("client-cache-key".to_string());
    second_request.options.cache_key = Some("client-cache-key".to_string());
    let first_response = service.execute(first_request).await.expect("first execution should succeed");
    let second_response = service.execute(second_request).await.expect("second execution should succeed");

    assert_eq!(first_response.output, json!({ "greeting": "first" }));
    assert_eq!(second_response.output, json!({ "greeting": "first" }));
    assert_eq!(model_provider.recorded_count(), 1);
}

#[tokio::test]
async fn service_skips_cache_without_client_cache_key() {
    let model_provider = TrackingModelProvider::new(vec![json!({ "value": "first" }), json!({ "value": "second" })]);
    let service = ExecutorService::new(model_provider.clone());
    let first_response = service
        .execute(request(fixtures::MINIMUM))
        .await
        .expect("first execution should succeed");
    let second_response = service
        .execute(request(fixtures::MINIMUM))
        .await
        .expect("second execution should succeed");

    assert_eq!(first_response.output, json!({ "greeting": "first" }));
    assert_eq!(second_response.output, json!({ "greeting": "second" }));
    assert_eq!(model_provider.recorded_count(), 2);
}

#[tokio::test]
async fn service_skips_cache_when_request_disables_it() {
    let model_provider = TrackingModelProvider::new(vec![json!({ "value": "first" }), json!({ "value": "second" })]);
    let service = ExecutorService::new(model_provider.clone());
    let cache_session = AgentCacheSession::new("browser-a");
    let mut first_request = request(fixtures::MINIMUM);
    let mut second_request = request(fixtures::MINIMUM);
    first_request.options.use_cache = false;
    second_request.options.use_cache = false;
    let first_response = service
        .execute_for_session(first_request, cache_session.clone())
        .await
        .expect("first execution should succeed");
    let second_response = service
        .execute_for_session(second_request, cache_session)
        .await
        .expect("second execution should succeed");

    assert_eq!(first_response.output, json!({ "greeting": "first" }));
    assert_eq!(second_response.output, json!({ "greeting": "second" }));
    assert_eq!(model_provider.recorded_count(), 2);
}

#[tokio::test]
async fn service_separates_cache_by_session() {
    let model_provider = TrackingModelProvider::new(vec![json!({ "value": "first" }), json!({ "value": "second" })]);
    let service = ExecutorService::new(model_provider.clone());
    let first_response = service
        .execute_for_session(request(fixtures::MINIMUM), AgentCacheSession::new("browser-a"))
        .await
        .expect("first execution should succeed");
    let second_response = service
        .execute_for_session(request(fixtures::MINIMUM), AgentCacheSession::new("browser-b"))
        .await
        .expect("second execution should succeed");

    assert_eq!(first_response.output, json!({ "greeting": "first" }));
    assert_eq!(second_response.output, json!({ "greeting": "second" }));
    assert_eq!(model_provider.recorded_count(), 2);
}

#[tokio::test]
async fn service_reuses_cached_context_compaction_for_same_session() {
    let model_provider = TrackingModelProvider::new(vec![
        json!({ "value": "research" }),
        json!("compact summary"),
        json!({ "value": "summary" }),
        json!("unexpected compaction rerun"),
    ]);
    let service = ExecutorService::new(model_provider.clone());
    let cache_session = AgentCacheSession::new("browser-a");
    let first_response = service
        .execute_for_session(request(fixtures::AGENT_CONTEXT_COMPACTION), cache_session.clone())
        .await
        .expect("first execution should succeed");
    let second_response = service
        .execute_for_session(request(fixtures::AGENT_CONTEXT_COMPACTION), cache_session)
        .await
        .expect("second execution should succeed");
    let recorded_agent_names = model_provider.recorded_agent_names();

    assert_eq!(first_response.output, json!({ "result": "summary" }));
    assert_eq!(second_response.output, json!({ "result": "summary" }));
    assert_eq!(recorded_agent_names, vec!["research", "summarize__context_compaction", "summarize"]);
}

#[tokio::test]
async fn service_reuses_cached_output_context_compaction_for_same_session() {
    let model_provider = TrackingModelProvider::new(vec![
        json!({ "result": "cat joke" }),
        json!("compact joke context"),
        json!("unexpected output compaction rerun"),
    ]);
    let service = ExecutorService::new(model_provider.clone());
    let cache_session = AgentCacheSession::new("browser-a");
    let first_response = service
        .execute_for_session(request(fixtures::AGENT_CONTEXT_EXPRESSION_COMPACTION), cache_session.clone())
        .await
        .expect("first execution should succeed");
    let second_response = service
        .execute_for_session(request(fixtures::AGENT_CONTEXT_EXPRESSION_COMPACTION), cache_session)
        .await
        .expect("second execution should succeed");
    let recorded_agent_names = model_provider.recorded_agent_names();

    assert_eq!(
        first_response.output,
        json!({ "compacted": { "agent": "analyzer__context_compaction" } })
    );
    assert_eq!(
        second_response.output,
        json!({ "compacted": { "agent": "analyzer__context_compaction" } })
    );
    assert_eq!(recorded_agent_names, vec!["analyzer", "analyzer__context_compaction"]);
}
