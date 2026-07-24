use axum::body::Body;
use axum::http::{Request, Response};
use axum::Router;
use serde_json::{json, Value};
use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use superwire_executor::ExecutorService;
use superwire_executor_server::{executor_router, executor_router_with_service};
use superwire_macros::workflow_source;
use superwire_protocol::event::{DiagnosticRetryability, ExecutorDiagnosticCode, ExecutorEvent, ExecutorEventKind};
use superwire_provider_cersei::{CerseiModelProvider, ProviderNetworkPolicy};
use tower::util::ServiceExt;

const RUN_IDENTIFIER_HEADER: &str = "x-superwire-run-id";

#[tokio::test]
async fn real_provider_router_stream_completes_strongly_typed_workflow() {
    let model_double = OpenAiCompatibleModelDouble::spawn([ModelDoubleResponse::Finalize(json!({
        "greeting": "Hello from the local model double."
    }))]);
    let router = SmokeWorkflow::router();
    let started_execution = StartedSmokeExecution::start(router, model_double.endpoint()).await;
    let run_identifier = started_execution.run_identifier.clone();
    let transcript = started_execution.finish().await;
    transcript.assert_clean_terminal_closure(ExecutorEventKind::WorkflowCompleted);

    model_double.wait_for_request_count(1).await;
    model_double.assert_requests_match_contract();
    model_double.assert_clean();
    transcript.assert_run_identifier_is_uuid(&run_identifier);
    transcript.assert_strictly_monotonic_identifiers();
    transcript.assert_kinds(&[
        ExecutorEventKind::WorkflowStarted,
        ExecutorEventKind::WorkflowPlanned,
        ExecutorEventKind::AgentStarted,
        ExecutorEventKind::ProviderAttemptStarted,
        ExecutorEventKind::ProviderAttemptCompleted,
        ExecutorEventKind::AgentCompleted,
        ExecutorEventKind::WorkflowCompleted,
    ]);

    let terminal_event = transcript.terminal_event();

    assert_eq!(
        terminal_event.data.as_ref().and_then(|data| data.get("output")),
        Some(&json!({ "greeting": "Hello from the local model double." }))
    );
}

#[tokio::test]
async fn default_executor_provider_policy_rejects_workflow_custom_endpoints_before_connecting() {
    let model_double = OpenAiCompatibleModelDouble::spawn([]);
    let started_execution = StartedSmokeExecution::start(executor_router(), model_double.endpoint()).await;
    let transcript = started_execution.finish().await;
    let terminal_event = transcript.terminal_event();
    let diagnostic = terminal_event
        .diagnostic
        .as_ref()
        .expect("rejected custom provider endpoint should produce a diagnostic");

    transcript.assert_clean_terminal_closure(ExecutorEventKind::WorkflowFailed);
    model_double.assert_clean();

    assert_eq!(model_double.request_count(), 0);
    assert_eq!(diagnostic.message, "custom provider endpoints are disabled");
}

#[tokio::test]
async fn real_provider_router_stream_retries_rate_limit_then_completes() {
    let model_double = OpenAiCompatibleModelDouble::spawn([
        ModelDoubleResponse::RateLimited,
        ModelDoubleResponse::Finalize(json!({ "greeting": "Hello after retry." })),
    ]);
    let started_execution = StartedSmokeExecution::start(SmokeWorkflow::router(), model_double.endpoint()).await;
    let transcript = started_execution.finish().await;

    model_double.wait_for_request_count(2).await;
    model_double.assert_requests_match_contract();
    model_double.assert_clean();
    transcript.assert_strictly_monotonic_identifiers();
    transcript.assert_clean_terminal_closure(ExecutorEventKind::WorkflowCompleted);
    transcript.assert_kinds(&[
        ExecutorEventKind::WorkflowStarted,
        ExecutorEventKind::WorkflowPlanned,
        ExecutorEventKind::AgentStarted,
        ExecutorEventKind::ProviderAttemptStarted,
        ExecutorEventKind::ProviderAttemptFailed,
        ExecutorEventKind::ProviderAttemptStarted,
        ExecutorEventKind::ProviderAttemptCompleted,
        ExecutorEventKind::AgentCompleted,
        ExecutorEventKind::WorkflowCompleted,
    ]);

    let attempt_started_frames = transcript
        .frames
        .iter()
        .filter(|frame| frame.event.kind == ExecutorEventKind::ProviderAttemptStarted)
        .collect::<Vec<_>>();
    let failed_attempt = transcript
        .events_of_kind(ExecutorEventKind::ProviderAttemptFailed)
        .into_iter()
        .next()
        .expect("retry smoke should include a failed provider attempt");
    let failed_diagnostic = failed_attempt
        .diagnostic
        .as_ref()
        .expect("failed provider attempt should include a diagnostic");

    assert_eq!(attempt_started_frames.len(), 2);
    assert_ne!(attempt_started_frames[0].identifier, attempt_started_frames[1].identifier);
    assert_eq!(failed_diagnostic.code, ExecutorDiagnosticCode::ProviderRateLimited);
    assert!(matches!(
        failed_diagnostic.retryability,
        DiagnosticRetryability::Safe | DiagnosticRetryability::AfterDelay
    ));
    assert_eq!(
        transcript
            .terminal_event()
            .data
            .as_ref()
            .and_then(|data| data.pointer("/output/greeting")),
        Some(&json!("Hello after retry."))
    );
}

