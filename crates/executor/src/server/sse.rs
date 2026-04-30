use crate::event::ExecutorEvent;
use axum::response::sse::Event;
use serde_json::json;
use std::convert::Infallible;

pub fn event_to_sse_result(event: ExecutorEvent) -> Result<Event, Infallible> {
    let event_name = event.kind.as_str();
    let event_data = serde_json::to_string(&event).unwrap_or_else(|error| {
        json!({
            "kind": "workflow_failed",
            "message": format!("failed to serialize executor event: {error}"),
        })
        .to_string()
    });

    Ok(Event::default().event(event_name).data(event_data))
}
