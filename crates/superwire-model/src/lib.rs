use std::future::Future;
use superwire_protocol::event::ExecutorEvent;
use tokio::sync::mpsc;

mod error;
mod provider;
mod response;
mod types;

pub use error::ModelProviderError;
pub use provider::ModelProvider;
pub use response::parse_model_json_output;
pub use types::{
    FinalizeCallKind, ModelAsset, ModelAssetSource, ModelFileAttachment, ModelPromptContent, ModelRequest, ModelResponse, ModelSchema,
    ModelSchemaCache, ModelToolDefinition, ModelToolSource, ToolCallLimitScope, ToolCallTracker,
};

pub trait ExecutorEventSenderExt {
    fn send_observed(&self, event: ExecutorEvent) -> impl Future<Output = ()> + Send;
    fn try_send_observed(&self, event: ExecutorEvent);
}

impl ExecutorEventSenderExt for mpsc::Sender<ExecutorEvent> {
    fn send_observed(&self, event: ExecutorEvent) -> impl Future<Output = ()> + Send {
        let event_sender = self.clone();

        async move {
            let event_kind = event.kind.as_str();

            if event_sender.send(event).await.is_err() {
                log::warn!("failed to emit executor event: kind={event_kind}, reason=event receiver is closed");
            }
        }
    }

    fn try_send_observed(&self, event: ExecutorEvent) {
        let event_kind = event.kind.as_str();

        match self.try_send(event) {
            Ok(()) => {}
            Err(mpsc::error::TrySendError::Closed(_)) => {
                log::warn!("failed to emit executor event: kind={event_kind}, reason=event receiver is closed");
            }
            Err(mpsc::error::TrySendError::Full(event)) => {
                let event_sender = self.clone();
                let Ok(runtime_handle) = tokio::runtime::Handle::try_current() else {
                    log::error!(
                        "failed to emit executor event: kind={event_kind}, reason=event buffer is full and no async runtime is available"
                    );

                    return;
                };

                runtime_handle.spawn(async move {
                    if event_sender.send(event).await.is_err() {
                        log::warn!(
                            "failed to emit executor event after waiting for capacity: kind={event_kind}, reason=event receiver is closed"
                        );
                    }
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use superwire_protocol::event::ExecutorEventKind;

    #[tokio::test]
    async fn observed_send_waits_for_capacity_instead_of_dropping_lifecycle_event() {
        let (event_sender, mut event_receiver) = mpsc::channel(1);

        event_sender
            .send(ExecutorEvent::workflow_started())
            .await
            .expect("first lifecycle event should fill the channel");
        event_sender.try_send_observed(ExecutorEvent::workflow_started());

        let first_event = event_receiver.recv().await.expect("first lifecycle event should be received");
        let second_event = tokio::time::timeout(Duration::from_secs(1), event_receiver.recv())
            .await
            .expect("overflow lifecycle event should be delivered after capacity becomes available")
            .expect("event sender should remain open");

        assert_eq!(first_event.kind, ExecutorEventKind::WorkflowStarted);
        assert_eq!(second_event.kind, ExecutorEventKind::WorkflowStarted);
    }
}
