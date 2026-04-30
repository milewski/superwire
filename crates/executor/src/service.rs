use crate::api::{ExecutionRequest, ExecutionResponse};
use crate::event::ExecutorEvent;
use crate::model::{ModelProvider, OpenAiModelProvider};
use crate::runtime::{ExecutorError, WorkflowExecutor};
use tokio::sync::mpsc;

const EVENT_BUFFER_SIZE: usize = 64;

#[derive(Debug, Clone)]
pub struct ExecutorService<ModelProviderType = OpenAiModelProvider> {
    model_provider: ModelProviderType,
}

impl Default for ExecutorService<OpenAiModelProvider> {
    fn default() -> Self {
        Self::new(OpenAiModelProvider)
    }
}

impl<ModelProviderType> ExecutorService<ModelProviderType> {
    #[must_use]
    pub fn new(model_provider: ModelProviderType) -> Self {
        Self { model_provider }
    }
}

impl<ModelProviderType> ExecutorService<ModelProviderType>
where
    ModelProviderType: ModelProvider + Clone + Send + Sync + 'static,
{
    pub async fn execute(&self, request: ExecutionRequest) -> Result<ExecutionResponse, ExecutorError> {
        let workflow_source = request
            .resolved_workflow_source()
            .map_err(|message| ExecutorError::Other { message })?;

        let executor = WorkflowExecutor::from_source(&workflow_source)?;
        let output = executor.execute(request.input, request.secrets, &self.model_provider, None).await?;

        Ok(ExecutionResponse { output })
    }

    pub fn execute_stream(&self, request: ExecutionRequest) -> mpsc::Receiver<ExecutorEvent> {
        let (event_sender, event_receiver) = mpsc::channel(EVENT_BUFFER_SIZE);
        let model_provider = self.model_provider.clone();

        tokio::spawn(async move {
            let execution_result = run_streamed_execution(request, model_provider, event_sender.clone()).await;

            if let Err(error) = execution_result {
                let _ = event_sender.send(ExecutorEvent::workflow_failed(error.to_string())).await;
            }
        });

        event_receiver
    }
}

async fn run_streamed_execution<ModelProviderType>(
    request: ExecutionRequest,
    model_provider: ModelProviderType,
    event_sender: mpsc::Sender<ExecutorEvent>,
) -> Result<(), ExecutorError>
where
    ModelProviderType: ModelProvider + Clone + Send + Sync + 'static,
{
    let workflow_source = request
        .resolved_workflow_source()
        .map_err(|message| ExecutorError::Other { message })?;

    event_sender
        .send(ExecutorEvent::workflow_started())
        .await
        .map_err(|error| ExecutorError::Other {
            message: format!("failed to send workflow start event: {error}"),
        })?;

    let executor = WorkflowExecutor::from_source(&workflow_source)?;
    event_sender
        .send(ExecutorEvent::workflow_planned(executor.agent_execution_order()))
        .await
        .map_err(|error| ExecutorError::Other {
            message: format!("failed to send workflow planned event: {error}"),
        })?;

    let output = executor
        .execute(request.input, request.secrets, &model_provider, Some(event_sender.clone()))
        .await?;

    event_sender
        .send(ExecutorEvent::workflow_completed(output))
        .await
        .map_err(|error| ExecutorError::Other {
            message: format!("failed to send workflow completion event: {error}"),
        })?;

    Ok(())
}
