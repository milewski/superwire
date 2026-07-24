use crate::network::MCP_HTTP_GLOBAL_TIMEOUT;
use crate::McpError;
use std::cell::RefCell;
use std::fmt::{Display, Formatter};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicU8, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

static MCP_BLOCKING_EXECUTOR: OnceLock<Arc<McpBlockingExecutor>> = OnceLock::new();

pub const MCP_BLOCKING_WORKER_COUNT: usize = 8;
pub const MCP_BLOCKING_QUEUE_CAPACITY: usize = 32;
pub const MCP_BLOCKING_QUEUE_START_WINDOW: Duration = Duration::from_secs(5);
pub const MCP_BLOCKING_DISPATCH_MARGIN: Duration = Duration::from_secs(1);
pub const MCP_BLOCKING_COMPLETION_TIMEOUT: Duration =
    Duration::from_secs(MCP_HTTP_GLOBAL_TIMEOUT.as_secs() + MCP_BLOCKING_QUEUE_START_WINDOW.as_secs());

type McpBlockingJobFunction = Box<dyn FnOnce(Result<(), McpError>) + Send + 'static>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum McpBlockingOperation {
    ApproveEndpoint,
    Initialize,
    ListTools,
    CallTool,
    ReadResource,
    GetPrompt,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
enum McpBlockingJobState {
    Queued,
    Running,
    Inactive,
}

impl McpBlockingOperation {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ApproveEndpoint => "approve endpoint",
            Self::Initialize => "initialize",
            Self::ListTools => "list tools",
            Self::CallTool => "call tool",
            Self::ReadResource => "read resource",
            Self::GetPrompt => "get prompt",
        }
    }

    const fn mandatory_operation_bound(self) -> Duration {
        match self {
            Self::ApproveEndpoint => crate::MCP_HTTP_RESOLVE_TIMEOUT,
            Self::Initialize | Self::ListTools | Self::CallTool | Self::ReadResource | Self::GetPrompt => MCP_HTTP_GLOBAL_TIMEOUT,
        }
    }

    fn minimum_start_lifetime(self) -> Duration {
        self.mandatory_operation_bound() + MCP_BLOCKING_DISPATCH_MARGIN
    }

    fn completion_timeout(self) -> Duration {
        self.mandatory_operation_bound() + MCP_BLOCKING_QUEUE_START_WINDOW
    }
}

impl Display for McpBlockingOperation {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

thread_local! {
    static MCP_BLOCKING_JOB_CONTEXT: RefCell<Option<McpBlockingJobContext>> = const { RefCell::new(None) };
}

#[derive(Debug)]
struct McpBlockingJobLifecycle {
    operation: McpBlockingOperation,
    deadline: Instant,
    state: AtomicU8,
}

#[derive(Debug, Clone)]
struct McpBlockingJobContext {
    lifecycle: Arc<McpBlockingJobLifecycle>,
}

impl McpBlockingJobContext {
    fn new(operation: McpBlockingOperation, completion_timeout: Duration) -> Self {
        let deadline = Instant::now()
            .checked_add(completion_timeout)
            .expect("MCP blocking completion deadline should fit");

        Self {
            lifecycle: Arc::new(McpBlockingJobLifecycle {
                operation,
                deadline,
                state: AtomicU8::new(McpBlockingJobState::Queued as u8),
            }),
        }
    }

