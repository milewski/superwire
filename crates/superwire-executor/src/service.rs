use crate::model::{ExecutorEventSenderExt, ModelProvider};
use crate::runtime::cache::{
    AgentCacheConfig, AgentCacheDriver, AgentCacheOptions, AgentCacheSession, AgentCacheStore, DEFAULT_AGENT_CACHE_TIME_TO_LIVE,
};
use crate::runtime::{ExecutorError, WorkflowExecutor};
use futures::FutureExt;
use std::collections::{HashMap, VecDeque};
use std::panic::AssertUnwindSafe;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};
use superwire_dsl::format_workflow_source;
use superwire_mcp::{HttpMcpClientFactory, McpClientFactory};
use superwire_protocol::api::{
    CacheInvalidationResponse, CancellationTransition, ExecutionRequest, ExecutionResponse, FormatRequest, FormatResponse, GraphRequest,
    GraphResponse, ValidationRequest, ValidationResponse,
};
use superwire_protocol::event::{
    ExecutorDiagnostic, ExecutorDiagnosticCode, ExecutorDiagnosticSubject, ExecutorEvent, ExecutorStage, PublicEventSerializationError,
    SerializedPublicExecutorEvent, MAX_SERIALIZED_PUBLIC_EVENT_BYTES,
};
use tokio::sync::mpsc;
use tokio::task::{AbortHandle, JoinError, JoinHandle};
use uuid::Uuid;

const EVENT_BUFFER_SIZE: usize = 64;
pub const MAX_RETAINED_EVENTS_PER_RUN: usize = 256;
pub const STREAM_SUBSCRIBER_CAPACITY: usize = MAX_RETAINED_EVENTS_PER_RUN + 1;
pub const MAX_RETAINED_EVENT_BYTES_PER_RUN: usize = 8 * 1024 * 1024;
pub const MAX_RETAINED_EVENT_BYTES_GLOBAL: usize = 64 * 1024 * 1024;
pub const TERMINAL_EVENT_RESERVE_BYTES: usize = MAX_SERIALIZED_PUBLIC_EVENT_BYTES;
const MAX_NONTERMINAL_EVENT_BYTES_PER_RUN: usize = MAX_RETAINED_EVENT_BYTES_PER_RUN - TERMINAL_EVENT_RESERVE_BYTES;
const MAX_EXPIRED_RUN_IDENTIFIERS: usize = 1024;
const MAX_RETAINED_EXECUTIONS: usize = 1024;
const COMPLETED_STREAM_RETENTION: Duration = Duration::from_secs(20 * 60);