#[tokio::test]
async fn real_provider_router_stream_reports_invalid_finalize_output() {
    let model_double = OpenAiCompatibleModelDouble::spawn([ModelDoubleResponse::Finalize(json!({
        "greeting": 17
    }))]);
    let started_execution = StartedSmokeExecution::start(SmokeWorkflow::router(), model_double.endpoint()).await;
    let transcript = started_execution.finish().await;

    model_double.wait_for_request_count(1).await;
    model_double.assert_requests_match_contract();
    model_double.assert_clean();
    transcript.assert_strictly_monotonic_identifiers();
    transcript.assert_clean_terminal_closure(ExecutorEventKind::WorkflowFailed);

    let agent_failure = transcript
        .events_of_kind(ExecutorEventKind::AgentFailed)
        .into_iter()
        .next()
        .expect("invalid finalize output should fail the agent");
    let agent_diagnostic = agent_failure
        .diagnostic
        .as_ref()
        .expect("agent failure should include a diagnostic");
    let workflow_diagnostic = transcript
        .terminal_event()
        .diagnostic
        .as_ref()
        .expect("workflow failure should include a diagnostic");

    assert_eq!(agent_diagnostic.code, ExecutorDiagnosticCode::InvalidOutput);
    assert_eq!(workflow_diagnostic.code, ExecutorDiagnosticCode::InvalidOutput);
}

#[tokio::test]
async fn real_provider_router_stream_cancels_before_model_completion() {
    let model_double = OpenAiCompatibleModelDouble::spawn([ModelDoubleResponse::BlockUntilShutdown]);
    let router = SmokeWorkflow::router();
    let started_execution = StartedSmokeExecution::start(router.clone(), model_double.endpoint()).await;

    model_double.wait_for_request_count(1).await;

    let accepted_transition = started_execution.cancel(router.clone()).await;
    let repeated_transition = started_execution.cancel(router.clone()).await;
    let transcript = started_execution.finish().await;
    let terminal_transition = CancelTransition::request(router, &transcript.run_identifier).await;

    model_double.assert_requests_match_contract();
    model_double.assert_clean();
    transcript.assert_strictly_monotonic_identifiers();
    transcript.assert_clean_terminal_closure(ExecutorEventKind::WorkflowCancelled);

    assert_eq!(accepted_transition, "accepted");
    assert!(matches!(repeated_transition.as_str(), "already_requested" | "already_terminal"));
    assert_eq!(terminal_transition, "already_terminal");
    assert!(transcript
        .events_of_kind(ExecutorEventKind::ProviderAttemptFailed)
        .iter()
        .any(|event| {
            event
                .diagnostic
                .as_ref()
                .is_some_and(|diagnostic| diagnostic.code == ExecutorDiagnosticCode::Cancelled)
        }));
    assert!(transcript.events_of_kind(ExecutorEventKind::AgentCancelled).iter().any(|event| {
        event
            .diagnostic
            .as_ref()
            .is_some_and(|diagnostic| diagnostic.code == ExecutorDiagnosticCode::Cancelled)
    }));
    assert_eq!(
        transcript
            .terminal_event()
            .diagnostic
            .as_ref()
            .expect("cancelled workflow should include a diagnostic")
            .code,
        ExecutorDiagnosticCode::Cancelled
    );
}

