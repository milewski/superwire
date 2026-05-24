use crate::model::{CerseiModelProvider, ModelProvider};
use crate::runtime::cache::{
    AgentCacheConfig, AgentCacheDriver, AgentCacheOptions, AgentCacheSession, AgentCacheStore, DEFAULT_AGENT_CACHE_TIME_TO_LIVE,
};
use crate::runtime::{ExecutorError, WorkflowExecutor};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use std::time::Instant;
use superwire_dsl::format_workflow_source;
use superwire_protocol::api::{
    CacheInvalidationResponse, ExecutionRequest, ExecutionResponse, FormatRequest, FormatResponse, GraphRequest, GraphResponse,
    ValidationRequest, ValidationResponse,
};
use superwire_protocol::event::ExecutorEvent;
use tokio::sync::mpsc;
use tokio::task::AbortHandle;

const EVENT_BUFFER_SIZE: usize = 64;
const COMPLETED_STREAM_RETENTION: Duration = Duration::from_secs(20 * 60);

#[derive(Debug, Clone)]
pub struct ExecutorService<ModelProviderType = CerseiModelProvider> {
    model_provider: ModelProviderType,
    streamed_executions: StreamedExecutionRegistry,
    agent_cache_store: Arc<dyn AgentCacheStore>,
    agent_cache_time_to_live: Duration,
}

impl Default for ExecutorService<CerseiModelProvider> {
    fn default() -> Self {
        Self::new(CerseiModelProvider)
    }
}

impl<ModelProviderType> ExecutorService<ModelProviderType> {
    #[must_use]
    pub fn new(model_provider: ModelProviderType) -> Self {
        let agent_cache_store = AgentCacheDriver::InMemory
            .build_store()
            .expect("in-memory agent cache store should build");

        Self {
            model_provider,
            streamed_executions: StreamedExecutionRegistry::default(),
            agent_cache_store,
            agent_cache_time_to_live: DEFAULT_AGENT_CACHE_TIME_TO_LIVE,
        }
    }

    pub fn with_agent_cache_driver(
        model_provider: ModelProviderType,
        cache_driver: AgentCacheDriver,
        cache_time_to_live: Duration,
    ) -> Result<Self, ExecutorError> {
        Self::with_agent_cache_config(model_provider, AgentCacheConfig::new(cache_driver), cache_time_to_live)
    }

    pub fn with_agent_cache_config(
        model_provider: ModelProviderType,
        cache_config: AgentCacheConfig,
        cache_time_to_live: Duration,
    ) -> Result<Self, ExecutorError> {
        Ok(Self {
            model_provider,
            streamed_executions: StreamedExecutionRegistry::default(),
            agent_cache_store: cache_config.build_store()?,
            agent_cache_time_to_live: cache_time_to_live,
        })
    }
}

