use super::fixtures;
use super::support;
use crate::event::ExecutorEventKind;
use serde_json::json;

#[tokio::test]
async fn lifecycle_events_are_emitted_in_order() {
    let service = support::service(vec![json!("first"), json!("second")]);
    let request = support::request_with_input(fixtures::LINEAR_CHAIN, json!({ "topic": "testing" }));
    let mut receiver = service.execute_stream(request);
    let mut kinds = Vec::new();

    while let Some(event) = receiver.recv().await {
        kinds.push(event.kind);
    }

    assert_eq!(kinds.first(), Some(&ExecutorEventKind::WorkflowStarted));
    assert!(kinds.contains(&ExecutorEventKind::WorkflowPlanned));
    assert!(kinds.contains(&ExecutorEventKind::AgentStarted));
    assert!(kinds.contains(&ExecutorEventKind::AgentCompleted));
    assert_eq!(kinds.last(), Some(&ExecutorEventKind::WorkflowCompleted));
}

#[tokio::test]
async fn agent_names_are_included_in_events() {
    let service = support::service(vec![json!("first"), json!("second")]);
    let request = support::request_with_input(fixtures::LINEAR_CHAIN, json!({ "topic": "testing" }));
    let mut receiver = service.execute_stream(request);
    let mut agent_names = Vec::new();

    while let Some(event) = receiver.recv().await {
        if let Some(name) = event.agent_name {
            agent_names.push(name);
        }
    }

    assert!(agent_names.contains(&"first".to_string()));
    assert!(agent_names.contains(&"second".to_string()));
}

#[tokio::test]
async fn failure_emits_workflow_failed_event() {
    let service = support::service(vec![]);
    let request = support::request_with_input(fixtures::INPUT_STRING, json!({ "topic": 123 }));
    let mut receiver = service.execute_stream(request);
    let mut kinds = Vec::new();

    while let Some(event) = receiver.recv().await {
        kinds.push(event.kind);
    }

    assert_eq!(kinds.first(), Some(&ExecutorEventKind::WorkflowStarted));
    assert_eq!(kinds.last(), Some(&ExecutorEventKind::WorkflowFailed));
}