#[tokio::test]
async fn file_cleanup_does_not_delay_streamed_cancellation_terminal() {
    const FILE_CONTENT_SECRET: &str = "cancelled-file-content-secret-sentinel";
    const PROVIDER_API_KEY_SECRET: &str = "cancelled-provider-key-secret-sentinel";

    let model_double = FileCleanupModelDouble::spawn();
    let router = FileCleanupSmokeWorkflow::router();
    let started_execution = StartedSmokeExecution::start_with_request(
        router.clone(),
        FileCleanupSmokeWorkflow::request(model_double.endpoint(), PROVIDER_API_KEY_SECRET),
    )
    .await;

    model_double.wait_for_request("POST", "/v1/chat/completions").await;

    let cancellation_started_at = Instant::now();
    let accepted_transition = started_execution.cancel(router).await;
    let transcript = started_execution.finish().await;
    let terminal_elapsed = cancellation_started_at.elapsed();

    model_double.wait_for_request("DELETE", "/v1/files/file-cleanup-smoke").await;
    model_double.assert_clean();
    transcript.assert_strictly_monotonic_identifiers();
    transcript.assert_clean_terminal_closure(ExecutorEventKind::WorkflowCancelled);
    assert!(
        transcript.events_of_kind(ExecutorEventKind::AgentFileDeleted).is_empty(),
        "post-cancellation cleanup must not keep the event channel open or emit lifecycle events"
    );

    assert_eq!(accepted_transition, "accepted");
    assert!(
        terminal_elapsed < Duration::from_secs(3),
        "cancellation terminal waited for detached cleanup: {terminal_elapsed:?}"
    );
    assert_eq!(
        transcript
            .terminal_event()
            .diagnostic
            .as_ref()
            .expect("cancelled workflow should include a diagnostic")
            .code,
        ExecutorDiagnosticCode::Cancelled
    );

    for event_frame in &transcript.frames {
        let serialized_event = serde_json::to_string(&event_frame.event).expect("public event should serialize");

        assert!(!serialized_event.contains(FILE_CONTENT_SECRET));
        assert!(!serialized_event.contains(PROVIDER_API_KEY_SECRET));
    }
}

struct SmokeWorkflow;

impl SmokeWorkflow {
    fn source() -> &'static str {
        workflow_source! {
            secrets {
                endpoint: string
            }

            provider local from ollama {
                endpoint: secrets.endpoint
            }

            model local_model from local {
                id: "smoke-model"
            }

            agent writer {
                model: model.local_model {
                    inference {
                        provider_max_retries: 1
                        provider_retry_base_delay_ms: 0
                    }
                }

                instruction: "Return one short greeting."
                output {
                    greeting: string
                }
            }

            output {
                greeting: agent.writer.greeting
            }
        }
    }

    fn router() -> Router {
        let model_provider = CerseiModelProvider::for_network_policy(ProviderNetworkPolicy::Trusted);
        executor_router_with_service(ExecutorService::new(model_provider), true)
    }

    fn request(endpoint: &str) -> Value {
        json!({
            "workflow_source": Self::source(),
            "secrets": {
                "endpoint": endpoint
            }
        })
    }
}

struct FileCleanupSmokeWorkflow;

impl FileCleanupSmokeWorkflow {
    fn source() -> &'static str {
        workflow_source! {
            secrets {
                endpoint: string
                api_key: string
            }

            provider local from openai_compatible {
                endpoint: secrets.endpoint
                api_key: secrets.api_key
            }

            model local_model from local {
                id: "smoke-model"
                wire_api: "chat/completion"
            }

            agent writer {
                model: model.local_model
                instruction: "Review the uploaded file."

                file "cancelled-file-content-secret-sentinel" {
                    name: "cancelled-file.txt"
                }

                output {
                    greeting: string
                }
            }

            output {
                greeting: agent.writer.greeting
            }
        }
    }

    fn router() -> Router {
        let model_provider = CerseiModelProvider::for_network_policy(ProviderNetworkPolicy::Trusted);
        executor_router_with_service(ExecutorService::new(model_provider), true)
    }

    fn request(endpoint: &str, api_key: &str) -> Value {
        json!({
            "workflow_source": Self::source(),
            "secrets": {
                "endpoint": endpoint,
                "api_key": api_key
            }
        })
    }
}