    fn begin(&self) -> Result<(), McpError> {
        if self.state() != McpBlockingJobState::Queued {
            return Err(McpError::BlockingOperationCancelled {
                operation: self.lifecycle.operation,
            });
        }

        self.ensure_remaining_lifetime(self.lifecycle.operation.minimum_start_lifetime())?;

        self.lifecycle
            .state
            .compare_exchange(
                McpBlockingJobState::Queued as u8,
                McpBlockingJobState::Running as u8,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .map_err(|_state| McpError::BlockingOperationCancelled {
                operation: self.lifecycle.operation,
            })?;

        Ok(())
    }

    fn authorize_http_dispatch(&self) -> Result<(), McpError> {
        if self.state() != McpBlockingJobState::Running {
            return Err(McpError::BlockingOperationCancelled {
                operation: self.lifecycle.operation,
            });
        }

        self.ensure_remaining_lifetime(MCP_HTTP_GLOBAL_TIMEOUT + MCP_BLOCKING_DISPATCH_MARGIN)
    }

    fn ensure_remaining_lifetime(&self, required_lifetime: Duration) -> Result<(), McpError> {
        let Some(remaining_lifetime) = self.lifecycle.deadline.checked_duration_since(Instant::now()) else {
            self.deactivate();

            return Err(McpError::BlockingOperationTimedOut {
                operation: self.lifecycle.operation,
            });
        };

        if remaining_lifetime < required_lifetime {
            self.deactivate();

            return Err(McpError::BlockingOperationDeadlineInsufficient {
                operation: self.lifecycle.operation,
            });
        }

        Ok(())
    }

    fn state(&self) -> McpBlockingJobState {
        match self.lifecycle.state.load(Ordering::Acquire) {
            state if state == McpBlockingJobState::Queued as u8 => McpBlockingJobState::Queued,
            state if state == McpBlockingJobState::Running as u8 => McpBlockingJobState::Running,
            _ => McpBlockingJobState::Inactive,
        }
    }

    fn deactivate(&self) {
        self.lifecycle.state.store(McpBlockingJobState::Inactive as u8, Ordering::Release);
    }
}

pub(crate) fn authorize_current_mcp_http_dispatch() -> Result<(), McpError> {
    MCP_BLOCKING_JOB_CONTEXT.with(|job_context| {
        job_context
            .borrow()
            .as_ref()
            .map_or(Ok(()), McpBlockingJobContext::authorize_http_dispatch)
    })
}
struct McpBlockingJob {
    context: McpBlockingJobContext,
    job_function: McpBlockingJobFunction,
}

impl McpBlockingJob {
    fn run(self) {
        let start_result = self.context.begin();
        let did_start = start_result.is_ok();

        if did_start {
            MCP_BLOCKING_JOB_CONTEXT.with(|job_context| {
                let replaced_context = job_context.replace(Some(self.context.clone()));
                debug_assert!(replaced_context.is_none());
            });
        }

        (self.job_function)(start_result);

        if did_start {
            MCP_BLOCKING_JOB_CONTEXT.with(|job_context| {
                job_context.replace(None);
            });
        }

        self.context.deactivate();
    }
}

struct McpBlockingExecutionGuard {
    context: McpBlockingJobContext,
}

impl Drop for McpBlockingExecutionGuard {
    fn drop(&mut self) {
        self.context.deactivate();
    }
}

#[derive(Debug)]
pub(crate) struct McpBlockingExecutor {
    worker_count: usize,
    queue_capacity: usize,
    queued_job_count: Arc<AtomicUsize>,
    sender: OnceLock<Result<SyncSender<McpBlockingJob>, String>>,
}

impl McpBlockingExecutor {
    fn new() -> Self {
        Self::with_limits(MCP_BLOCKING_WORKER_COUNT, MCP_BLOCKING_QUEUE_CAPACITY)
    }

    pub(crate) fn shared() -> Arc<Self> {
        Arc::clone(MCP_BLOCKING_EXECUTOR.get_or_init(|| Arc::new(Self::new())))
    }

    pub(crate) fn with_limits(worker_count: usize, queue_capacity: usize) -> Self {
        debug_assert!(worker_count > 0);
        debug_assert!(queue_capacity > 0);

        Self {
            worker_count,
            queue_capacity,
            queued_job_count: Arc::new(AtomicUsize::new(0)),
            sender: OnceLock::new(),
        }
    }