impl<ModelProviderType> ExecutorService<ModelProviderType>
where
    ModelProviderType: ModelProvider + Clone + Send + Sync + 'static,
{
    pub async fn execute(&self, request: ExecutionRequest) -> Result<ExecutionResponse, ExecutorError> {
        self.execute_with_request_cache_key(request).await
    }

    pub async fn execute_for_session(
        &self,
        mut request: ExecutionRequest,
        cache_session: AgentCacheSession,
    ) -> Result<ExecutionResponse, ExecutorError> {
        request.options.cache_key = Some(cache_session.identifier().to_string());

        self.execute_with_request_cache_key(request).await
    }

    async fn execute_with_request_cache_key(&self, request: ExecutionRequest) -> Result<ExecutionResponse, ExecutorError> {
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
        let cache_options = self.cache_options_for_request(&request);
        let output = executor
            .execute_with_cache(
                request.input,
                request.secrets,
                &self.model_provider,
                None,
                request.options.max_concurrency,
                cache_options,
            )
            .await?;

        log::info!("workflow execution completed");

        Ok(ExecutionResponse { output })
    }

    pub fn invalidate_agent_cache_session(&self, cache_session: &AgentCacheSession) -> Result<CacheInvalidationResponse, ExecutorError> {
        let purged_entries = self.agent_cache_store.purge_session(cache_session)?;

        Ok(CacheInvalidationResponse { purged_entries })
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

    pub fn graph(&self, request: GraphRequest) -> Result<GraphResponse, ExecutorError> {
        let workflow_source = request
            .resolved_workflow_source()
            .map_err(|message| ExecutorError::Other { message })?;

        let empty_input = Value::Null;
        let executor = WorkflowExecutor::from_source_with_runtime_values(&workflow_source, &empty_input, &request.secrets)?;

        executor.validate_runtime_configuration_without_input(&request.secrets)?;

        Ok(GraphResponse {
            valid: true,
            graph: executor.execution_graph(),
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
        let streamed_execution = self.start_streamed_execution(request);
        let (event_sender, event_receiver) = mpsc::channel(EVENT_BUFFER_SIZE);

        tokio::spawn(async move {
            streamed_execution.forward_events(event_sender).await;
        });

        event_receiver
    }

    pub fn start_streamed_execution(&self, request: ExecutionRequest) -> StreamedExecutionSubscription {
        self.start_streamed_execution_with_request_cache_key(request)
    }

    pub fn start_streamed_execution_for_session(
        &self,
        mut request: ExecutionRequest,
        cache_session: AgentCacheSession,
    ) -> StreamedExecutionSubscription {
        request.options.cache_key = Some(cache_session.identifier().to_string());

        self.start_streamed_execution_with_request_cache_key(request)
    }

    fn start_streamed_execution_with_request_cache_key(&self, request: ExecutionRequest) -> StreamedExecutionSubscription {
        let run_identifier = self.streamed_executions.next_run_identifier();
        let subscription = self.streamed_executions.insert(run_identifier.clone());
        let registry = self.streamed_executions.clone();
        let model_provider = self.model_provider.clone();
        let max_concurrency = request.options.max_concurrency;
        let cache_options = self.cache_options_for_request(&request);

        let execution_task = tokio::spawn(async move {
            registry
                .run_streamed_execution(request, model_provider, run_identifier, max_concurrency, cache_options)
                .await;
        });
        self.streamed_executions
            .attach_abort_handle(&subscription.run_identifier, execution_task.abort_handle());

        subscription
    }

    pub fn reconnect_streamed_execution(
        &self,
        run_identifier: &str,
        last_event_identifier: Option<u64>,
    ) -> Option<StreamedExecutionSubscription> {
        self.streamed_executions.subscribe(run_identifier, last_event_identifier)
    }

    pub fn cancel_streamed_execution(&self, run_identifier: &str) -> bool {
        self.streamed_executions.cancel(run_identifier)
    }

    fn cache_options_for_request(&self, request: &ExecutionRequest) -> AgentCacheOptions {
        if !request.options.use_cache {
            return AgentCacheOptions::disabled();
        }

        let Some(cache_key) = request.options.cache_key_identifier() else {
            return AgentCacheOptions::disabled();
        };

        let cache_session = AgentCacheSession::new(cache_key);

        AgentCacheOptions::enabled(cache_session, self.agent_cache_store.clone(), self.agent_cache_time_to_live)
    }
}

#[derive(Debug)]
pub struct StreamedExecutionSubscription {
    pub run_identifier: String,
    pub receiver: mpsc::Receiver<SequencedExecutorEvent>,
}

impl StreamedExecutionSubscription {
    async fn forward_events(mut self, event_sender: mpsc::Sender<ExecutorEvent>) {
        while let Some(sequenced_event) = self.receiver.recv().await {
            if event_sender.send(sequenced_event.event).await.is_err() {
                break;
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct SequencedExecutorEvent {
    pub event_identifier: u64,
    pub event: ExecutorEvent,
}

#[derive(Debug, Clone)]
struct StreamedExecutionRegistry {
    executions: Arc<Mutex<HashMap<String, StreamedExecution>>>,
    next_run_sequence: Arc<AtomicU64>,
}

impl Default for StreamedExecutionRegistry {
    fn default() -> Self {
        Self {
            executions: Arc::new(Mutex::new(HashMap::new())),
            next_run_sequence: Arc::new(AtomicU64::new(1)),
        }
    }
}

impl StreamedExecutionRegistry {
    fn next_run_identifier(&self) -> String {
        let timestamp_ms = ExecutorEvent::current_timestamp_ms();
        let run_sequence = self.next_run_sequence.fetch_add(1, Ordering::SeqCst);

        format!("{timestamp_ms}-{run_sequence}")
    }

    fn insert(&self, run_identifier: String) -> StreamedExecutionSubscription {
        let streamed_execution = StreamedExecution::default();
        let subscription = streamed_execution.subscribe(run_identifier.clone(), None);

        self.executions
            .lock()
            .expect("streamed execution registry lock should not be poisoned")
            .insert(run_identifier, streamed_execution);

        subscription
    }

    fn attach_abort_handle(&self, run_identifier: &str, abort_handle: AbortHandle) {
        if let Some(streamed_execution) = self
            .executions
            .lock()
            .expect("streamed execution registry lock should not be poisoned")
            .get(run_identifier)
        {
            streamed_execution.attach_abort_handle(abort_handle);
        }
    }

    fn subscribe(&self, run_identifier: &str, last_event_identifier: Option<u64>) -> Option<StreamedExecutionSubscription> {
        self.executions
            .lock()
            .expect("streamed execution registry lock should not be poisoned")
            .get(run_identifier)
            .map(|streamed_execution| streamed_execution.subscribe(run_identifier.to_string(), last_event_identifier))
    }

    fn record_event(&self, run_identifier: &str, event: ExecutorEvent) {
        let should_schedule_cleanup = self
            .executions
            .lock()
            .expect("streamed execution registry lock should not be poisoned")
            .get(run_identifier)
            .is_some_and(|streamed_execution| streamed_execution.record_event(event));

        if should_schedule_cleanup {
            self.schedule_cleanup(run_identifier.to_string());
        }
    }

    fn schedule_cleanup(&self, run_identifier: String) {
        let registry = self.clone();

        tokio::spawn(async move {
            tokio::time::sleep(COMPLETED_STREAM_RETENTION).await;
            registry.remove(&run_identifier);
        });
    }

    fn remove(&self, run_identifier: &str) {
        self.executions
            .lock()
            .expect("streamed execution registry lock should not be poisoned")
            .remove(run_identifier);
    }

    fn cancel(&self, run_identifier: &str) -> bool {
        let streamed_execution = self
            .executions
            .lock()
            .expect("streamed execution registry lock should not be poisoned")
            .get(run_identifier)
            .cloned();

        let Some(streamed_execution) = streamed_execution else {
            return false;
        };

        let should_schedule_cleanup = streamed_execution.cancel();

        if should_schedule_cleanup {
            self.schedule_cleanup(run_identifier.to_string());
        }

        true
    }

    async fn run_streamed_execution<ModelProviderType>(
        self,
        request: ExecutionRequest,
        model_provider: ModelProviderType,
        run_identifier: String,
        max_concurrency: usize,
        cache_options: AgentCacheOptions,
    ) where
        ModelProviderType: ModelProvider + Clone + Send + Sync + 'static,
    {
        let (event_sender, mut event_receiver) = mpsc::channel(EVENT_BUFFER_SIZE);
        let event_registry = self.clone();
        let event_run_identifier = run_identifier.clone();
        let event_recorder = tokio::spawn(async move {
            while let Some(event) = event_receiver.recv().await {
                event_registry.record_event(&event_run_identifier, event);
            }
        });

        let workflow_started_at = Instant::now();
        let execution_result = run_streamed_execution(
            request,
            model_provider,
            event_sender.clone(),
            max_concurrency,
            workflow_started_at,
            cache_options,
        )
        .await;

        if let Err(error) = execution_result {
            log::error!("streamed workflow execution failed: {error}");

            if event_sender
                .send(ExecutorEvent::workflow_failed(
                    error.to_string(),
                    Some(workflow_started_at.elapsed()),
                ))
                .await
                .is_err()
            {
                log::debug!("workflow failed event dropped because the stream recorder closed");
            }
        }

        drop(event_sender);
        let _ = event_recorder.await;
    }
}

#[derive(Debug, Default, Clone)]
struct StreamedExecution {
    state: Arc<Mutex<StreamedExecutionState>>,
}

impl StreamedExecution {
    fn attach_abort_handle(&self, abort_handle: AbortHandle) {
        self.state
            .lock()
            .expect("streamed execution lock should not be poisoned")
            .abort_handle = Some(abort_handle);
    }

    fn subscribe(&self, run_identifier: String, last_event_identifier: Option<u64>) -> StreamedExecutionSubscription {
        let mut state = self.state.lock().expect("streamed execution lock should not be poisoned");
        let missed_events = state.events_after(last_event_identifier);
        let channel_size = EVENT_BUFFER_SIZE.max(missed_events.len() + EVENT_BUFFER_SIZE);
        let (event_sender, event_receiver) = mpsc::channel(channel_size);

        for event in missed_events {
            if event_sender.try_send(event).is_err() {
                break;
            }
        }

        if !state.completed {
            state.subscribers.push(event_sender);
        }

        StreamedExecutionSubscription {
            run_identifier,
            receiver: event_receiver,
        }
    }

    fn record_event(&self, event: ExecutorEvent) -> bool {
        let mut state = self.state.lock().expect("streamed execution lock should not be poisoned");

        if state.completed {
            return false;
        }

        let sequenced_event = state.next_event(event);
        let completed = sequenced_event.event.is_terminal();

        state.events.push(sequenced_event.clone());
        state
            .subscribers
            .retain(|subscriber| subscriber.try_send(sequenced_event.clone()).is_ok());

        if completed {
            state.completed = true;
            state.subscribers.clear();
        }

        completed
    }

    fn cancel(&self) -> bool {
        let abort_handle = self
            .state
            .lock()
            .expect("streamed execution lock should not be poisoned")
            .abort_handle
            .clone();
        let completed = self.record_event(ExecutorEvent::workflow_failed("Workflow cancelled.".to_string(), None));

        if let Some(abort_handle) = abort_handle {
            abort_handle.abort();
        }

        completed
    }
}

#[derive(Debug, Default)]
struct StreamedExecutionState {
    events: Vec<SequencedExecutorEvent>,
    subscribers: Vec<mpsc::Sender<SequencedExecutorEvent>>,
    next_event_identifier: u64,
    completed: bool,
    abort_handle: Option<AbortHandle>,
}

impl StreamedExecutionState {
    fn events_after(&self, last_event_identifier: Option<u64>) -> Vec<SequencedExecutorEvent> {
        self.events
            .iter()
            .filter(|event| last_event_identifier.is_none_or(|event_identifier| event.event_identifier > event_identifier))
            .cloned()
            .collect()
    }

    fn next_event(&mut self, event: ExecutorEvent) -> SequencedExecutorEvent {
        self.next_event_identifier += 1;

        SequencedExecutorEvent {
            event_identifier: self.next_event_identifier,
            event,
        }
    }
}

async fn run_streamed_execution<ModelProviderType>(
    request: ExecutionRequest,
    model_provider: ModelProviderType,
    event_sender: mpsc::Sender<ExecutorEvent>,
    max_concurrency: usize,
    workflow_started_at: Instant,
    cache_options: AgentCacheOptions,
) -> Result<(), ExecutorError>
where
    ModelProviderType: ModelProvider + Clone + Send + Sync + 'static,
{
    let workflow_source = request
        .resolved_workflow_source()
        .map_err(|message| ExecutorError::Other { message })?;

    if event_sender.send(ExecutorEvent::workflow_started()).await.is_err() {
        log::debug!("workflow start event dropped because the stream receiver closed");
    }

    log::info!("starting streamed workflow execution");
    log::debug!(
        "resolved workflow source for streamed execution: bytes={}, input_provided={}, secrets_provided={}, max_concurrency={}",
        workflow_source.len(),
        !request.input.is_null(),
        !request.secrets.is_null(),
        max_concurrency
    );

    let executor = WorkflowExecutor::from_source_with_runtime_values_and_event_sender(
        &workflow_source,
        &request.input,
        &request.secrets,
        Some(&event_sender),
    )?;
    let agent_execution_order = executor.agent_execution_order();
    let planned_steps = executor.planned_execution_steps(&request.input, &request.secrets, max_concurrency)?;
    let mcp_imports = executor
        .mcp_imports()
        .iter()
        .map(|import| superwire_protocol::event::PlannedMcpImportEvent {
            name: import.name.clone(),
            kind: match import.kind {
                superwire_semantic::PlannedMcpImportKind::Prompt => "prompt".to_string(),
                superwire_semantic::PlannedMcpImportKind::Resource => "resource".to_string(),
            },
            server_name: import.server_name.clone(),
            item_name: import.item_name.clone(),
        })
        .collect::<Vec<_>>();

    log::debug!("streamed workflow planned with agent order: {agent_execution_order:?}");
    if event_sender
        .send(ExecutorEvent::workflow_planned(agent_execution_order, mcp_imports, planned_steps))
        .await
        .is_err()
    {
        log::debug!("workflow planned event dropped because the stream receiver closed");
    }

    let output = executor
        .execute_with_cache(
            request.input,
            request.secrets,
            &model_provider,
            Some(event_sender.clone()),
            max_concurrency,
            cache_options,
        )
        .await?;

    if event_sender
        .send(ExecutorEvent::workflow_completed(output, workflow_started_at.elapsed()))
        .await
        .is_err()
    {
        log::debug!("workflow completion event dropped because the stream receiver closed");
    }

    log::info!("streamed workflow execution completed");

    Ok(())
}