struct StartedSmokeExecution {
    run_identifier: String,
    response: Response<Body>,
}

impl StartedSmokeExecution {
    async fn start(router: Router, endpoint: &str) -> Self {
        Self::start_with_request(router, SmokeWorkflow::request(endpoint)).await
    }

    async fn start_with_request(router: Router, request_body: Value) -> Self {
        let request = Request::builder()
            .method("POST")
            .uri("/execute")
            .header("accept", "text/event-stream")
            .header("content-type", "application/json")
            .body(Body::from(request_body.to_string()))
            .expect("smoke execution request should build");
        let response = router.oneshot(request).await.expect("smoke execution request should execute");

        assert_eq!(response.status(), axum::http::StatusCode::OK);

        let run_identifier = response
            .headers()
            .get(RUN_IDENTIFIER_HEADER)
            .and_then(|header_value| header_value.to_str().ok())
            .expect("event stream should expose a run identifier")
            .to_string();

        Self { run_identifier, response }
    }

    async fn cancel(&self, router: Router) -> String {
        CancelTransition::request(router, &self.run_identifier).await
    }

    async fn finish(self) -> EventStreamTranscript {
        EventStreamTranscript::from_response(self.run_identifier, self.response).await
    }
}

struct CancelTransition;

impl CancelTransition {
    async fn request(router: Router, run_identifier: &str) -> String {
        let request = Request::builder()
            .method("POST")
            .uri(format!("/execute/{run_identifier}/cancel"))
            .body(Body::empty())
            .expect("cancellation request should build");
        let response = router.oneshot(request).await.expect("cancellation request should execute");

        assert_eq!(response.status(), axum::http::StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("cancellation response should read");
        let payload: Value = serde_json::from_slice(&body).expect("cancellation response should be JSON");

        payload
            .get("transition")
            .and_then(Value::as_str)
            .expect("cancellation response should contain transition")
            .to_string()
    }
}

#[derive(Debug)]
struct EventStreamFrame {
    identifier: u64,
    event_name: String,
    event: ExecutorEvent,
}

#[derive(Debug)]
struct EventStreamTranscript {
    run_identifier: String,
    frames: Vec<EventStreamFrame>,
}

impl EventStreamTranscript {
    async fn from_response(run_identifier: String, response: Response<Body>) -> Self {
        let body = tokio::time::timeout(Duration::from_secs(3), axum::body::to_bytes(response.into_body(), usize::MAX))
            .await
            .expect("event stream should close after a terminal event")
            .expect("event stream body should read");
        let body_text = String::from_utf8(body.to_vec()).expect("event stream should be UTF-8");
        let frames = body_text
            .replace("\r\n", "\n")
            .split("\n\n")
            .filter_map(Self::parse_frame)
            .collect::<Vec<_>>();

        assert!(!frames.is_empty(), "event stream should contain executor events");

        Self { run_identifier, frames }
    }

    fn parse_frame(frame_text: &str) -> Option<EventStreamFrame> {
        let mut identifier = None;
        let mut event_name = None;
        let mut data_lines = Vec::new();

        for line in frame_text.lines() {
            if let Some(value) = line.strip_prefix("id:") {
                identifier = value.trim().parse::<u64>().ok();
            } else if let Some(value) = line.strip_prefix("event:") {
                event_name = Some(value.trim().to_string());
            } else if let Some(value) = line.strip_prefix("data:") {
                data_lines.push(value.trim_start());
            }
        }

        let identifier = identifier?;
        let event_name = event_name?;
        let event = serde_json::from_str::<ExecutorEvent>(&data_lines.join("\n")).expect("SSE data should deserialize as ExecutorEvent");

        assert_eq!(event_name, event.kind.as_str());

        Some(EventStreamFrame {
            identifier,
            event_name,
            event,
        })
    }