    pub(crate) fn execute<ResultType, Operation>(
        &self,
        operation: McpBlockingOperation,
        operation_function: Operation,
    ) -> Result<ResultType, McpError>
    where
        ResultType: Send + 'static,
        Operation: FnOnce() -> Result<ResultType, McpError> + Send + 'static,
    {
        self.execute_with_completion_timeout(operation, operation.completion_timeout(), operation_function)
    }

    #[cfg(test)]
    pub(crate) fn execute_with_timeout<ResultType, Operation>(
        &self,
        operation: McpBlockingOperation,
        completion_timeout: Duration,
        operation_function: Operation,
    ) -> Result<ResultType, McpError>
    where
        ResultType: Send + 'static,
        Operation: FnOnce() -> Result<ResultType, McpError> + Send + 'static,
    {
        self.execute_with_completion_timeout(operation, completion_timeout, operation_function)
    }

    fn execute_with_completion_timeout<ResultType, Operation>(
        &self,
        operation: McpBlockingOperation,
        completion_timeout: Duration,
        operation_function: Operation,
    ) -> Result<ResultType, McpError>
    where
        ResultType: Send + 'static,
        Operation: FnOnce() -> Result<ResultType, McpError> + Send + 'static,
    {
        let job_context = McpBlockingJobContext::new(operation, completion_timeout);
        let execution_guard = McpBlockingExecutionGuard {
            context: job_context.clone(),
        };
        let sender = self
            .sender
            .get_or_init(|| Self::start_workers(self.worker_count, self.queue_capacity, Arc::clone(&self.queued_job_count)))
            .as_ref()
            .map_err(|message| McpError::BlockingExecutorUnavailable {
                operation,
                message: message.clone(),
            })?;
        let (result_sender, result_receiver) = mpsc::sync_channel(1);
        let result_context = job_context.clone();
        let blocking_job = McpBlockingJob {
            context: job_context.clone(),
            job_function: Box::new(move |start_result| {
                let result = start_result.and_then(|()| {
                    catch_unwind(AssertUnwindSafe(operation_function))
                        .map_err(|_panic_payload| McpError::BlockingOperationPanicked { operation })
                        .and_then(std::convert::identity)
                });

                if result_sender.send(result).is_err() {
                    result_context.deactivate();
                }
            }),
        };

        self.queued_job_count.fetch_add(1, Ordering::AcqRel);

        match sender.try_send(blocking_job) {
            Ok(()) => {}
            Err(TrySendError::Full(_blocking_job)) => {
                self.queued_job_count.fetch_sub(1, Ordering::AcqRel);

                return Err(McpError::BlockingExecutorSaturated { operation });
            }
            Err(TrySendError::Disconnected(_blocking_job)) => {
                self.queued_job_count.fetch_sub(1, Ordering::AcqRel);

                return Err(McpError::BlockingExecutorUnavailable {
                    operation,
                    message: "worker queue disconnected".to_string(),
                });
            }
        }

        let result = Self::receive_result(operation, &job_context, result_receiver);
        drop(execution_guard);

        result
    }

    fn start_workers(
        worker_count: usize,
        queue_capacity: usize,
        queued_job_count: Arc<AtomicUsize>,
    ) -> Result<SyncSender<McpBlockingJob>, String> {
        let (job_sender, job_receiver) = mpsc::sync_channel::<McpBlockingJob>(queue_capacity);
        let shared_receiver = Arc::new(Mutex::new(job_receiver));

        for worker_index in 0..worker_count {
            let worker_receiver = Arc::clone(&shared_receiver);
            let worker_queued_job_count = Arc::clone(&queued_job_count);
            thread::Builder::new()
                .name(format!("superwire-mcp-{worker_index}"))
                .spawn(move || loop {
                    let blocking_job = worker_receiver.lock().expect("MCP blocking worker queue lock poisoned").recv();
                    let Ok(blocking_job) = blocking_job else {
                        break;
                    };
                    worker_queued_job_count.fetch_sub(1, Ordering::AcqRel);

                    blocking_job.run();
                })
                .map_err(|error| format!("failed to start MCP blocking worker: {error}"))?;
        }

        Ok(job_sender)
    }

