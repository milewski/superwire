use crate::api::{ExecutionRequest, ExecutionResponse, FormatRequest, FormatResponse, ValidationRequest, ValidationResponse};
use crate::event::ExecutorEvent;
use crate::model::{ModelProvider, OpenAiModelProvider};
use crate::runtime::{ExecutorError, WorkflowExecutor};
use serde_json::Value;
use superwire_core::dsl::format_workflow_source;
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

        log::info!("starting workflow execution");
        log::debug!(
            "resolved workflow source for execution: bytes={}, input_provided={}, secrets_provided={}",
            workflow_source.len(),
            !request.input.is_null(),
            !request.secrets.is_null()
        );

        let executor = WorkflowExecutor::from_source_with_runtime_values(&workflow_source, &request.input, &request.secrets)?;
        log::debug!("workflow planned with agent order: {:?}", executor.agent_execution_order());
        let output = executor
            .execute(
                request.input,
                request.secrets,
                &self.model_provider,
                None,
                request.options.max_concurrency,
            )
            .await?;

        log::info!("workflow execution completed");

        Ok(ExecutionResponse { output })
    }

    pub fn validate(&self, request: ValidationRequest) -> Result<ValidationResponse, ExecutorError> {
        let workflow_source = request
            .resolved_workflow_source()
            .map_err(|message| ExecutorError::Other { message })?;

        let empty_input = Value::Null;
        let executor = WorkflowExecutor::from_source_with_runtime_values(&workflow_source, &empty_input, &request.secrets)?;

        executor.validate_runtime_configuration_without_input(&request.secrets)?;

        Ok(ValidationResponse {
            valid: true,
            details: None,
        })
    }

    pub fn format(&self, request: FormatRequest) -> Result<FormatResponse, ExecutorError> {
        let workflow_source = request
            .resolved_workflow_source()
            .map_err(|message| ExecutorError::Other { message })?;

        let formatted_workflow_source = format_workflow_source(&workflow_source).map_err(|error| ExecutorError::Other {
            message: error.to_string(),
        })?;

        Ok(FormatResponse {
            valid: true,
            formatted_workflow_source,
        })
    }

    pub fn execute_stream(&self, request: ExecutionRequest) -> mpsc::Receiver<ExecutorEvent> {
        let (event_sender, event_receiver) = mpsc::channel(EVENT_BUFFER_SIZE);
        let model_provider = self.model_provider.clone();
        let max_concurrency = request.options.max_concurrency;

        tokio::spawn(async move {
            let execution_result = run_streamed_execution(request, model_provider, event_sender.clone(), max_concurrency).await;

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
    max_concurrency: usize,
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

    log::info!("starting streamed workflow execution");
    log::debug!(
        "resolved workflow source for streamed execution: bytes={}, input_provided={}, secrets_provided={}, max_concurrency={}",
        workflow_source.len(),
        !request.input.is_null(),
        !request.secrets.is_null(),
        max_concurrency
    );

    let executor = WorkflowExecutor::from_source_with_runtime_values(&workflow_source, &request.input, &request.secrets)?;
    let agent_execution_order = executor.agent_execution_order();
    let mcp_imports = executor
        .mcp_imports()
        .iter()
        .map(|import| crate::event::PlannedMcpImportEvent {
            name: import.name.clone(),
            kind: match import.kind {
                superwire_core::semantic::PlannedMcpImportKind::Prompt => "prompt".to_string(),
                superwire_core::semantic::PlannedMcpImportKind::Resource => "resource".to_string(),
            },
            server_name: import.server_name.clone(),
            item_name: import.item_name.clone(),
        })
        .collect::<Vec<_>>();

    log::debug!("streamed workflow planned with agent order: {agent_execution_order:?}");
    event_sender
        .send(ExecutorEvent::workflow_planned(agent_execution_order, mcp_imports))
        .await
        .map_err(|error| ExecutorError::Other {
            message: format!("failed to send workflow planned event: {error}"),
        })?;

    let output = executor
        .execute(
            request.input,
            request.secrets,
            &model_provider,
            Some(event_sender.clone()),
            max_concurrency,
        )
        .await?;

    event_sender
        .send(ExecutorEvent::workflow_completed(output))
        .await
        .map_err(|error| ExecutorError::Other {
            message: format!("failed to send workflow completion event: {error}"),
        })?;

    log::info!("streamed workflow execution completed");

    Ok(())
}
