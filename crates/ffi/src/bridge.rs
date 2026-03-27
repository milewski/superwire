use std::marker::PhantomData;

use engine_agent::AgentError;
use engine_core::WorkflowRuntimeError;

use crate::error::FfiError;
use crate::types::{FfiInvocation, FfiResult};

#[derive(Debug, Default)]
pub struct EngineFfi {
    workflow_runtime_marker: PhantomData<WorkflowRuntimeError>,
    agent_marker: PhantomData<AgentError>,
}

impl EngineFfi {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn invoke(&self, invocation: FfiInvocation) -> Result<FfiResult, FfiError> {
        let operation = invocation.operation;

        Err(FfiError::NotImplemented { operation })
    }
}
