use axum::response::sse::Event;
use std::convert::Infallible;
use superwire_executor::service::SequencedExecutorEvent;
use superwire_protocol::event::MAX_SERIALIZED_PUBLIC_EVENT_BYTES;

pub fn event_to_sse_result(sequenced_event: SequencedExecutorEvent) -> Result<Event, Infallible> {
    debug_assert!(sequenced_event.maximum_sse_frame_bytes <= MAX_SERIALIZED_PUBLIC_EVENT_BYTES);

    Ok(Event::default()
        .event(sequenced_event.event.kind.as_str())
        .id(sequenced_event.event_identifier.to_string())
        .data(sequenced_event.serialized_data.as_ref()))
}