    fn assert_run_identifier_is_uuid(&self, run_identifier: &str) {
        assert_eq!(self.run_identifier, run_identifier);
        assert_eq!(run_identifier.len(), 32);
        assert!(run_identifier.bytes().all(|byte| byte.is_ascii_hexdigit()));
    }

    fn assert_strictly_monotonic_identifiers(&self) {
        for adjacent_frames in self.frames.windows(2) {
            assert!(
                adjacent_frames[0].identifier < adjacent_frames[1].identifier,
                "event IDs should be strictly monotonic: {:?}",
                self.frames
                    .iter()
                    .map(|frame| (frame.identifier, frame.event_name.as_str()))
                    .collect::<Vec<_>>()
            );
        }
    }

    fn assert_clean_terminal_closure(&self, expected_terminal_kind: ExecutorEventKind) {
        let terminal_frames = self
            .frames
            .iter()
            .filter(|frame| frame.event.kind.is_terminal())
            .collect::<Vec<_>>();

        assert_eq!(terminal_frames.len(), 1);
        assert_eq!(
            terminal_frames[0].event.kind, expected_terminal_kind,
            "unexpected terminal event: {:#?}",
            terminal_frames[0].event
        );
        assert_eq!(
            self.frames.last().map(|frame| &frame.event.kind),
            Some(&expected_terminal_kind),
            "terminal event should close the stream: {:#?}",
            self.frames
        );
    }

    fn assert_kinds(&self, expected_kinds: &[ExecutorEventKind]) {
        let actual_kinds = self.frames.iter().map(|frame| frame.event.kind).collect::<Vec<_>>();

        assert_eq!(actual_kinds, expected_kinds);
    }

    fn events_of_kind(&self, event_kind: ExecutorEventKind) -> Vec<&ExecutorEvent> {
        self.frames
            .iter()
            .filter(|frame| frame.event.kind == event_kind)
            .map(|frame| &frame.event)
            .collect()
    }

    fn terminal_event(&self) -> &ExecutorEvent {
        &self.frames.last().expect("event stream should have a terminal frame").event
    }
}

#[derive(Debug)]
enum ModelDoubleResponse {
    RateLimited,
    Finalize(Value),
    BlockUntilShutdown,
}

impl ModelDoubleResponse {
    fn as_http_response(&self) -> Option<String> {
        match self {
            Self::RateLimited => {
                let body = json!({ "error": { "message": "local deterministic rate limit" } }).to_string();

                Some(format!(
                    "HTTP/1.1 429 Too Many Requests\r\ncontent-type: application/json\r\nretry-after: 0\r\nconnection: close\r\ncontent-length: {}\r\n\r\n{}",
                    body.len(),
                    body
                ))
            }
            Self::Finalize(output) => {
                let arguments = json!({
                    "type": "success",
                    "output": output,
                });
                let chunk = json!({
                    "id": "chatcmpl-local-smoke",
                    "object": "chat.completion.chunk",
                    "created": 0,
                    "model": "smoke-model",
                    "choices": [{
                        "index": 0,
                        "delta": {
                            "tool_calls": [{
                                "index": 0,
                                "id": "call_finalize_1",
                                "type": "function",
                                "function": {
                                    "name": "finalize",
                                    "arguments": serde_json::to_string(&arguments)
                                        .expect("finalize arguments should serialize")
                                }
                            }]
                        },
                        "finish_reason": "tool_calls"
                    }]
                });
                let body = format!("data: {chunk}\n\ndata: [DONE]\n\n");

                Some(format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\nconnection: close\r\ncontent-length: {}\r\n\r\n{}",
                    body.len(),
                    body
                ))
            }
            Self::BlockUntilShutdown => None,
        }
    }
}

#[derive(Debug)]
struct OpenAiCompatibleModelDoubleState {
    responses: Mutex<VecDeque<ModelDoubleResponse>>,
    requests: Mutex<Vec<Value>>,
    failure: Mutex<Option<String>>,
    shutdown: AtomicBool,
    shutdown_gate: (Mutex<bool>, Condvar),
}

impl OpenAiCompatibleModelDoubleState {
    fn record_failure(&self, message: impl Into<String>) {
        let mut failure = self.failure.lock().expect("model double failure lock should not be poisoned");

        if failure.is_none() {
            *failure = Some(message.into());
        }
    }