    #[cfg(test)]
    pub(crate) fn queued_job_count(&self) -> usize {
        self.queued_job_count.load(Ordering::Acquire)
    }

    fn receive_result<ResultType>(
        operation: McpBlockingOperation,
        job_context: &McpBlockingJobContext,
        result_receiver: Receiver<Result<ResultType, McpError>>,
    ) -> Result<ResultType, McpError> {
        let remaining_lifetime = job_context
            .lifecycle
            .deadline
            .checked_duration_since(Instant::now())
            .unwrap_or(Duration::ZERO);
        let receive_result = move || result_receiver.recv_timeout(remaining_lifetime);
        let received_result = match tokio::runtime::Handle::try_current() {
            Ok(runtime_handle) if runtime_handle.runtime_flavor() == tokio::runtime::RuntimeFlavor::MultiThread => {
                tokio::task::block_in_place(receive_result)
            }
            Ok(_runtime_handle) => receive_result(),
            Err(_) => receive_result(),
        };

        received_result.map_err(|receive_error| match receive_error {
            mpsc::RecvTimeoutError::Timeout => McpError::BlockingOperationTimedOut { operation },
            mpsc::RecvTimeoutError::Disconnected => McpError::BlockingExecutorUnavailable {
                operation,
                message: "worker result channel disconnected".to_string(),
            },
        })?
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inactive_job_context_returns_typed_cancellation() {
        let job_context = McpBlockingJobContext::new(McpBlockingOperation::CallTool, MCP_BLOCKING_COMPLETION_TIMEOUT);
        job_context.deactivate();
        let error = job_context.begin().expect_err("inactive job should not start");

        assert!(matches!(
            error,
            McpError::BlockingOperationCancelled {
                operation: McpBlockingOperation::CallTool
            }
        ));
    }

    #[test]
    fn saturated_executor_returns_typed_error_without_waiting_for_running_jobs() {
        let blocking_executor = Arc::new(McpBlockingExecutor::with_limits(1, 1));
        let (release_sender, release_receiver) = mpsc::channel::<()>();
        let shared_release_receiver = Arc::new(Mutex::new(release_receiver));
        let (started_sender, started_receiver) = mpsc::channel();
        let first_executor = Arc::clone(&blocking_executor);
        let first_release_receiver = Arc::clone(&shared_release_receiver);
        let first_thread = thread::spawn(move || {
            first_executor.execute(McpBlockingOperation::CallTool, move || {
                started_sender.send(()).expect("worker start should be observable");
                first_release_receiver
                    .lock()
                    .expect("release receiver lock should not poison")
                    .recv()
                    .expect("first worker should be released");
                Ok(())
            })
        });
        started_receiver.recv().expect("first operation should start");

        let second_executor = Arc::clone(&blocking_executor);
        let second_release_receiver = Arc::clone(&shared_release_receiver);
        let second_thread = thread::spawn(move || {
            second_executor.execute(McpBlockingOperation::ReadResource, move || {
                second_release_receiver
                    .lock()
                    .expect("release receiver lock should not poison")
                    .recv()
                    .expect("second worker should be released");
                Ok(())
            })
        });
        while blocking_executor.queued_job_count() != 1 {
            thread::yield_now();
        }

        let saturation_error = blocking_executor
            .execute(McpBlockingOperation::GetPrompt, || Ok(()))
            .expect_err("full worker queue should reject another operation");

        assert!(matches!(
            saturation_error,
            McpError::BlockingExecutorSaturated {
                operation: McpBlockingOperation::GetPrompt
            }
        ));
        release_sender.send(()).expect("first worker should be released");
        release_sender.send(()).expect("second worker should be released");
        first_thread
            .join()
            .expect("first caller thread should finish")
            .expect("first operation should succeed");
        second_thread
            .join()
            .expect("second caller thread should finish")
            .expect("second operation should succeed");
    }
}
