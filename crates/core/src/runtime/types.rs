use engine_ai_agent::Context;
use serde_json::Value;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct WorkflowExecutionResult {
    pub output: Value,
    pub agent_outputs_by_name: HashMap<String, Value>,
    pub agent_contexts_by_name: HashMap<String, Context>,
}

pub(crate) struct ExecutionScope<'scope> {
    pub(crate) input_values: &'scope Value,
    pub(crate) secret_values: &'scope Value,
    pub(crate) agent_outputs_by_name: &'scope HashMap<String, Value>,
}

pub(crate) struct ModelBinding {
    pub(crate) provider_name: String,
    pub(crate) model_name: String,
}