    fn handle_connection(&self, mut stream: TcpStream) {
        let captured_request = match CapturedHttpRequest::read_from(&stream) {
            Ok(captured_request) => captured_request,
            Err(error) => {
                if !self.shutdown.load(Ordering::SeqCst) {
                    self.record_failure(error);
                }

                return;
            }
        };

        if captured_request.method != "POST" || captured_request.path != "/v1/chat/completions" {
            self.record_failure(format!(
                "unexpected model request {} {}",
                captured_request.method, captured_request.path
            ));

            return;
        }

        let request_body = match serde_json::from_slice::<Value>(&captured_request.body) {
            Ok(request_body) => request_body,
            Err(error) => {
                self.record_failure(format!("model request body should be JSON: {error}"));

                return;
            }
        };

        self.requests
            .lock()
            .expect("model double request lock should not be poisoned")
            .push(request_body);

        let response = self
            .responses
            .lock()
            .expect("model double response lock should not be poisoned")
            .pop_front();
        let Some(response) = response else {
            self.record_failure("model double received more requests than configured responses");

            return;
        };

        let Some(http_response) = response.as_http_response() else {
            let (shutdown_lock, shutdown_condition) = &self.shutdown_gate;
            let mut shutdown_requested = shutdown_lock.lock().expect("shutdown lock should not be poisoned");

            while !*shutdown_requested {
                shutdown_requested = shutdown_condition
                    .wait(shutdown_requested)
                    .expect("shutdown lock should not be poisoned");
            }

            return;
        };

        if let Err(error) = stream.write_all(http_response.as_bytes()).and_then(|()| stream.flush()) {
            self.record_failure(format!("model response should write: {error}"));
        }
    }
}

struct OpenAiCompatibleModelDouble {
    endpoint: String,
    state: Arc<OpenAiCompatibleModelDoubleState>,
    server_thread: Option<thread::JoinHandle<()>>,
}

impl OpenAiCompatibleModelDouble {
    fn spawn(responses: impl IntoIterator<Item = ModelDoubleResponse>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("model double should bind an ephemeral port");
        let local_address = listener.local_addr().expect("model double should expose its local address");

        listener.set_nonblocking(true).expect("model double listener should be nonblocking");

        let state = Arc::new(OpenAiCompatibleModelDoubleState {
            responses: Mutex::new(VecDeque::from_iter(responses)),
            requests: Mutex::new(Vec::new()),
            failure: Mutex::new(None),
            shutdown: AtomicBool::new(false),
            shutdown_gate: (Mutex::new(false), Condvar::new()),
        });
        let server_state = state.clone();
        let server_thread = thread::spawn(move || {
            while !server_state.shutdown.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((stream, _peer_address)) => server_state.handle_connection(stream),
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(2));
                    }
                    Err(error) => {
                        server_state.record_failure(format!("model double accept failed: {error}"));
                        break;
                    }
                }
            }
        });

        Self {
            endpoint: format!("http://{local_address}/v1"),
            state,
            server_thread: Some(server_thread),
        }
    }

    fn endpoint(&self) -> &str {
        &self.endpoint
    }

    fn request_count(&self) -> usize {
        self.state
            .requests
            .lock()
            .expect("model double request lock should not be poisoned")
            .len()
    }

    async fn wait_for_request_count(&self, expected_count: usize) {
        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                let request_count = self
                    .state
                    .requests
                    .lock()
                    .expect("model double request lock should not be poisoned")
                    .len();

                if request_count >= expected_count {
                    break;
                }

                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("model double should receive the expected request count");
    }

    fn assert_requests_match_contract(&self) {
        let requests = self
            .state
            .requests
            .lock()
            .expect("model double request lock should not be poisoned");

        assert!(!requests.is_empty(), "model double should capture provider requests");

        for request in requests.iter() {
            assert_eq!(request.get("model"), Some(&json!("smoke-model")));
            assert_eq!(request.get("stream"), Some(&json!(true)));

            let finalize_tool = request
                .get("tools")
                .and_then(Value::as_array)
                .and_then(|tools| {
                    tools
                        .iter()
                        .find(|tool| tool.pointer("/function/name").and_then(Value::as_str) == Some("finalize"))
                })
                .expect("provider request should include internal finalize tool");

            assert_eq!(
                finalize_tool.pointer("/function/parameters/properties/output/properties/greeting/type"),
                Some(&json!("string"))
            );
            assert!(request
                .get("messages")
                .and_then(Value::as_array)
                .is_some_and(|messages| !messages.is_empty()));
        }
    }

    fn assert_clean(&self) {
        let failure = self
            .state
            .failure
            .lock()
            .expect("model double failure lock should not be poisoned")
            .clone();
        let remaining_responses = self
            .state
            .responses
            .lock()
            .expect("model double response lock should not be poisoned")
            .len();

        assert!(failure.is_none(), "model double failed: {failure:?}");
        assert_eq!(remaining_responses, 0, "all configured model responses should be consumed");
    }
}

