use axum::response::sse::Event;
use serde_json::json;
use std::convert::Infallible;
use superwire_executor::service::SequencedExecutorEvent;

pub fn event_to_sse_result(sequenced_event: SequencedExecutorEvent) -> Result<Event, Infallible> {
    let event_name = sequenced_event.event.kind.as_str();
    let event_identifier = sequenced_event.event_identifier.to_string();
    let event_data = serde_json::to_string(&sequenced_event.event).unwrap_or_else(|error| {
        json!({
            "kind": "workflow_failed",
            "message": format!("failed to serialize executor event: {error}"),
        })
        .to_string()
    });

    Ok(Event::default().event(event_name).id(event_identifier).data(event_data))
}