#[derive(Debug, Clone)]
pub struct ExecutorService<ModelProviderType> {
    model_provider: ModelProviderType,
    streamed_executions: StreamedExecutionRegistry,
    agent_cache_store: Arc<dyn AgentCacheStore>,
    agent_cache_time_to_live: Duration,
    mcp_client_factory: Arc<dyn McpClientFactory>,
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
            mcp_client_factory: Arc::new(HttpMcpClientFactory),
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
            mcp_client_factory: Arc::new(HttpMcpClientFactory),
        })
    }

    #[must_use]
    pub fn with_mcp_client_factory(mut self, mcp_client_factory: Arc<dyn McpClientFactory>) -> Self {
        self.mcp_client_factory = mcp_client_factory;
        self
    }

    pub fn invalidate_agent_cache_session(&self, cache_session: &AgentCacheSession) -> Result<CacheInvalidationResponse, ExecutorError> {
        let purged_entries = self.agent_cache_store.purge_session(cache_session)?;

        Ok(CacheInvalidationResponse { purged_entries })
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
        let workflow_source = request.resolved_workflow_source().map_err(ExecutorError::invalid_input)?;

        log::info!("starting workflow execution");
        log::debug!(
            "resolved workflow source for execution: bytes={}, input_provided={}, secrets_provided={}",
            workflow_source.len(),
            !request.input.is_null(),
            !request.secrets.is_null()
        );

        let cache_options = self.cache_options_for_request(&request);
        let max_concurrency = request.options.max_concurrency;
        let input = request.input;
        let secrets = request.secrets;
        let mcp_client_factory = Arc::clone(&self.mcp_client_factory);
        let (executor, input, secrets) = tokio::task::spawn_blocking(move || {
            let executor = WorkflowExecutor::from_source_with_runtime_values_and_mcp_client_factory(
                &workflow_source,
                &input,
                &secrets,
                mcp_client_factory.as_ref(),
            )?;

            Ok::<_, ExecutorError>((executor, input, secrets))
        })
        .await
        .map_err(|join_error| ExecutorError::internal_panic(format!("workflow planning task failed: {join_error}")))??;
        log::debug!("workflow planned with agent order: {:?}", executor.agent_execution_order());
        let output =
            match AssertUnwindSafe(executor.execute_with_cache(input, secrets, &self.model_provider, None, max_concurrency, cache_options))
                .catch_unwind()
                .await
            {
                Ok(execution_result) => execution_result?,
                Err(_) => {
                    return Err(ExecutorError::internal_panic("workflow execution panicked"));
                }
            };

        log::info!("workflow execution completed");

        Ok(ExecutionResponse { output })
    }

    pub fn validate(&self, request: ValidationRequest) -> Result<ValidationResponse, ExecutorError> {
        let workflow_source = request.resolved_workflow_source().map_err(ExecutorError::invalid_input)?;
        let executor = WorkflowExecutor::from_source_for_validation_with_mcp_client_factory(
            &workflow_source,
            &request.secrets,
            self.mcp_client_factory.as_ref(),
        )?;

        executor.validate_runtime_configuration_without_input(&request.secrets)?;

        Ok(ValidationResponse {
            valid: true,
            details: None,
        })
    }

    pub fn graph(&self, request: GraphRequest) -> Result<GraphResponse, ExecutorError> {
        let workflow_source = request.resolved_workflow_source().map_err(ExecutorError::invalid_input)?;
        let executor = WorkflowExecutor::from_source_for_validation_with_mcp_client_factory(
            &workflow_source,
            &request.secrets,
            self.mcp_client_factory.as_ref(),
        )?;

        executor.validate_runtime_configuration_without_input(&request.secrets)?;

        Ok(GraphResponse {
            valid: true,
            graph: executor.execution_graph(),
        })
    }

    pub fn format(&self, request: FormatRequest) -> Result<FormatResponse, ExecutorError> {
        let workflow_source = request.resolved_workflow_source().map_err(ExecutorError::invalid_input)?;
        let formatted_workflow_source =
            format_workflow_source(&workflow_source).map_err(|error| ExecutorError::invalid_input(error.to_string()))?;

        Ok(FormatResponse {
            valid: true,
            formatted_workflow_source,
        })
    }

    pub fn execute_stream(&self, request: ExecutionRequest) -> mpsc::Receiver<ExecutorEvent> {
        let (event_sender, event_receiver) = mpsc::channel(EVENT_BUFFER_SIZE);

        match self.start_streamed_execution(request) {
            Ok(streamed_execution) => {
                tokio::spawn(async move {
                    streamed_execution.forward_events(event_sender).await;
                });
            }
            Err(error) => {
                let failure_event = ExecutorEvent::workflow_failed(error.diagnostic(), None);
                let _ = event_sender.try_send(failure_event);
            }
        }

        event_receiver
    }

    pub fn start_streamed_execution(&self, request: ExecutionRequest) -> Result<StreamedExecutionSubscription, ExecutorError> {
        self.start_streamed_execution_with_request_cache_key(request)
    }

    pub fn start_streamed_execution_for_session(
        &self,
        mut request: ExecutionRequest,
        cache_session: AgentCacheSession,
    ) -> Result<StreamedExecutionSubscription, ExecutorError> {
        request.options.cache_key = Some(cache_session.identifier().to_string());

        self.start_streamed_execution_with_request_cache_key(request)
    }

    fn start_streamed_execution_with_request_cache_key(
        &self,
        request: ExecutionRequest,
    ) -> Result<StreamedExecutionSubscription, ExecutorError> {
        let subscription = self.streamed_executions.insert()?;
        let run_identifier = subscription.run_identifier.clone();
        let execution_registry = self.streamed_executions.clone();
        let supervisor_registry = self.streamed_executions.clone();
        let supervisor_run_identifier = run_identifier.clone();
        let model_provider = self.model_provider.clone();
        let max_concurrency = request.options.max_concurrency;
        let cache_options = self.cache_options_for_request(&request);
        let mcp_client_factory = Arc::clone(&self.mcp_client_factory);

        let execution_task = tokio::spawn(async move {
            execution_registry
                .run_streamed_execution(
                    request,
                    model_provider,
                    mcp_client_factory,
                    run_identifier,
                    max_concurrency,
                    cache_options,
                )
                .await;
        });

        tokio::spawn(async move {
            supervisor_registry
                .supervise_streamed_execution(supervisor_run_identifier, execution_task)
                .await;
        });

        Ok(subscription)
    }

    pub fn reconnect_streamed_execution(
        &self,
        run_identifier: &str,
        last_event_identifier: Option<u64>,
    ) -> Result<StreamedExecutionSubscription, ExecutorError> {
        self.streamed_executions.subscribe(run_identifier, last_event_identifier)
    }

    pub fn cancel_streamed_execution(&self, run_identifier: &str) -> CancellationTransition {
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
            if event_sender.send(sequenced_event.event.as_ref().clone()).await.is_err() {
                break;
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct SequencedExecutorEvent {
    pub event_identifier: u64,
    pub event: Arc<ExecutorEvent>,
    pub serialized_data: Arc<str>,
    pub maximum_sse_frame_bytes: usize,
    _retention_reservation: Option<Arc<RetainedEventReservation>>,
}

impl SequencedExecutorEvent {
    fn retained(
        event_identifier: u64,
        serialized_event: SerializedPublicExecutorEvent,
        retention_reservation: Arc<RetainedEventReservation>,
    ) -> Self {
        let (event, serialized_data, maximum_sse_frame_bytes) = serialized_event.into_parts();

        Self {
            event_identifier,
            event: Arc::new(event),
            serialized_data: Arc::from(serialized_data),
            maximum_sse_frame_bytes,
            _retention_reservation: Some(retention_reservation),
        }
    }

    fn synthetic(event_identifier: u64, event: ExecutorEvent) -> Self {
        let serialized_event = event
            .into_serialized_public()
            .expect("synthetic stream events must fit the public event contract");
        let (event, serialized_data, maximum_sse_frame_bytes) = serialized_event.into_parts();

        Self {
            event_identifier,
            event: Arc::new(event),
            serialized_data: Arc::from(serialized_data),
            maximum_sse_frame_bytes,
            _retention_reservation: None,
        }
    }
}

#[derive(Debug, Default)]
struct GlobalStreamRetentionBudget {
    state: Mutex<GlobalStreamRetentionBudgetState>,
}

impl GlobalStreamRetentionBudget {
    fn reserve_terminal_capacity(&self) -> bool {
        let mut state = self.lock_state();
        let Some(next_reserved_bytes) = state.terminal_reserve_bytes.checked_add(TERMINAL_EVENT_RESERVE_BYTES) else {
            return false;
        };
        let Some(next_total_bytes) = state.retained_event_bytes.checked_add(next_reserved_bytes) else {
            return false;
        };

        if next_total_bytes > MAX_RETAINED_EVENT_BYTES_GLOBAL {
            return false;
        }

        state.terminal_reserve_bytes = next_reserved_bytes;

        true
    }

    fn reserve_retained_event(&self, event_bytes: usize) -> bool {
        let mut state = self.lock_state();
        let Some(next_retained_bytes) = state.retained_event_bytes.checked_add(event_bytes) else {
            return false;
        };
        let Some(next_total_bytes) = next_retained_bytes.checked_add(state.terminal_reserve_bytes) else {
            return false;
        };

        if next_total_bytes > MAX_RETAINED_EVENT_BYTES_GLOBAL {
            return false;
        }

        state.retained_event_bytes = next_retained_bytes;

        true
    }

    fn consume_terminal_reserve(&self, event_bytes: usize) -> bool {
        let mut state = self.lock_state();

        if event_bytes > TERMINAL_EVENT_RESERVE_BYTES || state.terminal_reserve_bytes < TERMINAL_EVENT_RESERVE_BYTES {
            return false;
        }

        state.terminal_reserve_bytes -= TERMINAL_EVENT_RESERVE_BYTES;
        state.retained_event_bytes = state.retained_event_bytes.saturating_add(event_bytes);

        true
    }

    fn release_retained_event(&self, event_bytes: usize) {
        let mut state = self.lock_state();

        debug_assert!(state.retained_event_bytes >= event_bytes);
        state.retained_event_bytes = state.retained_event_bytes.saturating_sub(event_bytes);
    }

    fn release_terminal_reserve(&self) {
        let mut state = self.lock_state();

        debug_assert!(state.terminal_reserve_bytes >= TERMINAL_EVENT_RESERVE_BYTES);
        state.terminal_reserve_bytes = state.terminal_reserve_bytes.saturating_sub(TERMINAL_EVENT_RESERVE_BYTES);
    }

    fn lock_state(&self) -> MutexGuard<'_, GlobalStreamRetentionBudgetState> {
        self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    #[cfg(test)]
    fn retained_event_bytes(&self) -> usize {
        self.lock_state().retained_event_bytes
    }

    #[cfg(test)]
    fn terminal_reserve_bytes(&self) -> usize {
        self.lock_state().terminal_reserve_bytes
    }
}

#[derive(Debug, Default)]
struct GlobalStreamRetentionBudgetState {
    retained_event_bytes: usize,
    terminal_reserve_bytes: usize,
}

#[derive(Debug)]
struct RunStreamRetentionBudget {
    global_budget: Arc<GlobalStreamRetentionBudget>,
    state: Mutex<RunStreamRetentionBudgetState>,
}

impl RunStreamRetentionBudget {
    fn new(global_budget: Arc<GlobalStreamRetentionBudget>) -> Option<Arc<Self>> {
        if !global_budget.reserve_terminal_capacity() {
            return None;
        }

        Some(Arc::new(Self {
            global_budget,
            state: Mutex::new(RunStreamRetentionBudgetState {
                retained_event_bytes: 0,
                terminal_reserve_available: true,
            }),
        }))
    }

    fn reserve_nonterminal_event(self: &Arc<Self>, event_bytes: usize) -> Option<Arc<RetainedEventReservation>> {
        let mut state = self.lock_state();
        let next_retained_bytes = state.retained_event_bytes.checked_add(event_bytes)?;

        if next_retained_bytes > MAX_NONTERMINAL_EVENT_BYTES_PER_RUN || !self.global_budget.reserve_retained_event(event_bytes) {
            return None;
        }

        state.retained_event_bytes = next_retained_bytes;

        Some(Arc::new(RetainedEventReservation {
            event_bytes,
            run_budget: Arc::clone(self),
        }))
    }

    fn reserve_terminal_event(self: &Arc<Self>, event_bytes: usize) -> Option<Arc<RetainedEventReservation>> {
        let mut state = self.lock_state();

        if !state.terminal_reserve_available
            || state.retained_event_bytes.saturating_add(event_bytes) > MAX_RETAINED_EVENT_BYTES_PER_RUN
            || !self.global_budget.consume_terminal_reserve(event_bytes)
        {
            return None;
        }

        state.terminal_reserve_available = false;
        state.retained_event_bytes = state.retained_event_bytes.saturating_add(event_bytes);

        Some(Arc::new(RetainedEventReservation {
            event_bytes,
            run_budget: Arc::clone(self),
        }))
    }

    fn release_retained_event(&self, event_bytes: usize) {
        let mut state = self.lock_state();

        debug_assert!(state.retained_event_bytes >= event_bytes);
        state.retained_event_bytes = state.retained_event_bytes.saturating_sub(event_bytes);
        drop(state);

        self.global_budget.release_retained_event(event_bytes);
    }

    fn lock_state(&self) -> MutexGuard<'_, RunStreamRetentionBudgetState> {
        self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    #[cfg(test)]
    fn retained_event_bytes(&self) -> usize {
        self.lock_state().retained_event_bytes
    }
}

impl Drop for RunStreamRetentionBudget {
    fn drop(&mut self) {
        let state = self.state.get_mut().unwrap_or_else(std::sync::PoisonError::into_inner);

        if state.terminal_reserve_available {
            self.global_budget.release_terminal_reserve();
        }
    }
}

#[derive(Debug)]
struct RunStreamRetentionBudgetState {
    retained_event_bytes: usize,
    terminal_reserve_available: bool,
}

#[derive(Debug)]
struct RetainedEventReservation {
    event_bytes: usize,
    run_budget: Arc<RunStreamRetentionBudget>,
}

impl Drop for RetainedEventReservation {
    fn drop(&mut self) {
        self.run_budget.release_retained_event(self.event_bytes);
    }
}

#[derive(Debug, Clone)]
struct StreamedExecutionRegistry {
    executions: Arc<Mutex<HashMap<String, StreamedExecution>>>,
    expired_run_identifiers: Arc<Mutex<VecDeque<String>>>,
    completed_run_identifiers: Arc<Mutex<VecDeque<(Instant, String)>>>,
    retention_budget: Arc<GlobalStreamRetentionBudget>,
}

impl Default for StreamedExecutionRegistry {
    fn default() -> Self {
        Self {
            executions: Arc::new(Mutex::new(HashMap::new())),
            expired_run_identifiers: Arc::new(Mutex::new(VecDeque::new())),
            completed_run_identifiers: Arc::new(Mutex::new(VecDeque::new())),
            retention_budget: Arc::new(GlobalStreamRetentionBudget::default()),
        }
    }
}

impl StreamedExecutionRegistry {
    fn insert(&self) -> Result<StreamedExecutionSubscription, ExecutorError> {
        self.prune_expired();

        loop {
            let retention_budget = match RunStreamRetentionBudget::new(Arc::clone(&self.retention_budget)) {
                Some(retention_budget) => retention_budget,
                None if self.evict_oldest_completed_execution() => continue,
                None => return Err(ExecutorError::stream_capacity_exceeded()),
            };
            let run_identifier = Uuid::new_v4().simple().to_string();
            let streamed_execution = StreamedExecution::new(run_identifier.clone(), retention_budget);
            let subscription = streamed_execution.initial_subscription();
            let mut executions = self.lock_executions();

            if executions.contains_key(&run_identifier) {
                continue;
            }

            executions.insert(run_identifier, streamed_execution);

            return Ok(subscription);
        }
    }

    fn attach_abort_handle(&self, run_identifier: &str, abort_handle: AbortHandle) {
        let streamed_execution = self.lock_executions().get(run_identifier).cloned();

        if let Some(streamed_execution) = streamed_execution {
            streamed_execution.attach_abort_handle(abort_handle);
        }
    }

    fn subscribe(&self, run_identifier: &str, last_event_identifier: Option<u64>) -> Result<StreamedExecutionSubscription, ExecutorError> {
        self.prune_expired();
        let streamed_execution = self.lock_executions().get(run_identifier).cloned();

        if let Some(streamed_execution) = streamed_execution {
            return streamed_execution.subscribe(last_event_identifier);
        }

        if self
            .lock_expired_run_identifiers()
            .iter()
            .any(|expired_run_identifier| expired_run_identifier == run_identifier)
        {
            return Err(ExecutorError::stream_expired());
        }

        Err(ExecutorError::unknown_run())
    }

    fn record_event(&self, run_identifier: &str, event: ExecutorEvent) {
        let event_kind = event.kind.as_str();
        let streamed_execution = self.lock_executions().get(run_identifier).cloned();
        let Some(streamed_execution) = streamed_execution else {
            log::warn!("failed to record executor event: kind={event_kind}, reason=run is not registered");

            return;
        };

        if streamed_execution.record_event(event) {
            self.retain_completed_execution(run_identifier.to_string());
        }
    }

    fn claim_terminal(&self, run_identifier: &str, event: ExecutorEvent) -> bool {
        self.lock_executions()
            .get(run_identifier)
            .is_some_and(|streamed_execution| streamed_execution.claim_terminal(event))
    }

    fn replace_pending_terminal_with_failure(&self, run_identifier: &str, event: ExecutorEvent) {
        if let Some(streamed_execution) = self.lock_executions().get(run_identifier) {
            streamed_execution.replace_pending_terminal_with_failure(event);
        }
    }

    fn publish_pending_terminal(&self, run_identifier: &str) -> bool {
        let published = self
            .lock_executions()
            .get(run_identifier)
            .is_some_and(StreamedExecution::publish_pending_terminal);

        if published {
            self.retain_completed_execution(run_identifier.to_string());
        }

        published
    }

    fn is_terminal(&self, run_identifier: &str) -> bool {
        self.lock_executions()
            .get(run_identifier)
            .is_some_and(StreamedExecution::is_terminal)
    }

    fn is_cancellation_requested(&self, run_identifier: &str) -> bool {
        self.lock_executions()
            .get(run_identifier)
            .is_some_and(StreamedExecution::is_cancellation_requested)
    }

    fn retain_completed_execution(&self, run_identifier: String) {
        let evicted_run_identifier = {
            let mut completed_run_identifiers = self.lock_completed_run_identifiers();

            completed_run_identifiers.push_back((Instant::now(), run_identifier));

            if completed_run_identifiers.len() > MAX_RETAINED_EXECUTIONS {
                completed_run_identifiers
                    .pop_front()
                    .map(|(_completed_at, run_identifier)| run_identifier)
            } else {
                None
            }
        };

        if let Some(evicted_run_identifier) = evicted_run_identifier {
            self.remove(&evicted_run_identifier);
        }
    }

    fn evict_oldest_completed_execution(&self) -> bool {
        let oldest_run_identifier = self
            .lock_completed_run_identifiers()
            .pop_front()
            .map(|(_completed_at, run_identifier)| run_identifier);
        let Some(oldest_run_identifier) = oldest_run_identifier else {
            return false;
        };

        self.remove(&oldest_run_identifier);

        true
    }

    fn prune_expired(&self) {
        let expired_run_identifiers = {
            let mut completed_run_identifiers = self.lock_completed_run_identifiers();
            let mut expired_run_identifiers = Vec::new();

            while completed_run_identifiers
                .front()
                .is_some_and(|(completed_at, _run_identifier)| completed_at.elapsed() >= COMPLETED_STREAM_RETENTION)
            {
                if let Some((_completed_at, run_identifier)) = completed_run_identifiers.pop_front() {
                    expired_run_identifiers.push(run_identifier);
                }
            }

            expired_run_identifiers
        };

        for expired_run_identifier in expired_run_identifiers {
            self.remove(&expired_run_identifier);
        }
    }

    fn remove(&self, run_identifier: &str) {
        if self.lock_executions().remove(run_identifier).is_none() {
            return;
        }

        self.lock_completed_run_identifiers()
            .retain(|(_completed_at, completed_run_identifier)| completed_run_identifier != run_identifier);

        let mut expired_run_identifiers = self.lock_expired_run_identifiers();

        expired_run_identifiers.push_back(run_identifier.to_string());

        while expired_run_identifiers.len() > MAX_EXPIRED_RUN_IDENTIFIERS {
            expired_run_identifiers.pop_front();
        }
    }

    fn cancel(&self, run_identifier: &str) -> CancellationTransition {
        self.prune_expired();

        let streamed_execution = self.lock_executions().get(run_identifier).cloned();
        let Some(streamed_execution) = streamed_execution else {
            return CancellationTransition::UnknownRun;
        };
        streamed_execution.cancel()
    }

    async fn run_streamed_execution<ModelProviderType>(
        self,
        request: ExecutionRequest,
        model_provider: ModelProviderType,
        mcp_client_factory: Arc<dyn McpClientFactory>,
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
        let terminal_execution = self
            .lock_executions()
            .get(&run_identifier)
            .cloned()
            .expect("streamed execution must remain registered while it is running");
        let workflow_event_sender = event_sender.clone();
        let workflow_task = tokio::spawn(async move {
            match run_streamed_execution(
                request,
                model_provider,
                mcp_client_factory,
                workflow_event_sender,
                max_concurrency,
                cache_options,
            )
            .await
            {
                Ok(output) => {
                    terminal_execution.claim_terminal(ExecutorEvent::workflow_completed(output, workflow_started_at.elapsed()));

                    Ok(())
                }
                Err(error) => {
                    terminal_execution.claim_terminal(ExecutorEvent::workflow_failed(
                        error.diagnostic(),
                        Some(workflow_started_at.elapsed()),
                    ));

                    Err(error)
                }
            }
        });

        self.attach_abort_handle(&run_identifier, workflow_task.abort_handle());

        let execution_result = match workflow_task.await {
            Ok(execution_result) => Some(execution_result),
            Err(join_error) if join_error.is_cancelled() && self.is_cancellation_requested(&run_identifier) => None,
            Err(join_error) => {
                let error = Self::join_error(join_error, "streamed workflow execution");

                self.claim_terminal(
                    &run_identifier,
                    ExecutorEvent::workflow_failed(error.diagnostic(), Some(workflow_started_at.elapsed())),
                );

                Some(Err(error))
            }
        };

        if let Some(Err(error)) = &execution_result {
            let diagnostic = error.diagnostic();

            log::error!(
                "streamed workflow execution failed: code={:?}, stage={:?}",
                diagnostic.code,
                diagnostic.stage
            );
        }

        drop(event_sender);

        if let Err(join_error) = event_recorder.await {
            let error = Self::join_error(join_error, "stream event recorder");

            self.replace_pending_terminal_with_failure(
                &run_identifier,
                ExecutorEvent::workflow_failed(error.diagnostic(), Some(workflow_started_at.elapsed())),
            );
        }

        if self.publish_pending_terminal(&run_identifier) || self.is_terminal(&run_identifier) {
            return;
        }

        self.record_event(
            &run_identifier,
            ExecutorEvent::workflow_failed(
                ExecutorError::internal_panic("streamed execution ended without a reserved terminal outcome").diagnostic(),
                Some(workflow_started_at.elapsed()),
            ),
        );
    }

    async fn supervise_streamed_execution(&self, run_identifier: String, execution_task: JoinHandle<()>) {
        let Err(join_error) = execution_task.await else {
            return;
        };

        if join_error.is_cancelled() {
            return;
        }

        let error = Self::join_error(join_error, "streamed execution");

        self.replace_pending_terminal_with_failure(&run_identifier, ExecutorEvent::workflow_failed(error.diagnostic(), None));

        if !self.publish_pending_terminal(&run_identifier) && !self.is_terminal(&run_identifier) {
            self.record_event(&run_identifier, ExecutorEvent::workflow_failed(error.diagnostic(), None));
        }
    }

    fn join_error(join_error: JoinError, task_name: &str) -> ExecutorError {
        if !join_error.is_panic() {
            return ExecutorError::internal_with_source(format!("{task_name} task failed: {join_error}"), join_error);
        }

        ExecutorError::internal_panic(format!("{task_name} task panicked"))
    }

    fn lock_executions(&self) -> MutexGuard<'_, HashMap<String, StreamedExecution>> {
        self.executions.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn lock_expired_run_identifiers(&self) -> MutexGuard<'_, VecDeque<String>> {
        self.expired_run_identifiers
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn lock_completed_run_identifiers(&self) -> MutexGuard<'_, VecDeque<(Instant, String)>> {
        self.completed_run_identifiers
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

#[derive(Debug, Clone)]
struct StreamedExecution {
    run_identifier: String,
    started_at: Instant,
    state: Arc<Mutex<StreamedExecutionState>>,
}

impl StreamedExecution {
    fn new(run_identifier: String, retention_budget: Arc<RunStreamRetentionBudget>) -> Self {
        Self {
            run_identifier,
            started_at: Instant::now(),
            state: Arc::new(Mutex::new(StreamedExecutionState::new(retention_budget))),
        }
    }

    fn initial_subscription(&self) -> StreamedExecutionSubscription {
        let (event_sender, event_receiver) = mpsc::channel(STREAM_SUBSCRIBER_CAPACITY);

        self.lock_state().subscribers.push(StreamedExecutionSubscriber {
            sender: event_sender,
            last_delivered_event_identifier: 0,
        });

        StreamedExecutionSubscription {
            run_identifier: self.run_identifier.clone(),
            receiver: event_receiver,
        }
    }

    fn attach_abort_handle(&self, abort_handle: AbortHandle) {
        let mut state = self.lock_state();

        if state.has_terminal_outcome() {
            if state.cancellation_requested {
                drop(state);
                abort_handle.abort();
            }

            return;
        }

        state.abort_handle = Some(abort_handle);
    }

    fn subscribe(&self, last_event_identifier: Option<u64>) -> Result<StreamedExecutionSubscription, ExecutorError> {
        let mut state = self.lock_state();

        if let Some(requested_event_identifier) = last_event_identifier {
            if requested_event_identifier > state.next_event_identifier {
                return Err(self.cursor_ahead_error(requested_event_identifier, state.next_event_identifier));
            }
        }

        let missed_events = state
            .events_after(last_event_identifier)
            .map_err(|oldest_available| ExecutorError::stream_gap(last_event_identifier, oldest_available))?;
        let (event_sender, event_receiver) = mpsc::channel(STREAM_SUBSCRIBER_CAPACITY);
        let mut last_delivered_event_identifier = last_event_identifier.unwrap_or(0);

        for event in missed_events {
            last_delivered_event_identifier = event.event_identifier;
            event_sender
                .try_send(event)
                .map_err(|error| ExecutorError::internal_with_source("failed to prepare replay subscription", error))?;
        }

        if !state.is_terminal() {
            state.subscribers.push(StreamedExecutionSubscriber {
                sender: event_sender,
                last_delivered_event_identifier,
            });
        }

        Ok(StreamedExecutionSubscription {
            run_identifier: self.run_identifier.clone(),
            receiver: event_receiver,
        })
    }

    fn record_event(&self, event: ExecutorEvent) -> bool {
        self.lock_state().record_event(event)
    }

    fn claim_terminal(&self, event: ExecutorEvent) -> bool {
        self.lock_state().claim_terminal(event)
    }

    fn replace_pending_terminal_with_failure(&self, event: ExecutorEvent) {
        self.lock_state().replace_pending_terminal_with_failure(event);
    }

    fn publish_pending_terminal(&self) -> bool {
        self.lock_state().publish_pending_terminal()
    }

    fn is_terminal(&self) -> bool {
        self.lock_state().is_terminal()
    }

    fn cancel(&self) -> CancellationTransition {
        let mut state = self.lock_state();

        if state.is_terminal() {
            return CancellationTransition::AlreadyTerminal;
        }

        if state.cancellation_requested {
            return CancellationTransition::AlreadyRequested;
        }

        if state.has_terminal_outcome() {
            return CancellationTransition::AlreadyTerminal;
        }

        state.cancellation_requested = true;
        let claimed_cancellation = state.claim_terminal(ExecutorEvent::workflow_cancelled(
            ExecutorError::cancellation_diagnostic(),
            Some(self.started_at.elapsed()),
        ));
        debug_assert!(claimed_cancellation, "unclaimed cancellation must reserve the terminal outcome");
        let abort_handle = state.abort_handle.clone();
        drop(state);

        if let Some(abort_handle) = abort_handle {
            abort_handle.abort();
        }

        CancellationTransition::Accepted
    }

    fn is_cancellation_requested(&self) -> bool {
        self.lock_state().cancellation_requested
    }

    fn cursor_ahead_error(&self, requested_event_identifier: u64, latest_event_identifier: u64) -> ExecutorError {
        ExecutorError::Diagnostic {
            diagnostic: Box::new(ExecutorDiagnostic::error(
                ExecutorDiagnosticCode::StreamGap,
                ExecutorStage::Stream,
                format!("replay cursor {requested_event_identifier} exceeds latest event identifier {latest_event_identifier}"),
                ExecutorDiagnosticSubject::Stream {
                    requested_after: Some(requested_event_identifier),
                    oldest_available: Some(latest_event_identifier.saturating_add(1)),
                },
            )),
            source: None,
        }
    }

    fn lock_state(&self) -> MutexGuard<'_, StreamedExecutionState> {
        self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

#[derive(Debug)]
struct StreamedExecutionSubscriber {
    sender: mpsc::Sender<SequencedExecutorEvent>,
    last_delivered_event_identifier: u64,
}

#[derive(Debug)]
struct StreamedExecutionState {
    events: VecDeque<SequencedExecutorEvent>,
    subscribers: Vec<StreamedExecutionSubscriber>,
    next_event_identifier: u64,
    terminal_event_identifier: Option<u64>,
    pending_terminal_event: Option<ExecutorEvent>,
    abort_handle: Option<AbortHandle>,
    cancellation_requested: bool,
    retained_history_bytes: usize,
    retention_budget: Arc<RunStreamRetentionBudget>,
}

impl StreamedExecutionState {
    fn new(retention_budget: Arc<RunStreamRetentionBudget>) -> Self {
        Self {
            events: VecDeque::new(),
            subscribers: Vec::new(),
            next_event_identifier: 0,
            terminal_event_identifier: None,
            pending_terminal_event: None,
            abort_handle: None,
            cancellation_requested: false,
            retained_history_bytes: 0,
            retention_budget,
        }
    }

    fn events_after(&self, last_event_identifier: Option<u64>) -> Result<Vec<SequencedExecutorEvent>, u64> {
        let requested_event_identifier = last_event_identifier.unwrap_or(0);

        if let Some(oldest_event) = self.events.front() {
            if requested_event_identifier.saturating_add(1) < oldest_event.event_identifier {
                return Err(oldest_event.event_identifier);
            }
        }

        Ok(self
            .events
            .iter()
            .filter(|event| event.event_identifier > requested_event_identifier)
            .cloned()
            .collect())
    }

    fn record_event(&mut self, event: ExecutorEvent) -> bool {
        if event.is_terminal() {
            if !self.claim_terminal(event) {
                return false;
            }

            return self.publish_pending_terminal();
        }

        if self.is_terminal() {
            return false;
        }

        self.append_event(event)
    }

    fn claim_terminal(&mut self, event: ExecutorEvent) -> bool {
        debug_assert!(
            event.is_terminal(),
            "only workflow terminal events may reserve the terminal outcome"
        );

        if self.has_terminal_outcome() {
            return false;
        }

        self.pending_terminal_event = Some(event);

        true
    }

    fn replace_pending_terminal_with_failure(&mut self, event: ExecutorEvent) {
        debug_assert!(event.is_terminal(), "terminal replacement must be a workflow terminal event");

        if self.cancellation_requested || self.is_terminal() {
            return;
        }

        self.pending_terminal_event = Some(event);
    }

    fn publish_pending_terminal(&mut self) -> bool {
        if self.is_terminal() {
            return false;
        }

        let Some(event) = self.pending_terminal_event.take() else {
            return false;
        };

        self.append_event(event)
    }

    fn append_event(&mut self, event: ExecutorEvent) -> bool {
        let (mut serialized_event, mut abort_execution) = Self::serialize_event_or_failure(event);
        let mut is_terminal = serialized_event.event().is_terminal();
        let mut retention_reservation = if is_terminal {
            self.retention_budget
                .reserve_terminal_event(serialized_event.maximum_sse_frame_bytes())
        } else {
            self.reserve_nonterminal_event(serialized_event.maximum_sse_frame_bytes())
        };

        if retention_reservation.is_none() && !is_terminal {
            serialized_event = Self::serialize_stream_capacity_failure();
            abort_execution = true;
            is_terminal = true;
            retention_reservation = self
                .retention_budget
                .reserve_terminal_event(serialized_event.maximum_sse_frame_bytes());
        }

        let retention_reservation =
            retention_reservation.expect("each active stream reserves enough capacity for one bounded terminal event");
        self.next_event_identifier = self.next_event_identifier.saturating_add(1);
        let sequenced_event = SequencedExecutorEvent::retained(self.next_event_identifier, serialized_event, retention_reservation);

        self.retained_history_bytes = self.retained_history_bytes.saturating_add(sequenced_event.maximum_sse_frame_bytes);
        self.events.push_back(sequenced_event.clone());
        self.enforce_history_limits();

        self.subscribers
            .retain_mut(|subscriber| match subscriber.sender.try_send(sequenced_event.clone()) {
                Ok(()) => {
                    subscriber.last_delivered_event_identifier = sequenced_event.event_identifier;

                    true
                }
                Err(mpsc::error::TrySendError::Closed(_)) => false,
                Err(mpsc::error::TrySendError::Full(_)) => {
                    let gap_cursor = subscriber.last_delivered_event_identifier;
                    let gap_event = SequencedExecutorEvent::synthetic(
                        gap_cursor,
                        ExecutorEvent::stream_gap(
                            ExecutorError::stream_gap(Some(gap_cursor), sequenced_event.event_identifier).diagnostic(),
                        ),
                    );
                    let event_sender = subscriber.sender.clone();

                    tokio::spawn(async move {
                        if event_sender.send(gap_event).await.is_err() {
                            log::debug!("slow stream subscriber closed before receiving its gap event");
                        }
                    });

                    false
                }
            });

        if is_terminal {
            self.terminal_event_identifier = Some(sequenced_event.event_identifier);
            let abort_handle = if abort_execution {
                self.abort_handle.take()
            } else {
                self.abort_handle = None;

                None
            };
            self.subscribers.clear();

            if let Some(abort_handle) = abort_handle {
                abort_handle.abort();
            }
        }

        is_terminal
    }

    fn serialize_event_or_failure(event: ExecutorEvent) -> (SerializedPublicExecutorEvent, bool) {
        match event.into_serialized_public() {
            Ok(serialized_event) => (serialized_event, false),
            Err(PublicEventSerializationError::TooLarge {
                actual_bytes,
                maximum_bytes,
            }) => (
                Self::serialize_failure(ExecutorDiagnostic::event_too_large(actual_bytes, maximum_bytes)),
                true,
            ),
            Err(PublicEventSerializationError::Serialization { message }) => (
                Self::serialize_failure(ExecutorDiagnostic::error(
                    ExecutorDiagnosticCode::InternalError,
                    ExecutorStage::Internal,
                    message,
                    ExecutorDiagnosticSubject::Workflow,
                )),
                true,
            ),
        }
    }

    fn serialize_stream_capacity_failure() -> SerializedPublicExecutorEvent {
        Self::serialize_failure(ExecutorDiagnostic::stream_capacity_exceeded())
    }

    fn serialize_failure(diagnostic: ExecutorDiagnostic) -> SerializedPublicExecutorEvent {
        ExecutorEvent::workflow_failed(diagnostic, None)
            .into_serialized_public()
            .expect("typed public event failures must fit the public event contract")
    }

    fn reserve_nonterminal_event(&mut self, event_bytes: usize) -> Option<Arc<RetainedEventReservation>> {
        loop {
            if let Some(retention_reservation) = self.retention_budget.reserve_nonterminal_event(event_bytes) {
                return Some(retention_reservation);
            }

            let evicted_event = self.events.pop_front()?;
            self.retained_history_bytes = self.retained_history_bytes.saturating_sub(evicted_event.maximum_sse_frame_bytes);
        }
    }

    fn enforce_history_limits(&mut self) {
        while self.events.len() > MAX_RETAINED_EVENTS_PER_RUN || self.retained_history_bytes > MAX_RETAINED_EVENT_BYTES_PER_RUN {
            let Some(evicted_event) = self.events.pop_front() else {
                break;
            };

            self.retained_history_bytes = self.retained_history_bytes.saturating_sub(evicted_event.maximum_sse_frame_bytes);
        }
    }

    fn has_terminal_outcome(&self) -> bool {
        self.pending_terminal_event.is_some() || self.is_terminal()
    }

    fn is_terminal(&self) -> bool {
        self.terminal_event_identifier.is_some()
    }
}

async fn run_streamed_execution<ModelProviderType>(
    request: ExecutionRequest,
    model_provider: ModelProviderType,
    mcp_client_factory: Arc<dyn McpClientFactory>,
    event_sender: mpsc::Sender<ExecutorEvent>,
    max_concurrency: usize,
    cache_options: AgentCacheOptions,
) -> Result<serde_json::Value, ExecutorError>
where
    ModelProviderType: ModelProvider + Clone + Send + Sync + 'static,
{
    let workflow_source = request.resolved_workflow_source().map_err(ExecutorError::invalid_input)?;

    event_sender.send_observed(ExecutorEvent::workflow_started()).await;

    log::info!("starting streamed workflow execution");
    log::debug!(
        "resolved workflow source for streamed execution: bytes={}, input_provided={}, secrets_provided={}, max_concurrency={}",
        workflow_source.len(),
        !request.input.is_null(),
        !request.secrets.is_null(),
        max_concurrency
    );

    let input = request.input;
    let secrets = request.secrets;
    let planning_event_sender = event_sender.clone();
    let (executor, input, secrets) = tokio::task::spawn_blocking(move || {
        let executor = WorkflowExecutor::from_source_with_runtime_values_and_event_sender_and_mcp_client_factory(
            &workflow_source,
            &input,
            &secrets,
            Some(&planning_event_sender),
            mcp_client_factory.as_ref(),
        )?;

        Ok::<_, ExecutorError>((executor, input, secrets))
    })
    .await
    .map_err(|join_error| ExecutorError::internal_panic(format!("workflow planning task failed: {join_error}")))??;
    let agent_execution_order = executor.agent_execution_order();
    let planned_steps = executor.planned_execution_steps(&input, &secrets, max_concurrency)?;
    let mcp_imports = executor
        .mcp_imports()
        .iter()
        .map(|import| superwire_protocol::event::PlannedMcpImportEvent {
            name: import.name.clone(),
            kind: (&import.kind).into(),
            server_name: import.server_name.clone(),
            item_name: import.item_name.clone(),
        })
        .collect::<Vec<_>>();

    log::debug!("streamed workflow planned with agent order: {agent_execution_order:?}");

    event_sender
        .send_observed(ExecutorEvent::workflow_planned(agent_execution_order, mcp_imports, planned_steps))
        .await;

    let output = executor
        .execute_with_cache(input, secrets, &model_provider, Some(event_sender), max_concurrency, cache_options)
        .await?;

    log::info!("streamed workflow execution completed");

    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::support::{request, TestModelProvider};
    use superwire_macros::workflow_source;
    use superwire_mcp::{McpClientBackend, McpError, McpServerConfig, McpServerLock};
    use tokio::sync::oneshot;

    use serde_json::json;
    use superwire_protocol::event::{ExecutorDiagnosticCode, ExecutorEventKind};

    fn agent_started_event(agent_index: usize) -> ExecutorEvent {
        ExecutorEvent::agent_started(
            format!("agent_{agent_index}"),
            "test-model".to_string(),
            vec!["internal:finalize".to_string()],
            None,
        )
    }

    fn streamed_execution(run_identifier: String) -> StreamedExecution {
        streamed_execution_with_global_budget(run_identifier, Arc::new(GlobalStreamRetentionBudget::default()))
    }

    fn streamed_execution_with_global_budget(run_identifier: String, global_budget: Arc<GlobalStreamRetentionBudget>) -> StreamedExecution {
        let retention_budget = RunStreamRetentionBudget::new(global_budget).expect("test stream should reserve terminal event capacity");

        StreamedExecution::new(run_identifier, retention_budget)
    }

    #[test]
    fn streamed_execution_retains_repeated_events_with_distinct_identities() {
        let streamed_execution = streamed_execution("test-run".to_string());
        let mut stream_subscription = streamed_execution.initial_subscription();
        let event = agent_started_event(1);

        assert!(!streamed_execution.record_event(event.clone()));
        assert!(!streamed_execution.record_event(event));
        assert!(streamed_execution.record_event(ExecutorEvent::workflow_completed(
            json!({ "status": "done" }),
            Duration::from_millis(10),
        )));

        let first_event = stream_subscription.receiver.try_recv().expect("first event should be streamed");
        let second_event = stream_subscription.receiver.try_recv().expect("repeated event should be streamed");
        let terminal_event = stream_subscription.receiver.try_recv().expect("terminal event should be streamed");

        assert_eq!(first_event.event_identifier, 1);
        assert_eq!(second_event.event_identifier, 2);
        assert_eq!(terminal_event.event_identifier, 3);
        assert_eq!(first_event.event.kind, ExecutorEventKind::AgentStarted);
        assert_eq!(second_event.event.kind, ExecutorEventKind::AgentStarted);
        assert_eq!(terminal_event.event.kind, ExecutorEventKind::WorkflowCompleted);
        assert!(stream_subscription.receiver.try_recv().is_err());
        assert_eq!(streamed_execution.lock_state().events.len(), 3);
    }

    #[test]
    fn live_and_replay_subscribers_share_public_event_payloads() {
        const SECRET_SENTINEL: &str = "superwire-secret-sentinel";
        let streamed_execution = streamed_execution("test-run".to_string());
        let mut live_subscription = streamed_execution.initial_subscription();

        assert!(!streamed_execution.record_event(ExecutorEvent::tool_call_started(
            "writer".to_string(),
            "search".to_string(),
            &json!({
                "query": SECRET_SENTINEL,
                "limit": 5,
            }),
        )));

        let live_event = live_subscription.receiver.try_recv().expect("live event should be streamed");
        let mut replay_subscription = streamed_execution.subscribe(Some(0)).expect("retained event should be replayable");
        let replayed_event = replay_subscription.receiver.try_recv().expect("retained event should be replayed");
        let serialized_event = serde_json::to_string(live_event.event.as_ref()).expect("event should serialize");

        assert!(Arc::ptr_eq(&live_event.event, &replayed_event.event));
        assert!(!serialized_event.contains(SECRET_SENTINEL));
        assert!(!serialized_event.contains("\"arguments\""));
        assert!(serialized_event.contains("\"argument_names\":[\"limit\",\"query\"]"));
    }

    #[test]
    fn cancellation_records_exactly_one_terminal_transition() {
        let streamed_execution = streamed_execution("test-run".to_string());
        let mut stream_subscription = streamed_execution.initial_subscription();

        assert_eq!(streamed_execution.cancel(), CancellationTransition::Accepted);
        assert_eq!(streamed_execution.cancel(), CancellationTransition::AlreadyRequested);
        assert!(streamed_execution.publish_pending_terminal());
        assert_eq!(streamed_execution.cancel(), CancellationTransition::AlreadyTerminal);

        let terminal_event = stream_subscription
            .receiver
            .try_recv()
            .expect("cancellation event should be streamed");

        assert_eq!(terminal_event.event.kind, ExecutorEventKind::WorkflowCancelled);
        assert_eq!(
            terminal_event
                .event
                .diagnostic
                .as_ref()
                .expect("cancellation diagnostic should be present")
                .code,
            ExecutorDiagnosticCode::Cancelled
        );
        assert!(stream_subscription.receiver.try_recv().is_err());
        assert_eq!(streamed_execution.lock_state().events.len(), 1);
    }

    #[tokio::test]
    async fn concurrent_cancellation_accepts_exactly_one_transition() {
        const CANCELLATION_COUNT: usize = 8;

        let streamed_execution = streamed_execution("test-run".to_string());
        let cancellation_barrier = Arc::new(tokio::sync::Barrier::new(CANCELLATION_COUNT));
        let mut cancellation_tasks = Vec::new();

        for _cancellation_index in 0..CANCELLATION_COUNT {
            let streamed_execution = streamed_execution.clone();
            let cancellation_barrier = cancellation_barrier.clone();

            cancellation_tasks.push(tokio::spawn(async move {
                cancellation_barrier.wait().await;
                streamed_execution.cancel()
            }));
        }

        let mut accepted_count = 0;
        let mut already_requested_count = 0;

        for cancellation_task in cancellation_tasks {
            match cancellation_task.await.expect("cancellation task should complete") {
                CancellationTransition::Accepted => accepted_count += 1,
                CancellationTransition::AlreadyRequested => already_requested_count += 1,
                CancellationTransition::AlreadyTerminal => panic!("execution is not terminal yet"),
                CancellationTransition::UnknownRun => panic!("registered execution should be known"),
            }
        }

        assert_eq!(accepted_count, 1);
        assert_eq!(already_requested_count, CANCELLATION_COUNT - 1);
        assert!(streamed_execution.is_cancellation_requested());
        assert!(streamed_execution.publish_pending_terminal());
        assert_eq!(streamed_execution.lock_state().events.len(), 1);
    }

    #[tokio::test]
    async fn completion_and_cancellation_race_linearizes_one_terminal_outcome() {
        const RACE_COUNT: usize = 128;

        for race_index in 0..RACE_COUNT {
            let streamed_execution = streamed_execution(format!("race-run-{race_index}"));
            let race_barrier = Arc::new(tokio::sync::Barrier::new(2));
            let completion_execution = streamed_execution.clone();
            let completion_barrier = race_barrier.clone();
            let completion_task = tokio::spawn(async move {
                completion_barrier.wait().await;

                completion_execution.claim_terminal(ExecutorEvent::workflow_completed(
                    json!({ "race": race_index }),
                    Duration::from_millis(1),
                ))
            });
            let cancellation_execution = streamed_execution.clone();
            let cancellation_task = tokio::spawn(async move {
                race_barrier.wait().await;

                cancellation_execution.cancel()
            });
            let completion_claimed = completion_task.await.expect("completion task should finish");
            let cancellation_transition = cancellation_task.await.expect("cancellation task should finish");

            match cancellation_transition {
                CancellationTransition::Accepted => assert!(!completion_claimed),
                CancellationTransition::AlreadyTerminal => assert!(completion_claimed),
                CancellationTransition::AlreadyRequested | CancellationTransition::UnknownRun => {
                    panic!("single cancellation must either win or observe completed terminal state")
                }
            }

            assert!(streamed_execution.publish_pending_terminal());

            let terminal_kind = streamed_execution
                .lock_state()
                .events
                .back()
                .map(|event| event.event.kind)
                .expect("race must publish a terminal event");

            if cancellation_transition == CancellationTransition::Accepted {
                assert_eq!(terminal_kind, ExecutorEventKind::WorkflowCancelled);
            } else {
                assert_eq!(terminal_kind, ExecutorEventKind::WorkflowCompleted);
            }
        }
    }

    #[test]
    fn reconnect_rejects_history_older_than_retention_window() {
        let streamed_execution = streamed_execution("test-run".to_string());

        for agent_index in 0..=MAX_RETAINED_EVENTS_PER_RUN {
            assert!(!streamed_execution.record_event(agent_started_event(agent_index)));
        }

        let error = streamed_execution
            .subscribe(Some(0))
            .expect_err("history before the oldest retained event should be rejected");

        assert_eq!(error.diagnostic().code, ExecutorDiagnosticCode::StreamGap);
        assert_eq!(streamed_execution.lock_state().events.len(), MAX_RETAINED_EVENTS_PER_RUN);
    }

    #[test]
    fn retained_history_evicts_by_serialized_bytes_with_honest_gap_cursor() {
        const LARGE_AGENT_NAME_BYTES: usize = 200 * 1024;
        const EVENT_COUNT: usize = 100;

        let streamed_execution = streamed_execution("byte-bounded-run".to_string());

        for event_index in 0..EVENT_COUNT {
            let event = ExecutorEvent::agent_started(
                format!("{}-{event_index}", "a".repeat(LARGE_AGENT_NAME_BYTES)),
                "test-model".to_string(),
                Vec::new(),
                None,
            );

            assert!(!streamed_execution.record_event(event));
        }

        let state = streamed_execution.lock_state();
        let retained_event_count = state.events.len();
        let oldest_available = state
            .events
            .front()
            .map(|event| event.event_identifier)
            .expect("byte-bounded history should retain recent events");

        assert!(retained_event_count < EVENT_COUNT);
        assert!(retained_event_count < MAX_RETAINED_EVENTS_PER_RUN);
        assert!(state.retained_history_bytes <= MAX_NONTERMINAL_EVENT_BYTES_PER_RUN);
        assert!(oldest_available > 1);
        drop(state);

        let diagnostic = streamed_execution
            .subscribe(Some(0))
            .expect_err("evicted byte history should reject the stale cursor")
            .diagnostic();

        assert_eq!(diagnostic.code, ExecutorDiagnosticCode::StreamGap);
        assert_eq!(
            diagnostic.subject,
            ExecutorDiagnosticSubject::Stream {
                requested_after: Some(0),
                oldest_available: Some(oldest_available),
            }
        );
    }

    #[test]
    fn reconnect_rejects_cursor_ahead_of_latest_event() {
        let streamed_execution = streamed_execution("test-run".to_string());

        assert!(!streamed_execution.record_event(agent_started_event(1)));

        let error = streamed_execution
            .subscribe(Some(2))
            .expect_err("cursor ahead of the run must be rejected");

        assert_eq!(error.diagnostic().code, ExecutorDiagnosticCode::StreamGap);
        assert!(error.diagnostic().message.contains("exceeds latest event identifier 1"));
    }

    #[test]
    fn reconnect_distinguishes_expired_and_unknown_runs() {
        let registry = StreamedExecutionRegistry::default();
        let subscription = registry.insert().expect("stream should reserve retention capacity");
        let run_identifier = subscription.run_identifier;

        registry.record_event(
            &run_identifier,
            ExecutorEvent::workflow_completed(json!({ "status": "done" }), Duration::from_millis(1)),
        );

        {
            let mut completed_run_identifiers = registry.lock_completed_run_identifiers();
            let (completed_at, _completed_run_identifier) = completed_run_identifiers
                .front_mut()
                .expect("completed execution should be retained");

            *completed_at = Instant::now()
                .checked_sub(COMPLETED_STREAM_RETENTION + Duration::from_millis(1))
                .expect("completed timestamp should be representable");
        }

        let expired_error = registry
            .subscribe(&run_identifier, None)
            .expect_err("stale run should be reported as expired");
        let unknown_error = registry
            .subscribe("unknown-run", None)
            .expect_err("unregistered run should be reported as unknown");

        assert_eq!(expired_error.diagnostic().code, ExecutorDiagnosticCode::StreamExpired);
        assert_eq!(unknown_error.diagnostic().code, ExecutorDiagnosticCode::UnknownRun);
    }

    #[test]
    fn completed_execution_retention_is_count_bounded() {
        let registry = StreamedExecutionRegistry::default();
        let mut first_run_identifier = None;

        for execution_index in 0..=MAX_RETAINED_EXECUTIONS {
            let subscription = registry.insert().expect("stream should reserve retention capacity");
            let run_identifier = subscription.run_identifier;

            if execution_index == 0 {
                first_run_identifier = Some(run_identifier.clone());
            }

            registry.record_event(
                &run_identifier,
                ExecutorEvent::workflow_completed(json!({ "status": "done" }), Duration::from_millis(1)),
            );
        }

        let first_run_identifier = first_run_identifier.expect("first run identifier should be captured");
        let first_run_error = registry
            .subscribe(&first_run_identifier, None)
            .expect_err("oldest completed run should be evicted");

        assert_eq!(registry.lock_executions().len(), MAX_RETAINED_EXECUTIONS);
        assert_eq!(registry.lock_completed_run_identifiers().len(), MAX_RETAINED_EXECUTIONS);
        assert_eq!(first_run_error.diagnostic().code, ExecutorDiagnosticCode::StreamExpired);
    }

    #[test]
    fn process_global_byte_budget_evicts_oldest_completed_histories() {
        const TERMINAL_OUTPUT_BYTES: usize = 220 * 1024;

        let registry = StreamedExecutionRegistry::default();
        let execution_count = MAX_RETAINED_EVENT_BYTES_GLOBAL / TERMINAL_OUTPUT_BYTES + 32;
        let mut first_run_identifier = None;

        for execution_index in 0..execution_count {
            let subscription = registry.insert().expect("completed history churn should remain bounded");
            let run_identifier = subscription.run_identifier.clone();
            drop(subscription);

            if execution_index == 0 {
                first_run_identifier = Some(run_identifier.clone());
            }

            registry.record_event(
                &run_identifier,
                ExecutorEvent::workflow_completed(json!({ "value": "x".repeat(TERMINAL_OUTPUT_BYTES) }), Duration::from_millis(1)),
            );
        }

        let first_run_identifier = first_run_identifier.expect("first run identifier should be captured");
        let first_run_error = registry
            .subscribe(&first_run_identifier, None)
            .expect_err("global byte churn should evict the oldest completed run");

        assert_eq!(first_run_error.diagnostic().code, ExecutorDiagnosticCode::StreamExpired);
        assert!(registry.retention_budget.retained_event_bytes() <= MAX_RETAINED_EVENT_BYTES_GLOBAL);
        assert!(registry.lock_executions().len() < execution_count);
    }

    #[test]
    fn active_terminal_reservations_reject_new_streams_at_global_capacity() {
        let registry = StreamedExecutionRegistry::default();
        let maximum_active_streams = MAX_RETAINED_EVENT_BYTES_GLOBAL / TERMINAL_EVENT_RESERVE_BYTES;
        let mut active_subscriptions = Vec::with_capacity(maximum_active_streams);

        for _stream_index in 0..maximum_active_streams {
            active_subscriptions.push(registry.insert().expect("active stream should fit the global terminal reserve"));
        }

        let error = registry
            .insert()
            .expect_err("global terminal reserve exhaustion should reject a new stream");

        assert_eq!(error.diagnostic().code, ExecutorDiagnosticCode::StreamCapacityExceeded);
        assert_eq!(
            registry.retention_budget.terminal_reserve_bytes(),
            maximum_active_streams * TERMINAL_EVENT_RESERVE_BYTES
        );
        assert!(registry.retention_budget.terminal_reserve_bytes() <= MAX_RETAINED_EVENT_BYTES_GLOBAL);
        drop(active_subscriptions);
    }

    #[tokio::test]
    async fn slow_subscriber_receives_explicit_gap_before_closure() {
        let streamed_execution = streamed_execution("test-run".to_string());
        let mut stream_subscription = streamed_execution.initial_subscription();

        for agent_index in 0..=STREAM_SUBSCRIBER_CAPACITY {
            assert!(!streamed_execution.record_event(agent_started_event(agent_index)));
        }

        for expected_identifier in 1..=STREAM_SUBSCRIBER_CAPACITY {
            let event = stream_subscription
                .receiver
                .recv()
                .await
                .expect("buffered event should be streamed");

            assert_eq!(event.event_identifier, expected_identifier as u64);
        }

        let gap_event = stream_subscription.receiver.recv().await.expect("gap event should be streamed");

        assert_eq!(gap_event.event.kind, ExecutorEventKind::StreamGap);
        assert_eq!(
            gap_event.event.diagnostic.as_ref().expect("gap diagnostic should be present").code,
            ExecutorDiagnosticCode::StreamGap
        );
        assert!(stream_subscription.receiver.recv().await.is_none());
    }

    #[tokio::test]
    async fn slow_subscriber_arc_lifetimes_are_budgeted_and_terminal_reserve_still_delivers_failure() {
        const LARGE_AGENT_NAME_BYTES: usize = 200 * 1024;

        let global_budget = Arc::new(GlobalStreamRetentionBudget::default());
        let streamed_execution = streamed_execution_with_global_budget("slow-byte-run".to_string(), Arc::clone(&global_budget));
        let retention_budget = Arc::clone(&streamed_execution.lock_state().retention_budget);
        let mut slow_subscription = streamed_execution.initial_subscription();
        let mut recorded_terminal_failure = false;

        for event_index in 0..STREAM_SUBSCRIBER_CAPACITY {
            let event = ExecutorEvent::agent_started(
                format!("{}-{event_index}", "a".repeat(LARGE_AGENT_NAME_BYTES)),
                "test-model".to_string(),
                Vec::new(),
                None,
            );

            if streamed_execution.record_event(event) {
                recorded_terminal_failure = true;

                break;
            }
        }

        let retained_bytes_before_drain = retention_budget.retained_event_bytes();

        assert!(recorded_terminal_failure);
        assert_eq!(global_budget.terminal_reserve_bytes(), 0);
        assert!(retained_bytes_before_drain > streamed_execution.lock_state().retained_history_bytes);

        let mut terminal_diagnostic_code = None;

        while let Some(sequenced_event) = slow_subscription.receiver.recv().await {
            if sequenced_event.event.is_terminal() {
                terminal_diagnostic_code = sequenced_event.event.diagnostic.as_ref().map(|diagnostic| diagnostic.code);
            }
        }

        let retained_bytes_after_drain = retention_budget.retained_event_bytes();

        assert_eq!(terminal_diagnostic_code, Some(ExecutorDiagnosticCode::StreamCapacityExceeded));
        assert!(retained_bytes_after_drain < retained_bytes_before_drain);
        assert_eq!(retained_bytes_after_drain, global_budget.retained_event_bytes());
        assert_eq!(streamed_execution.lock_state().events.len(), 1);
        assert!(retained_bytes_after_drain <= TERMINAL_EVENT_RESERVE_BYTES);
    }

    #[tokio::test]
    async fn terminal_overflow_gap_preserves_reconnect_cursor_and_terminal_replay() {
        const RUN_CAPABILITY_SENTINEL: &str = "superwire-run-capability-sentinel";
        let streamed_execution = streamed_execution(RUN_CAPABILITY_SENTINEL.to_string());
        let mut slow_subscription = streamed_execution.initial_subscription();

        for agent_index in 0..STREAM_SUBSCRIBER_CAPACITY {
            assert!(!streamed_execution.record_event(agent_started_event(agent_index)));
        }

        assert!(streamed_execution.record_event(ExecutorEvent::workflow_completed(
            json!({ "status": "done" }),
            Duration::from_millis(1),
        )));

        for expected_identifier in 1..=STREAM_SUBSCRIBER_CAPACITY {
            let event = slow_subscription.receiver.recv().await.expect("buffered event should be streamed");

            assert_eq!(event.event_identifier, expected_identifier as u64);
        }

        let gap_event = slow_subscription
            .receiver
            .recv()
            .await
            .expect("terminal overflow should emit a gap");

        assert_eq!(gap_event.event.kind, ExecutorEventKind::StreamGap);
        assert_eq!(gap_event.event_identifier, STREAM_SUBSCRIBER_CAPACITY as u64);
        let serialized_gap_event = serde_json::to_string(gap_event.event.as_ref()).expect("gap event should serialize");

        assert!(!serialized_gap_event.contains(RUN_CAPABILITY_SENTINEL));
        assert!(!serialized_gap_event.contains("run_identifier"));
        assert!(slow_subscription.receiver.recv().await.is_none());

        let mut replay_subscription = streamed_execution
            .subscribe(Some(gap_event.event_identifier))
            .expect("last delivered cursor should reconnect");
        let replayed_terminal = replay_subscription
            .receiver
            .recv()
            .await
            .expect("reconnect should replay the dropped terminal event");

        assert_eq!(replayed_terminal.event_identifier, STREAM_SUBSCRIBER_CAPACITY as u64 + 1);
        assert_eq!(replayed_terminal.event.kind, ExecutorEventKind::WorkflowCompleted);
        assert!(replay_subscription.receiver.recv().await.is_none());
    }

    #[test]
    fn streamed_execution_keeps_subscriber_connected_during_bounded_event_bursts() {
        let streamed_execution = streamed_execution("test-run".to_string());
        let mut stream_subscription = streamed_execution.initial_subscription();

        for agent_index in 0..100 {
            assert!(!streamed_execution.record_event(agent_started_event(agent_index)));
        }

        assert!(streamed_execution.record_event(ExecutorEvent::workflow_completed(
            json!({ "status": "done" }),
            Duration::from_millis(10),
        )));

        let mut event_count = 0;
        let mut last_event_kind = None;

        while let Ok(sequenced_event) = stream_subscription.receiver.try_recv() {
            event_count += 1;
            last_event_kind = Some(sequenced_event.event.kind);
        }

        assert_eq!(event_count, 101);
        assert_eq!(last_event_kind, Some(ExecutorEventKind::WorkflowCompleted));
    }

    #[derive(Debug)]
    struct BlockingDiscoveryClient {
        started_sender: Mutex<Option<oneshot::Sender<()>>>,
        release_receiver: Mutex<std::sync::mpsc::Receiver<()>>,
    }

    impl McpClientBackend for BlockingDiscoveryClient {
        fn list_tools(&self) -> Result<McpServerLock, McpError> {
            self.started_sender
                .lock()
                .expect("discovery start sender lock should not poison")
                .take()
                .expect("discovery should start once")
                .send(())
                .expect("discovery start should be observable");
            self.release_receiver
                .lock()
                .expect("discovery release receiver lock should not poison")
                .recv()
                .expect("discovery should be released");

            Ok(McpServerLock::default())
        }

        fn call_tool(&self, _tool_name: &str, _arguments: serde_json::Value) -> Result<serde_json::Value, McpError> {
            Ok(serde_json::Value::Null)
        }

        fn read_resource(&self, _resource_name: &str, _arguments: serde_json::Value) -> Result<serde_json::Value, McpError> {
            Ok(serde_json::Value::Null)
        }

        fn get_prompt(&self, _prompt_name: &str, _arguments: serde_json::Value) -> Result<serde_json::Value, McpError> {
            Ok(serde_json::Value::Null)
        }
    }

    #[derive(Debug)]
    struct BlockingDiscoveryClientFactory {
        client: Arc<BlockingDiscoveryClient>,
    }

    impl McpClientFactory for BlockingDiscoveryClientFactory {
        fn client_for_config(&self, _server_config: McpServerConfig) -> Result<Arc<dyn McpClientBackend>, McpError> {
            Ok(self.client.clone())
        }
    }

    #[tokio::test]
    async fn mcp_discovery_does_not_block_the_current_tokio_thread() {
        let workflow_source = workflow_source! {
            provider openai from openai {
                endpoint: "http://localhost:1234/v1"
                api_key: "test-api-key"
            }

            model openai_model from openai {
                id: "model-a"
            }

            mcp local {
                endpoint: "http://127.0.0.1:3000/mcp"
            }

            agent greeting {
                model: model.openai_model
                instruction: "Write a short welcome message."
                output {
                    value: string
                }
            }

            output {
                greeting: agent.greeting.value
            }
        };
        let (started_sender, started_receiver) = oneshot::channel();
        let (release_sender, release_receiver) = std::sync::mpsc::channel();
        let mcp_client = Arc::new(BlockingDiscoveryClient {
            started_sender: Mutex::new(Some(started_sender)),
            release_receiver: Mutex::new(release_receiver),
        });
        let mcp_client_factory = Arc::new(BlockingDiscoveryClientFactory { client: mcp_client });
        let service =
            ExecutorService::new(TestModelProvider::new(vec![json!({ "value": "done" })])).with_mcp_client_factory(mcp_client_factory);
        let execution_task = tokio::spawn(async move { service.execute(request(workflow_source)).await });

        started_receiver.await.expect("MCP discovery should start");
        let timer_started_at = Instant::now();
        tokio::time::sleep(Duration::from_millis(20)).await;

        assert!(timer_started_at.elapsed() >= Duration::from_millis(20));
        assert!(!execution_task.is_finished());
        release_sender.send(()).expect("MCP discovery should be released");
        let response = tokio::time::timeout(Duration::from_secs(2), execution_task)
            .await
            .expect("execution should finish after discovery release")
            .expect("execution task should not panic")
            .expect("workflow should execute");

        assert_eq!(response.output, json!({ "greeting": "done" }));
    }

    #[derive(Debug)]
    struct UnavailableInvalidationCacheStore;

    impl AgentCacheStore for UnavailableInvalidationCacheStore {
        fn get(
            &self,
            _key: &crate::runtime::cache::AgentCacheKey,
        ) -> Result<Option<crate::runtime::cache::CachedAgentExecution>, ExecutorError> {
            Ok(None)
        }

        fn put(
            &self,
            _key: crate::runtime::cache::AgentCacheKey,
            _execution: crate::runtime::cache::CachedAgentExecution,
            _time_to_live: Duration,
        ) -> Result<(), ExecutorError> {
            Ok(())
        }

        fn purge_session(&self, _session: &AgentCacheSession) -> Result<usize, ExecutorError> {
            Err(ExecutorError::cache(
                superwire_protocol::event::CacheOperation::Purge,
                "cache is unavailable",
            ))
        }
    }

    #[test]
    fn cache_invalidation_outage_returns_typed_failure() {
        let service = ExecutorService {
            model_provider: (),
            streamed_executions: StreamedExecutionRegistry::default(),
            agent_cache_store: Arc::new(UnavailableInvalidationCacheStore),
            agent_cache_time_to_live: DEFAULT_AGENT_CACHE_TIME_TO_LIVE,
            mcp_client_factory: Arc::new(HttpMcpClientFactory),
        };

        let error = service
            .invalidate_agent_cache_session(&AgentCacheSession::new("unavailable-cache"))
            .expect_err("cache invalidation outage must fail the request");

        assert_eq!(error.diagnostic().code, ExecutorDiagnosticCode::CacheUnavailable);
    }
}