impl Drop for OpenAiCompatibleModelDouble {
    fn drop(&mut self) {
        self.state.shutdown.store(true, Ordering::SeqCst);

        let (shutdown_lock, shutdown_condition) = &self.state.shutdown_gate;
        let mut shutdown_requested = shutdown_lock.lock().expect("shutdown lock should not be poisoned");
        *shutdown_requested = true;
        shutdown_condition.notify_all();
        drop(shutdown_requested);

        if let Some(server_thread) = self.server_thread.take() {
            server_thread.join().expect("model double thread should stop cleanly");
        }
    }
}

#[derive(Debug)]
struct FileCleanupModelDoubleState {
    requests: Mutex<Vec<(String, String)>>,
    failure: Mutex<Option<String>>,
    shutdown: AtomicBool,
    shutdown_gate: (Mutex<bool>, Condvar),
}

impl FileCleanupModelDoubleState {
    fn handle_connection(&self, mut stream: TcpStream) {
        let captured_request = match CapturedHttpRequest::read_from(&stream) {
            Ok(captured_request) => captured_request,
            Err(error) => {
                if !self.shutdown.load(Ordering::SeqCst) {
                    self.record_failure(error);
                }

                return;
            }
        };
        let method = captured_request.method;
        let path = captured_request.path;

        self.requests
            .lock()
            .expect("file cleanup model request lock should not be poisoned")
            .push((method.clone(), path.clone()));

        match (method.as_str(), path.as_str()) {
            ("POST", "/v1/files") => {
                let response_body = json!({
                    "id": "file-cleanup-smoke",
                    "filename": "cancelled-file.txt",
                    "purpose": "file-extract",
                    "bytes": captured_request.body.len()
                })
                .to_string();
                let http_response = format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\nconnection: close\r\ncontent-length: {}\r\n\r\n{}",
                    response_body.len(),
                    response_body
                );

                if let Err(error) = stream.write_all(http_response.as_bytes()).and_then(|()| stream.flush()) {
                    self.record_failure(format!("file upload response should write: {error}"));
                }
            }
            ("POST", "/v1/chat/completions") | ("DELETE", "/v1/files/file-cleanup-smoke") => {
                let (shutdown_lock, shutdown_condition) = &self.shutdown_gate;
                let mut shutdown_requested = shutdown_lock.lock().expect("file cleanup shutdown lock should not be poisoned");

                while !*shutdown_requested {
                    shutdown_requested = shutdown_condition
                        .wait(shutdown_requested)
                        .expect("file cleanup shutdown lock should not be poisoned");
                }
            }
            _ => self.record_failure(format!("unexpected file cleanup model request {method} {path}")),
        }
    }

    fn record_failure(&self, message: impl Into<String>) {
        let mut failure = self.failure.lock().expect("file cleanup model failure lock should not be poisoned");

        if failure.is_none() {
            *failure = Some(message.into());
        }
    }
}

struct FileCleanupModelDouble {
    endpoint: String,
    state: Arc<FileCleanupModelDoubleState>,
    server_thread: Option<thread::JoinHandle<()>>,
}

impl FileCleanupModelDouble {
    fn spawn() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("file cleanup model double should bind an ephemeral port");
        let local_address = listener
            .local_addr()
            .expect("file cleanup model double should expose its local address");

        listener
            .set_nonblocking(true)
            .expect("file cleanup model double listener should be nonblocking");

        let state = Arc::new(FileCleanupModelDoubleState {
            requests: Mutex::new(Vec::new()),
            failure: Mutex::new(None),
            shutdown: AtomicBool::new(false),
            shutdown_gate: (Mutex::new(false), Condvar::new()),
        });
        let server_state = Arc::clone(&state);
        let server_thread = thread::spawn(move || {
            let mut connection_threads = Vec::new();

            while !server_state.shutdown.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((stream, _peer_address)) => {
                        let connection_state = Arc::clone(&server_state);

                        connection_threads.push(thread::spawn(move || connection_state.handle_connection(stream)));
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(2));
                    }
                    Err(error) => {
                        server_state.record_failure(format!("file cleanup model double accept failed: {error}"));

                        break;
                    }
                }
            }

            for connection_thread in connection_threads {
                connection_thread
                    .join()
                    .expect("file cleanup model connection thread should stop cleanly");
            }
        });

        Self {
            endpoint: format!("http://{local_address}/v1"),
            state,
            server_thread: Some(server_thread),
        }
    }

    fn endpoint(&self) -> &str {
        &self.endpoint
    }

    async fn wait_for_request(&self, method: &str, path: &str) {
        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                let request_observed = self
                    .state
                    .requests
                    .lock()
                    .expect("file cleanup model request lock should not be poisoned")
                    .iter()
                    .any(|(request_method, request_path)| request_method == method && request_path == path);

                if request_observed {
                    break;
                }

                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap_or_else(|error| panic!("file cleanup model should receive {method} {path}: {error}"));
    }

    fn assert_clean(&self) {
        let failure = self
            .state
            .failure
            .lock()
            .expect("file cleanup model failure lock should not be poisoned")
            .clone();

        assert!(failure.is_none(), "file cleanup model double failed: {failure:?}");
    }
}

impl Drop for FileCleanupModelDouble {
    fn drop(&mut self) {
        self.state.shutdown.store(true, Ordering::SeqCst);

        let (shutdown_lock, shutdown_condition) = &self.state.shutdown_gate;
        let mut shutdown_requested = shutdown_lock.lock().expect("file cleanup shutdown lock should not be poisoned");
        *shutdown_requested = true;
        shutdown_condition.notify_all();
        drop(shutdown_requested);

        if let Some(server_thread) = self.server_thread.take() {
            server_thread.join().expect("file cleanup model double thread should stop cleanly");
        }
    }
}

#[derive(Debug)]
struct CapturedHttpRequest {
    method: String,
    path: String,
    body: Vec<u8>,
}

impl CapturedHttpRequest {
    fn read_from(stream: &TcpStream) -> Result<Self, String> {
        let reader_stream = stream
            .try_clone()
            .map_err(|error| format!("model request stream should clone: {error}"))?;
        let mut reader = BufReader::new(reader_stream);
        let mut request_line = String::new();
        let bytes_read = reader
            .read_line(&mut request_line)
            .map_err(|error| format!("model request line should read: {error}"))?;

        if bytes_read == 0 {
            return Err("model request connection closed before a request line".to_string());
        }

        let mut request_line_parts = request_line.split_whitespace();
        let method = request_line_parts
            .next()
            .ok_or_else(|| "model request should include a method".to_string())?
            .to_string();
        let path = request_line_parts
            .next()
            .ok_or_else(|| "model request should include a path".to_string())?
            .to_string();
        let mut content_length = 0_usize;

        loop {
            let mut header_line = String::new();
            reader
                .read_line(&mut header_line)
                .map_err(|error| format!("model request header should read: {error}"))?;

            if header_line == "\r\n" || header_line == "\n" || header_line.is_empty() {
                break;
            }

            if let Some((header_name, header_value)) = header_line.split_once(':') {
                if header_name.eq_ignore_ascii_case("content-length") {
                    content_length = header_value
                        .trim()
                        .parse::<usize>()
                        .map_err(|error| format!("model request content length should parse: {error}"))?;
                }
            }
        }

        let mut body = vec![0_u8; content_length];
        reader
            .read_exact(&mut body)
            .map_err(|error| format!("model request body should read: {error}"))?;

        Ok(Self { method, path, body })
    }
}
